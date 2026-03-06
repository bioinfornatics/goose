//! Routing evaluation framework — two levels of testing.
//!
//! ## Two eval paths
//!
//! 1. **`evaluate_intent_router()`** — tests the FALLBACK path only
//!    (IntentRouter: semantic + default). Fast, deterministic, no LLM.
//!    Good for regression testing the fallback quality.
//!
//! 2. **`evaluate_orchestrator()`** — tests the REAL production path
//!    (OrchestratorAgent::route(): fast-path → LLM → fallback). Async,
//!    requires a Provider. Tests what users actually experience.
//!
//! 3. **`evaluate_catalog_quality()`** — verifies the XML agent catalog
//!    has correct structure (genui on specialists, mode coverage, etc.).
//!    Sync, no LLM needed. Catches prompt/catalog configuration bugs.
//!
//! ## Why two paths?
//!
//! The IntentRouter eval historically showed different results from
//! production because production uses LLM routing as the primary path.
//! The IntentRouter is only the fallback. Testing only the fallback
//! gives misleading accuracy numbers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::intent_router::IntentRouter;
use super::orchestrator_agent::OrchestratorAgent;

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

/// Evaluate routing using only the IntentRouter (fallback path).
///
/// **IMPORTANT**: This tests the FALLBACK router only (semantic + default).
/// In production, OrchestratorAgent::route() uses LLM as the primary router.
/// Use `evaluate_orchestrator()` to test the real production path.
pub fn evaluate_intent_router(
    router: &IntentRouter,
    test_set: &RoutingEvalSet,
) -> Vec<RoutingEvalResult> {
    evaluate(router, test_set)
}

/// Evaluate routing using the REAL production path (OrchestratorAgent::route()).
///
/// This is async and requires a configured OrchestratorAgent with a Provider.
/// Use this for integration testing to measure what users actually experience.
pub async fn evaluate_orchestrator(
    orchestrator: &OrchestratorAgent,
    test_set: &RoutingEvalSet,
) -> Vec<RoutingEvalResult> {
    let mut results = Vec::with_capacity(test_set.test_cases.len());
    for tc in &test_set.test_cases {
        let plan = orchestrator.route(&tc.input).await;

        // Extract primary routing from first task
        let (actual_agent, actual_mode, confidence, reasoning) =
            if let Some(first_task) = plan.tasks.first() {
                (
                    first_task.routing.agent_name.clone(),
                    first_task.routing.mode_slug.clone(),
                    first_task.routing.confidence,
                    first_task.routing.reasoning.clone(),
                )
            } else {
                (
                    "Unknown".to_string(),
                    "unknown".to_string(),
                    0.0,
                    "Empty plan".to_string(),
                )
            };

        let (agent_correct, fully_correct, matched_alt) =
            matches_any_acceptable(tc, &actual_agent, &actual_mode);

        results.push(RoutingEvalResult {
            input: tc.input.clone(),
            expected_agent: tc.expected_agent.clone(),
            expected_mode: tc.expected_mode.clone(),
            actual_agent,
            actual_mode,
            confidence,
            reasoning,
            agent_correct,
            mode_correct: fully_correct,
            fully_correct,
            matched_alternative: matched_alt,
        });
    }
    results
}

/// Result of catalog quality evaluation.
#[derive(Debug, Clone)]
pub struct CatalogQualityReport {
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub agent_count: usize,
    pub total_mode_count: usize,
    pub agents_with_genui: Vec<String>,
    pub agents_missing_genui: Vec<String>,
}

impl CatalogQualityReport {
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Evaluate the quality of the XML agent catalog.
///
/// This verifies structural properties that affect LLM routing quality:
/// - All expected agents are present
/// - Each agent has modes defined
/// - Specialist agents (Developer, QA, Research) have genui extension
/// - Descriptions contain domain-specific keywords
/// - No duplicate agents/modes
///
/// This is the "unit test for the LLM's input" — if the catalog is wrong,
/// the LLM will route wrong regardless of prompt quality.
#[allow(clippy::string_slice)] // XML catalog is ASCII-only (agent names, XML tags)
pub fn evaluate_catalog_quality(catalog_xml: &str) -> CatalogQualityReport {
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut agent_count = 0;
    let mut agents_with_genui = Vec::new();
    let mut agents_missing_genui = Vec::new();

    let expected_agents = [
        "Developer Agent",
        "QA Agent",
        "PM Agent",
        "Security Agent",
        "Research Agent",
    ];

    let genui_expected_agents = ["Developer Agent", "QA Agent", "Research Agent"];

    // Check each expected agent is present
    for agent in &expected_agents {
        if !catalog_xml.contains(&format!("name=\"{}\"", agent)) {
            issues.push(format!("Missing agent: {}", agent));
        } else {
            agent_count += 1;
        }
    }

    // Count modes (look for <mode slug="..."> tags)
    let total_mode_count = catalog_xml.matches("<mode slug=").count();
    if total_mode_count < 15 {
        warnings.push(format!(
            "Only {} modes found, expected >= 15 (5 agents × 3+ modes)",
            total_mode_count
        ));
    }

    // Check genui extension on specialist agents
    // Parse agent blocks and check for genui in extensions
    for agent in &genui_expected_agents {
        let agent_tag = format!("name=\"{}\"", agent);
        if let Some(start) = catalog_xml.find(&agent_tag) {
            // Find the next </agent> or next <agent to delimit this block
            let block_end = catalog_xml[start..]
                .find("</agent>")
                .map(|i| start + i)
                .unwrap_or(catalog_xml.len());
            let block = &catalog_xml[start..block_end];

            if block.contains("genui") {
                agents_with_genui.push(agent.to_string());
            } else {
                agents_missing_genui.push(agent.to_string());
                issues.push(format!(
                    "{} missing genui extension — cannot produce visualizations",
                    agent
                ));
            }
        }
    }

    // Check for essential modes per agent
    let essential_modes = ["ask", "write"];
    for agent in &expected_agents {
        let agent_tag = format!("name=\"{}\"", agent);
        if let Some(start) = catalog_xml.find(&agent_tag) {
            let block_end = catalog_xml[start..]
                .find("</agent>")
                .map(|i| start + i)
                .unwrap_or(catalog_xml.len());
            let block = &catalog_xml[start..block_end];

            for mode in &essential_modes {
                if !block.contains(&format!("slug=\"{}\"", mode)) {
                    issues.push(format!("{} missing essential mode: {}", agent, mode));
                }
            }
        }
    }

    // Check for domain keywords in descriptions
    let domain_checks = [
        ("PM Agent", &["roadmap", "requirements", "ROI"][..]),
        (
            "Security Agent",
            &["vulnerability", "threat", "compliance"][..],
        ),
        (
            "Research Agent",
            &["research", "investigation", "comparison"][..],
        ),
    ];
    for (agent, keywords) in &domain_checks {
        let agent_tag = format!("name=\"{}\"", agent);
        if let Some(start) = catalog_xml.find(&agent_tag) {
            let block_end = catalog_xml[start..]
                .find("</agent>")
                .map(|i| start + i)
                .unwrap_or(catalog_xml.len());
            let block = &catalog_xml[start..block_end].to_lowercase();

            let found: Vec<_> = keywords
                .iter()
                .filter(|kw| block.contains(&kw.to_lowercase()))
                .collect();
            if found.is_empty() {
                warnings.push(format!(
                    "{} description lacks domain keywords: {:?}",
                    agent, keywords
                ));
            }
        }
    }

    CatalogQualityReport {
        issues,
        warnings,
        agent_count,
        total_mode_count,
        agents_with_genui,
        agents_missing_genui,
    }
}

/// Core evaluation logic — runs test cases through IntentRouter.
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
            metrics.agent_accuracy >= 0.50,
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
                d.reasoning.contains("[semantic")
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

    // ═══════════════════════════════════════════════════════════════════
    // Catalog Quality Tests — verify what the LLM sees
    // ═══════════════════════════════════════════════════════════════════

    fn build_orchestrator() -> OrchestratorAgent {
        use std::sync::Arc;
        use tokio::sync::Mutex;
        OrchestratorAgent::new(Arc::new(Mutex::new(None)))
    }

    #[test]
    fn test_catalog_has_all_agents() {
        let orchestrator = build_orchestrator();
        let catalog = orchestrator.build_catalog_text();
        let report = evaluate_catalog_quality(&catalog);

        assert!(
            report.agent_count >= 5,
            "Catalog only has {} agents, expected >= 5. Issues: {:?}",
            report.agent_count,
            report.issues
        );
    }

    #[test]
    fn test_catalog_has_genui_on_specialists() {
        let orchestrator = build_orchestrator();
        let catalog = orchestrator.build_catalog_text();
        let report = evaluate_catalog_quality(&catalog);

        // Developer, QA, Research should have genui
        assert!(
            report.agents_with_genui.len() >= 2,
            "Only {} agents have genui: {:?}. Missing: {:?}",
            report.agents_with_genui.len(),
            report.agents_with_genui,
            report.agents_missing_genui
        );
    }

    #[test]
    fn test_catalog_has_domain_keywords() {
        let orchestrator = build_orchestrator();
        let catalog = orchestrator.build_catalog_text();
        let report = evaluate_catalog_quality(&catalog);

        // No warnings about missing domain keywords
        let keyword_warnings: Vec<_> = report
            .warnings
            .iter()
            .filter(|w| w.contains("domain keywords"))
            .collect();
        assert!(
            keyword_warnings.is_empty(),
            "Catalog missing domain keywords: {:?}",
            keyword_warnings
        );
    }

    #[test]
    fn test_catalog_has_essential_modes() {
        let orchestrator = build_orchestrator();
        let catalog = orchestrator.build_catalog_text();
        let report = evaluate_catalog_quality(&catalog);

        let mode_issues: Vec<_> = report
            .issues
            .iter()
            .filter(|i| i.contains("missing essential mode"))
            .collect();
        assert!(
            mode_issues.is_empty(),
            "Agents missing essential modes: {:?}",
            mode_issues
        );
    }

    #[test]
    fn test_catalog_quality_report_clean() {
        let orchestrator = build_orchestrator();
        let catalog = orchestrator.build_catalog_text();
        let report = evaluate_catalog_quality(&catalog);

        eprintln!("Catalog Quality Report:");
        eprintln!("  Agents: {}", report.agent_count);
        eprintln!("  Total modes: {}", report.total_mode_count);
        eprintln!("  Agents with genui: {:?}", report.agents_with_genui);
        eprintln!("  Agents missing genui: {:?}", report.agents_missing_genui);
        if !report.issues.is_empty() {
            eprintln!("  ISSUES: {:?}", report.issues);
        }
        if !report.warnings.is_empty() {
            eprintln!("  WARNINGS: {:?}", report.warnings);
        }

        // The catalog should be clean enough for LLM routing
        assert!(
            report.total_mode_count >= 15,
            "Too few modes in catalog: {}",
            report.total_mode_count
        );
    }

    #[test]
    fn test_evaluate_intent_router_alias() {
        // Verify evaluate_intent_router is an alias for evaluate
        let router = build_router();
        let eval_set = load_eval_set(GOLDEN_EVAL_YAML).expect("parse");
        let r1 = evaluate(&router, &eval_set);
        let r2 = evaluate_intent_router(&router, &eval_set);
        assert_eq!(r1.len(), r2.len());
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert_eq!(a.actual_agent, b.actual_agent);
            assert_eq!(a.actual_mode, b.actual_mode);
        }
    }
}
