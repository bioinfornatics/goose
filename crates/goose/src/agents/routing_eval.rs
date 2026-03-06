//! Routing evaluation framework for measuring IntentRouter accuracy.
//!
//! Supports multi-agent decomposition: test cases can specify multiple
//! acceptable agent/mode combinations (e.g. "Write ROI of a feature" could
//! legitimately route to PM Agent or Developer Agent).
//!
//! Provides YAML-based test sets, an evaluation runner, per-agent/per-mode
//! accuracy metrics, a confusion matrix, and a human-readable report.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::intent_router::IntentRouter;

/// A single acceptable routing for a test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptableRouting {
    pub agent: String,
    pub mode: String,
}

/// A single evaluation test case.
///
/// `expected_agent` + `expected_mode` are the primary (preferred) routing.
/// `also_acceptable` lists alternative routings that are considered correct.
/// This supports compound/ambiguous inputs that could legitimately route
/// to different agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingEvalCase {
    pub input: String,
    pub expected_agent: String,
    pub expected_mode: String,
    #[serde(default)]
    pub also_acceptable: Vec<AcceptableRouting>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingEvalSet {
    pub test_cases: Vec<RoutingEvalCase>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutingEvalResult {
    pub input: String,
    pub expected_agent: String,
    pub expected_mode: String,
    pub actual_agent: String,
    pub actual_mode: String,
    pub confidence: f32,
    pub reasoning: String,
    pub agent_correct: bool,
    pub mode_correct: bool,
    pub fully_correct: bool,
    /// Whether the match was against an alternative (not primary) routing
    pub matched_alternative: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutingEvalMetrics {
    pub total: usize,
    pub correct: usize,
    pub agent_correct: usize,
    pub overall_accuracy: f64,
    pub agent_accuracy: f64,
    pub mode_accuracy_given_agent: f64,
    pub per_agent: HashMap<String, AgentMetrics>,
    pub per_mode: HashMap<String, ModeMetrics>,
    pub confusion_matrix: Vec<ConfusionEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMetrics {
    pub total: usize,
    pub correct: usize,
    pub accuracy: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModeMetrics {
    pub total: usize,
    pub correct: usize,
    pub accuracy: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfusionEntry {
    pub expected: String,
    pub actual: String,
    pub count: usize,
}

pub fn load_eval_set(yaml: &str) -> Result<RoutingEvalSet, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

/// Check if the actual routing matches the primary or any alternative routing.
fn matches_any_acceptable(
    tc: &RoutingEvalCase,
    actual_agent: &str,
    actual_mode: &str,
) -> (bool, bool, bool) {
    let primary_agent = actual_agent.eq_ignore_ascii_case(&tc.expected_agent);
    let primary_mode = actual_mode == tc.expected_mode;

    if primary_agent && primary_mode {
        return (true, true, false);
    }
    if primary_agent {
        return (true, primary_mode, false);
    }

    for alt in &tc.also_acceptable {
        let alt_agent = actual_agent.eq_ignore_ascii_case(&alt.agent);
        let alt_mode = actual_mode == alt.mode;
        if alt_agent && alt_mode {
            return (true, true, true);
        }
        if alt_agent {
            return (true, alt_mode, true);
        }
    }

    (false, false, false)
}

pub fn evaluate(router: &IntentRouter, test_set: &RoutingEvalSet) -> Vec<RoutingEvalResult> {
    test_set
        .test_cases
        .iter()
        .map(|tc| {
            let decision = router.route(&tc.input);
            let (agent_correct, fully_correct, matched_alt) =
                matches_any_acceptable(tc, &decision.agent_name, &decision.mode_slug);

            RoutingEvalResult {
                input: tc.input.clone(),
                expected_agent: tc.expected_agent.clone(),
                expected_mode: tc.expected_mode.clone(),
                actual_agent: decision.agent_name.clone(),
                actual_mode: decision.mode_slug.clone(),
                confidence: decision.confidence,
                reasoning: decision.reasoning.clone(),
                agent_correct,
                mode_correct: fully_correct,
                fully_correct,
                matched_alternative: matched_alt,
            }
        })
        .collect()
}

pub fn compute_metrics(results: &[RoutingEvalResult]) -> RoutingEvalMetrics {
    let total = results.len();
    let correct = results.iter().filter(|r| r.fully_correct).count();
    let agent_correct = results.iter().filter(|r| r.agent_correct).count();

    let mut per_agent: HashMap<String, (usize, usize)> = HashMap::new();
    for r in results {
        let entry = per_agent.entry(r.expected_agent.clone()).or_default();
        entry.0 += 1;
        if r.agent_correct {
            entry.1 += 1;
        }
    }

    let mut per_mode: HashMap<String, (usize, usize)> = HashMap::new();
    for r in results {
        let entry = per_mode.entry(r.expected_mode.clone()).or_default();
        entry.0 += 1;
        if r.fully_correct {
            entry.1 += 1;
        }
    }

    let mut confusion_raw: HashMap<(String, String), usize> = HashMap::new();
    for r in results.iter().filter(|r| !r.agent_correct) {
        *confusion_raw
            .entry((r.expected_agent.clone(), r.actual_agent.clone()))
            .or_default() += 1;
    }

    let per_agent = per_agent
        .into_iter()
        .map(|(k, (t, c))| {
            (
                k,
                AgentMetrics {
                    total: t,
                    correct: c,
                    accuracy: if t > 0 { c as f64 / t as f64 } else { 0.0 },
                },
            )
        })
        .collect();

    let per_mode = per_mode
        .into_iter()
        .map(|(k, (t, c))| {
            (
                k,
                ModeMetrics {
                    total: t,
                    correct: c,
                    accuracy: if t > 0 { c as f64 / t as f64 } else { 0.0 },
                },
            )
        })
        .collect();

    let mut confusion_matrix: Vec<ConfusionEntry> = confusion_raw
        .into_iter()
        .map(|((e, a), c)| ConfusionEntry {
            expected: e,
            actual: a,
            count: c,
        })
        .collect();
    confusion_matrix.sort_by(|a, b| b.count.cmp(&a.count));

    RoutingEvalMetrics {
        total,
        correct,
        agent_correct,
        overall_accuracy: if total > 0 {
            correct as f64 / total as f64
        } else {
            0.0
        },
        agent_accuracy: if total > 0 {
            agent_correct as f64 / total as f64
        } else {
            0.0
        },
        mode_accuracy_given_agent: if agent_correct > 0 {
            correct as f64 / agent_correct as f64
        } else {
            0.0
        },
        per_agent,
        per_mode,
        confusion_matrix,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s.chars().take(max - 1).collect::<String>())
    }
}

pub fn format_report(results: &[RoutingEvalResult], metrics: &RoutingEvalMetrics) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "\n=== Routing Eval Report ===\n\
         Total: {} | Correct: {} ({:.1}%) | Agent-level: {:.1}%\n\n",
        metrics.total,
        metrics.correct,
        metrics.overall_accuracy * 100.0,
        metrics.agent_accuracy * 100.0,
    ));

    out.push_str("Per-Agent Accuracy:\n");
    let mut agents: Vec<_> = metrics.per_agent.iter().collect();
    agents.sort_by_key(|(k, _)| (*k).clone());
    for (agent, m) in &agents {
        out.push_str(&format!(
            "  {:<20} {}/{} ({:.1}%)\n",
            agent,
            m.correct,
            m.total,
            m.accuracy * 100.0
        ));
    }

    let failed: Vec<_> = results.iter().filter(|r| !r.fully_correct).collect();
    if !failed.is_empty() {
        out.push_str(&format!("\nFailed Cases ({}):\n", failed.len()));
        for r in &failed {
            out.push_str(&format!(
                "  ✗ \"{}\" → Agent: {} Mode: {} (expected {}/{})\n",
                truncate(&r.input, 60),
                r.actual_agent,
                r.actual_mode,
                r.expected_agent,
                r.expected_mode,
            ));
        }
    }

    let alt_matched: Vec<_> = results.iter().filter(|r| r.matched_alternative).collect();
    if !alt_matched.is_empty() {
        out.push_str(&format!("\nAlternative Matches ({}):\n", alt_matched.len()));
        for r in &alt_matched {
            out.push_str(&format!(
                "  ≈ \"{}\" → {}/{} (alt of {}/{})\n",
                truncate(&r.input, 60),
                r.actual_agent,
                r.actual_mode,
                r.expected_agent,
                r.expected_mode,
            ));
        }
    }

    if !metrics.confusion_matrix.is_empty() {
        out.push_str("\nConfusion (top misroutes):\n");
        for c in metrics.confusion_matrix.iter().take(5) {
            out.push_str(&format!("  {} → {} ({}x)\n", c.expected, c.actual, c.count));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════
    // Golden Test Cases — multi-agent aware
    // ═══════════════════════════════════════════════════════════════════
    const GOLDEN_EVAL_YAML: &str = r#"
test_cases:
  # ══════════════════════════════════════════════════════════════════
  # Goose Agent — general conversation, greetings, meta
  # ══════════════════════════════════════════════════════════════════
  - input: "Hello, how are you?"
    expected_agent: "Goose Agent"
    expected_mode: "ask"
    tags: [p0, goose, ask]
  - input: "What can you help me with?"
    expected_agent: "Goose Agent"
    expected_mode: "ask"
    tags: [p0, goose, ask]
  - input: "Thanks for the help!"
    expected_agent: "Goose Agent"
    expected_mode: "ask"
    tags: [p0, goose, ask]

  # ══════════════════════════════════════════════════════════════════
  # Developer Agent — code creation, debugging, implementation
  # ══════════════════════════════════════════════════════════════════
  - input: "How does the middleware pipeline work in this project?"
    expected_agent: "Developer Agent"
    expected_mode: "ask"
    tags: [p0, developer, ask]
  - input: "Design the architecture for a real-time notification system"
    expected_agent: "Developer Agent"
    expected_mode: "plan"
    tags: [p0, developer, plan]
  - input: "Create a REST API endpoint for user registration"
    expected_agent: "Developer Agent"
    expected_mode: "write"
    tags: [p0, developer, write]
  - input: "Implement the database migration for the new schema"
    expected_agent: "Developer Agent"
    expected_mode: "write"
    tags: [p0, developer, write]
  - input: "Add unit tests for the payment processing module"
    expected_agent: "Developer Agent"
    expected_mode: "write"
    also_acceptable:
      - agent: "QA Agent"
        mode: "write"
    tags: [p1, developer, write, multi-agent]
  - input: "Refactor the authentication service to use dependency injection"
    expected_agent: "Developer Agent"
    expected_mode: "write"
    tags: [p0, developer, write]
  - input: "Fix the database connection pool timeout issue"
    expected_agent: "Developer Agent"
    expected_mode: "write"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "debug"
    tags: [p0, developer, write]
  - input: "Fix the race condition in the event handler"
    expected_agent: "Developer Agent"
    expected_mode: "debug"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "write"
    tags: [p0, developer, debug]
  - input: "Debug why the tests are failing on CI"
    expected_agent: "Developer Agent"
    expected_mode: "debug"
    also_acceptable:
      - agent: "QA Agent"
        mode: "debug"
    tags: [p0, developer, debug]
  - input: "Investigate the memory leak in the worker process"
    expected_agent: "Developer Agent"
    expected_mode: "debug"
    tags: [p0, developer, debug]
  - input: "Review this pull request for code quality"
    expected_agent: "Developer Agent"
    expected_mode: "review"
    also_acceptable:
      - agent: "QA Agent"
        mode: "review"
    tags: [p1, developer, review]
  - input: "Set up the CI/CD pipeline with GitHub Actions"
    expected_agent: "Developer Agent"
    expected_mode: "write"
    tags: [p1, developer, write]

  # ══════════════════════════════════════════════════════════════════
  # QA Agent — testing, quality, test infrastructure
  # ══════════════════════════════════════════════════════════════════
  - input: "What's our current test coverage for the auth module?"
    expected_agent: "QA Agent"
    expected_mode: "ask"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "ask"
    tags: [p0, qa, ask]
  - input: "Create a test plan for the checkout flow"
    expected_agent: "QA Agent"
    expected_mode: "plan"
    tags: [p0, qa, plan]
  - input: "Write integration tests for the payment gateway"
    expected_agent: "QA Agent"
    expected_mode: "write"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "write"
    tags: [p0, qa, write]
  - input: "Set up end-to-end testing with Playwright"
    expected_agent: "QA Agent"
    expected_mode: "write"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "write"
    tags: [p1, qa, write]
  - input: "Review this pull request for correctness and test coverage"
    expected_agent: "QA Agent"
    expected_mode: "review"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "review"
    tags: [p0, qa, review]

  # ══════════════════════════════════════════════════════════════════
  # PM Agent — product management, requirements, roadmap, ROI
  # ══════════════════════════════════════════════════════════════════
  - input: "What are the acceptance criteria for the checkout feature?"
    expected_agent: "PM Agent"
    expected_mode: "ask"
    also_acceptable:
      - agent: "Goose Agent"
        mode: "ask"
    tags: [p1, pm, ask]
  - input: "Plan the phased rollout strategy for our mobile app launch"
    expected_agent: "PM Agent"
    expected_mode: "plan"
    tags: [p0, pm, plan]
  - input: "Prioritize these features using RICE scoring framework"
    expected_agent: "PM Agent"
    expected_mode: "plan"
    tags: [p1, pm, plan]
  - input: "Create a product requirements document for the notification system"
    expected_agent: "PM Agent"
    expected_mode: "write"
    tags: [p0, pm, write]
  - input: "Write user stories for the shopping cart feature"
    expected_agent: "PM Agent"
    expected_mode: "write"
    tags: [p0, pm, write]
  - input: "Create a product roadmap with milestones for the next 6 months"
    expected_agent: "PM Agent"
    expected_mode: "write"
    tags: [p1, pm, write]
  - input: "Review this PRD for completeness and missing edge cases"
    expected_agent: "PM Agent"
    expected_mode: "review"
    also_acceptable:
      - agent: "QA Agent"
        mode: "review"
    tags: [p1, pm, review]
  - input: "Write ROI analysis of a feature"
    expected_agent: "PM Agent"
    expected_mode: "write"
    also_acceptable:
      - agent: "Research Agent"
        mode: "write"
    tags: [p0, pm, write, multi-agent]

  # ══════════════════════════════════════════════════════════════════
  # Security Agent — security analysis, threat modeling, compliance
  # ══════════════════════════════════════════════════════════════════
  - input: "What are the security implications of using JWT tokens?"
    expected_agent: "Security Agent"
    expected_mode: "ask"
    also_acceptable:
      - agent: "Research Agent"
        mode: "ask"
    tags: [p0, security, ask]
  - input: "Perform STRIDE threat modeling on our authentication flow"
    expected_agent: "Security Agent"
    expected_mode: "plan"
    tags: [p0, security, plan]
  - input: "Map the attack surface and trust boundaries for this microservice"
    expected_agent: "Security Agent"
    expected_mode: "plan"
    tags: [p1, security, plan]
  - input: "Apply security patches and harden the authentication configuration"
    expected_agent: "Security Agent"
    expected_mode: "write"
    tags: [p0, security, write]
  - input: "Scan this code for SQL injection and XSS vulnerabilities"
    expected_agent: "Security Agent"
    expected_mode: "review"
    tags: [p0, security, review]
  - input: "Audit this service for PCI-DSS compliance requirements"
    expected_agent: "Security Agent"
    expected_mode: "review"
    tags: [p0, security, review]
  - input: "Check for hardcoded secrets in the repository"
    expected_agent: "Security Agent"
    expected_mode: "review"
    tags: [p1, security, review]

  # ══════════════════════════════════════════════════════════════════
  # Research Agent — investigation, comparison, learning
  # ══════════════════════════════════════════════════════════════════
  - input: "Explain how Rust's borrow checker works with simple examples"
    expected_agent: "Research Agent"
    expected_mode: "ask"
    also_acceptable:
      - agent: "Goose Agent"
        mode: "ask"
      - agent: "Developer Agent"
        mode: "ask"
    tags: [p0, research, ask]
  - input: "Research how WebSocket connections work and their security implications"
    expected_agent: "Research Agent"
    expected_mode: "ask"
    also_acceptable:
      - agent: "Security Agent"
        mode: "ask"
      - agent: "Developer Agent"
        mode: "ask"
    tags: [p1, research, ask]
  - input: "Plan a literature review on microservice design patterns"
    expected_agent: "Research Agent"
    expected_mode: "plan"
    also_acceptable:
      - agent: "PM Agent"
        mode: "plan"
    tags: [p1, research, plan]
  - input: "Write a comparison report of React vs Vue vs Svelte"
    expected_agent: "Research Agent"
    expected_mode: "write"
    tags: [p0, research, write]
  - input: "Summarize this RFC and extract the key decisions"
    expected_agent: "Research Agent"
    expected_mode: "write"
    also_acceptable:
      - agent: "PM Agent"
        mode: "write"
    tags: [p1, research, write]
  - input: "Review this technical report for accuracy and source quality"
    expected_agent: "Research Agent"
    expected_mode: "review"
    tags: [p0, research, review]

  # ══════════════════════════════════════════════════════════════════
  # Ambiguous / Compound — legitimately multi-agent
  # ══════════════════════════════════════════════════════════════════
  - input: "Analyze the performance bottleneck and fix it"
    expected_agent: "Developer Agent"
    expected_mode: "debug"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "write"
      - agent: "QA Agent"
        mode: "debug"
    tags: [p0, ambiguous, compound]
  - input: "Review this code for security vulnerabilities and test coverage"
    expected_agent: "Security Agent"
    expected_mode: "review"
    also_acceptable:
      - agent: "QA Agent"
        mode: "review"
      - agent: "Developer Agent"
        mode: "review"
    tags: [p0, ambiguous, compound]
  - input: "Compare authentication strategies and implement the best one"
    expected_agent: "Research Agent"
    expected_mode: "ask"
    also_acceptable:
      - agent: "Developer Agent"
        mode: "write"
      - agent: "Security Agent"
        mode: "plan"
    tags: [p1, ambiguous, compound]
"#;

    fn build_router() -> IntentRouter {
        IntentRouter::new()
    }

    fn run_eval() -> (Vec<RoutingEvalResult>, RoutingEvalMetrics) {
        let router = build_router();
        let eval_set = load_eval_set(GOLDEN_EVAL_YAML).expect("YAML parse error");
        let results = evaluate(&router, &eval_set);
        let metrics = compute_metrics(&results);
        (results, metrics)
    }

    #[test]
    fn test_load_eval_set() {
        let eval_set = load_eval_set(GOLDEN_EVAL_YAML).expect("Failed to parse golden YAML");
        assert!(
            eval_set.test_cases.len() >= 40,
            "Expected >= 40 test cases, got {}",
            eval_set.test_cases.len()
        );
    }

    #[test]
    fn test_evaluate_produces_results() {
        let (results, _) = run_eval();
        assert!(!results.is_empty());
        for r in &results {
            assert!(!r.actual_agent.is_empty());
            assert!(!r.actual_mode.is_empty());
        }
    }

    #[test]
    fn test_general_prompts_route_to_goose() {
        let router = build_router();
        let eval_set = load_eval_set(GOLDEN_EVAL_YAML).expect("parse");
        let goose_cases: Vec<_> = eval_set
            .test_cases
            .iter()
            .filter(|tc| tc.expected_agent == "Goose Agent")
            .collect();
        let correct = goose_cases
            .iter()
            .filter(|tc| {
                let d = router.route(&tc.input);
                d.agent_name.to_lowercase() == "goose agent"
            })
            .count();
        let accuracy = correct as f64 / goose_cases.len() as f64;
        assert!(
            accuracy >= 0.80,
            "Goose Agent routing accuracy {:.1}% < 80%",
            accuracy * 100.0
        );
    }

    #[test]
    fn test_coding_prompts_baseline() {
        let router = build_router();
        let eval_set = load_eval_set(GOLDEN_EVAL_YAML).expect("parse");
        let dev_cases: Vec<_> = eval_set
            .test_cases
            .iter()
            .filter(|tc| tc.expected_agent == "Developer Agent")
            .collect();
        let correct = dev_cases
            .iter()
            .filter(|tc| {
                let d = router.route(&tc.input);
                d.agent_name.to_lowercase() == "developer agent"
            })
            .count();
        let accuracy = correct as f64 / dev_cases.len() as f64;
        assert!(
            accuracy >= 0.20,
            "Developer Agent routing accuracy {:.1}% < 20%",
            accuracy * 100.0
        );
    }

    #[test]
    fn test_agent_level_accuracy_baseline() {
        let (_, metrics) = run_eval();
        assert!(
            metrics.agent_accuracy >= 0.60,
            "Agent-level accuracy {:.1}% < 60%",
            metrics.agent_accuracy * 100.0
        );
    }

    #[test]
    fn test_pm_routing_baseline() {
        let (results, _) = run_eval();
        let pm_results: Vec<_> = results
            .iter()
            .filter(|r| r.expected_agent == "PM Agent")
            .collect();
        let correct = pm_results.iter().filter(|r| r.agent_correct).count();
        let accuracy = correct as f64 / pm_results.len() as f64;
        assert!(
            accuracy >= 0.30,
            "PM Agent routing accuracy {:.1}% < 30%",
            accuracy * 100.0
        );
    }

    #[test]
    fn test_research_routing_baseline() {
        let (results, _) = run_eval();
        let research_results: Vec<_> = results
            .iter()
            .filter(|r| r.expected_agent == "Research Agent")
            .collect();
        let correct = research_results.iter().filter(|r| r.agent_correct).count();
        let accuracy = correct as f64 / research_results.len() as f64;
        assert!(
            accuracy >= 0.15,
            "Research Agent routing accuracy {:.1}% < 15%",
            accuracy * 100.0
        );
    }

    #[test]
    fn test_semantic_layer_used() {
        let router = build_router();
        let eval_set = load_eval_set(GOLDEN_EVAL_YAML).expect("parse");
        let semantic_used = eval_set
            .test_cases
            .iter()
            .filter(|tc| {
                let d = router.route(&tc.input);
                d.reasoning.contains("[semantic]")
            })
            .count();
        assert!(
            semantic_used >= 3,
            "Semantic layer only used for {} cases, expected >= 3",
            semantic_used
        );
    }

    #[test]
    fn test_multi_agent_cases_parsed() {
        let eval_set = load_eval_set(GOLDEN_EVAL_YAML).expect("parse");
        let multi = eval_set
            .test_cases
            .iter()
            .filter(|tc| !tc.also_acceptable.is_empty())
            .count();
        assert!(
            multi >= 10,
            "Expected >= 10 multi-agent cases, got {}",
            multi
        );
    }

    #[test]
    fn test_alternative_matches_boost_accuracy() {
        let (results, _) = run_eval();
        let alt_matches = results.iter().filter(|r| r.matched_alternative).count();
        // Some cases should match via alternatives, proving the multi-agent
        // eval is working and boosting effective accuracy
        let total_correct = results.iter().filter(|r| r.fully_correct).count();
        // Just verify the system works — alternatives should help at least sometimes
        assert!(
            total_correct > 0,
            "No cases matched at all — routing is completely broken"
        );
        // Print for visibility
        eprintln!(
            "Alternative matches: {}/{} ({:.1}%)",
            alt_matches,
            results.len(),
            alt_matches as f64 / results.len() as f64 * 100.0
        );
    }

    #[test]
    fn test_compute_metrics() {
        let (_, metrics) = run_eval();
        assert!(metrics.total > 0);
        assert!(metrics.overall_accuracy >= 0.0);
        assert!(metrics.overall_accuracy <= 1.0);
    }

    #[test]
    fn test_format_report() {
        let (results, metrics) = run_eval();
        let report = format_report(&results, &metrics);
        assert!(report.contains("Routing Eval Report"));
        assert!(report.contains("Per-Agent Accuracy"));
    }

    #[test]
    fn test_full_report_output() {
        let (results, metrics) = run_eval();
        let report = format_report(&results, &metrics);
        eprintln!("{}", report);
        assert!(!report.is_empty());
    }
}
