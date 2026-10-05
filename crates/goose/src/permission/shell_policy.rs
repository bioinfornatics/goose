use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::{GooseMode, PermissionManager};
use crate::conversation::message::{Message, ToolRequest};
use crate::session::SessionManager;
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

const MAX: usize = 32768;
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Evaluation {
    Decision(Decision),
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternValidationError {
    NoAlternatives,
    EmptyAlternative,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentPattern {
    Match(Vec<String>),
    NotMatch(Vec<String>),
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellPolicyConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<ShellRuleConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellRuleConfig {
    pub id: String,
    pub decision: Decision,
    pub pattern: Vec<ShellArgumentPattern>,
    pub reason: String,
    #[serde(default, rename = "match", skip_serializing_if = "Vec::is_empty")]
    pub match_examples: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_match: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ShellArgumentPattern {
    Literal(String),
    AnyOf { any_of: Vec<String> },
}

impl ShellPolicyConfig {
    pub fn compile(&self) -> Result<Vec<TokenPrefixRule>> {
        let mut ids = std::collections::HashSet::new();
        self.rules
            .iter()
            .map(|rule| {
                if rule.id.trim().is_empty() || !ids.insert(rule.id.as_str()) {
                    bail!("shell rule IDs must be non-empty and unique");
                }
                if rule.reason.trim().is_empty() {
                    bail!("shell rule '{}' requires a reason", rule.id);
                }
                let prefix = rule
                    .pattern
                    .iter()
                    .map(|pattern| match pattern {
                        ShellArgumentPattern::Literal(value) => {
                            ArgumentPattern::matches([value.clone()])
                        }
                        ShellArgumentPattern::AnyOf { any_of } => {
                            ArgumentPattern::matches(any_of.clone())
                        }
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|error| {
                        anyhow::anyhow!("invalid shell rule '{}': {error:?}", rule.id)
                    })?;
                let compiled = TokenPrefixRule::new(rule.decision, prefix).map_err(|error| {
                    anyhow::anyhow!("invalid shell rule '{}': {error:?}", rule.id)
                })?;
                for example in &rule.match_examples {
                    if !compiled.matches_command(example)? {
                        bail!(
                            "shell rule '{}' match example does not match: {example}",
                            rule.id
                        );
                    }
                }
                for example in &rule.not_match {
                    if compiled.matches_command(example)? {
                        bail!(
                            "shell rule '{}' not_match example matches: {example}",
                            rule.id
                        );
                    }
                }
                Ok(compiled)
            })
            .collect()
    }
}
impl ArgumentPattern {
    pub fn matches<I, S>(x: I) -> Result<Self, PatternValidationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::make(x, true)
    }
    pub fn not_matches<I, S>(x: I) -> Result<Self, PatternValidationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::make(x, false)
    }
    fn make<I, S>(x: I, p: bool) -> Result<Self, PatternValidationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let x: Vec<_> = x.into_iter().map(Into::into).collect();
        if x.is_empty() {
            return Err(PatternValidationError::NoAlternatives);
        }
        if x.iter().any(String::is_empty) {
            return Err(PatternValidationError::EmptyAlternative);
        }
        Ok(if p { Self::Match(x) } else { Self::NotMatch(x) })
    }
    fn test(&self, x: &str) -> bool {
        match self {
            Self::Match(v) => v.iter().any(|s| s == x),
            Self::NotMatch(v) => v.iter().all(|s| s != x),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenPrefixRule {
    pub decision: Decision,
    prefix: Vec<ArgumentPattern>,
}
impl TokenPrefixRule {
    pub fn new(
        decision: Decision,
        prefix: Vec<ArgumentPattern>,
    ) -> Result<Self, PatternValidationError> {
        if prefix.is_empty() {
            Err(PatternValidationError::NoAlternatives)
        } else {
            Ok(Self { decision, prefix })
        }
    }
    pub fn prefix(&self) -> &[ArgumentPattern] {
        &self.prefix
    }
    fn eval(&self, t: &[String]) -> Option<Decision> {
        (t.len() >= self.prefix.len() && self.prefix.iter().zip(t).all(|(p, x)| p.test(x)))
            .then_some(self.decision)
    }

    fn matches_command(&self, command: &str) -> Result<bool> {
        let commands =
            parse(command).map_err(|_| anyhow::anyhow!("example uses unsupported shell syntax"))?;
        Ok(commands.len() == 1
            && unwrap(&commands[0]).is_some_and(|tokens| self.eval(tokens).is_some()))
    }
}
#[derive(Clone, Debug, Default)]
pub struct ShellPolicy {
    rules: Vec<TokenPrefixRule>,
}
impl ShellPolicy {
    pub fn new(rules: Vec<TokenPrefixRule>) -> Self {
        Self { rules }
    }
    pub fn with_builtin_rules() -> Self {
        let p = |x: &[&str]| ArgumentPattern::matches(x.iter().copied()).unwrap();
        Self::new(vec![
            TokenPrefixRule::new(Decision::Deny, vec![p(&["sudo", "doas", "su"])]).unwrap(),
            TokenPrefixRule::new(Decision::Ask, vec![p(&["rm", "/bin/rm"])]).unwrap(),
        ])
    }
    pub fn rules(&self) -> &[TokenPrefixRule] {
        &self.rules
    }
    pub fn evaluate(&self, s: &str) -> Evaluation {
        let Ok(cs) = parse(s) else {
            return Evaluation::Unknown;
        };
        let mut d = Decision::Allow;
        for c in cs {
            let Some(c) = unwrap(&c) else {
                return Evaluation::Unknown;
            };
            for r in &self.rules {
                if let Some(x) = r.eval(c) {
                    d = d.max(x)
                }
            }
            if root_rm(c) {
                d = Decision::Deny
            }
        }
        Evaluation::Decision(d)
    }
}
pub fn evaluate_shell_command(s: &str) -> Evaluation {
    ShellPolicy::with_builtin_rules().evaluate(s)
}

pub struct ShellPolicyInspector {
    permission_manager: Arc<PermissionManager>,
    session_manager: Arc<SessionManager>,
}

impl ShellPolicyInspector {
    pub fn new(
        permission_manager: Arc<PermissionManager>,
        session_manager: Arc<SessionManager>,
    ) -> Self {
        Self {
            permission_manager,
            session_manager,
        }
    }
}

#[async_trait]
impl ToolInspector for ShellPolicyInspector {
    fn name(&self) -> &'static str {
        "shell_policy"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        let policy = match self.session_manager.get_session(session_id, false).await {
            Ok(session) => self
                .permission_manager
                .compile_shell_policy(Some(&session.working_dir)),
            Err(error) => Err(error),
        };
        let policy = match policy {
            Ok(policy) => policy,
            Err(error) => {
                tracing::error!(%error, "Failed to load shell policy; asking instead");
                return Ok(tool_requests
                    .iter()
                    .filter(|request| {
                        request.tool_call.as_ref().is_ok_and(|tool_call| {
                            matches!(tool_call.name.as_ref(), "shell" | "developer__shell")
                        })
                    })
                    .map(|request| InspectionResult {
                        tool_request_id: request.id.clone(),
                        action: InspectionAction::RequireApproval(Some(
                            "Shell policy could not be loaded".to_string(),
                        )),
                        reason: "Shell policy could not be loaded".to_string(),
                        confidence: 1.0,
                        inspector_name: self.name().to_string(),
                        finding_id: None,
                    })
                    .collect());
            }
        };
        Ok(tool_requests
            .iter()
            .filter_map(|request| {
                let tool_call = request.tool_call.as_ref().ok()?;
                let tool_name = tool_call.name.as_ref();
                if !matches!(tool_name, "shell" | "developer__shell") {
                    return None;
                }
                let command = tool_call
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("command"))
                    .and_then(serde_json::Value::as_str);
                let (action, reason) = match command.map(|command| policy.evaluate(command)) {
                    Some(Evaluation::Decision(Decision::Deny)) => (
                        InspectionAction::Deny,
                        "Shell command is forbidden by deterministic policy",
                    ),
                    Some(Evaluation::Decision(Decision::Ask)) => (
                        InspectionAction::RequireApproval(Some(
                            "Shell command requires approval by deterministic policy".to_string(),
                        )),
                        "Shell command requires approval by deterministic policy",
                    ),
                    Some(Evaluation::Decision(Decision::Allow)) => return None,
                    Some(Evaluation::Unknown) | None => (
                        InspectionAction::RequireApproval(Some(
                            "Shell syntax could not be safely classified".to_string(),
                        )),
                        "Shell syntax could not be safely classified",
                    ),
                };
                Some(InspectionResult {
                    tool_request_id: request.id.clone(),
                    action,
                    reason: reason.to_string(),
                    confidence: 1.0,
                    inspector_name: self.name().to_string(),
                    finding_id: None,
                })
            })
            .collect())
    }
}
fn root_rm(t: &[String]) -> bool {
    if !matches!(t.first().map(String::as_str), Some("rm" | "/bin/rm")) {
        return false;
    }
    let (mut r, mut f, mut root, mut opts) = (false, false, false, true);
    for x in &t[1..] {
        if opts && x == "--" {
            opts = false;
            continue;
        }
        if opts && x.starts_with("--") {
            r |= x == "--recursive";
            f |= x == "--force";
            continue;
        }
        if opts && x.starts_with('-') && x != "-" {
            for c in x[1..].chars() {
                r |= c == 'r' || c == 'R';
                f |= c == 'f'
            }
            continue;
        }
        root |= !x.is_empty() && x.bytes().all(|b| b == b'/')
    }
    r && f && root
}
fn unwrap(mut t: &[String]) -> Option<&[String]> {
    for _ in 0..16 {
        let n = match t.first()?.as_str() {
            "env" | "/usr/bin/env" => env(t)?,
            "command" | "nohup" => simple(t)?,
            "nice" => nice(t)?,
            "timeout" => timeout(t)?,
            _ => return Some(t),
        };
        t = t.get(n..)?
    }
    None
}
fn simple(t: &[String]) -> Option<usize> {
    let mut i = 1;
    while let Some(x) = t.get(i) {
        if x == "--" {
            return (i + 1 < t.len()).then_some(i + 1);
        }
        if x.starts_with('-') && x != "-" {
            i += 1
        } else {
            return Some(i);
        }
    }
    None
}
fn assign(x: &str) -> bool {
    let Some((n, _)) = x.split_once('=') else {
        return false;
    };
    let mut c = n.chars();
    matches!(c.next(),Some(x)if x=='_'||x.is_ascii_alphabetic())
        && c.all(|x| x == '_' || x.is_ascii_alphanumeric())
}
fn env(t: &[String]) -> Option<usize> {
    let mut i = 1;
    while let Some(x) = t.get(i) {
        if x == "--" {
            return (i + 1 < t.len()).then_some(i + 1);
        }
        if matches!(x.as_str(), "-u" | "--unset" | "-C" | "--chdir") {
            i += 2
        } else if x.starts_with('-') && x != "-" || assign(x) {
            i += 1
        } else {
            return Some(i);
        }
    }
    None
}
fn nice(t: &[String]) -> Option<usize> {
    let mut i = 1;
    while let Some(x) = t.get(i) {
        if x == "--" {
            return (i + 1 < t.len()).then_some(i + 1);
        }
        if matches!(x.as_str(), "-n" | "--adjustment") {
            i += 2
        } else if x.starts_with("--adjustment=")
            || x.strip_prefix('-')
                .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
        {
            i += 1
        } else {
            return Some(i);
        }
    }
    None
}
fn timeout(t: &[String]) -> Option<usize> {
    let mut i = 1;
    while let Some(x) = t.get(i) {
        if x == "--" {
            i += 1;
            break;
        }
        if matches!(x.as_str(), "-k" | "--kill-after" | "-s" | "--signal") {
            i += 2
        } else if x.starts_with("--kill-after=")
            || x.starts_with("--signal=")
            || x.starts_with('-') && x != "-"
        {
            i += 1
        } else {
            break;
        }
    }
    (i + 1 < t.len()).then_some(i + 1)
}
fn parse(s: &str) -> Result<Vec<Vec<String>>, ()> {
    if s.is_empty() || s.len() > MAX || s.contains('\0') {
        return Err(());
    }
    let mut out = vec![vec![]];
    let (mut w, mut started, mut q, mut esc) = (String::new(), false, None, false);
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < c.len() {
        let x = c[i];
        if esc {
            w.push(x);
            started = true;
            esc = false;
            i += 1;
            continue;
        }
        if q == Some('\'') {
            if x == '\'' {
                q = None
            } else {
                w.push(x)
            }
            started = true;
            i += 1;
            continue;
        }
        if q == Some('"') {
            match x {
                '"' => q = None,
                '\\' => esc = true,
                '$' | '`' => return Err(()),
                _ => w.push(x),
            }
            started = true;
            i += 1;
            continue;
        }
        match x {
            '\\' => {
                esc = true;
                started = true
            }
            '\'' | '"' => {
                q = Some(x);
                started = true
            }
            '$' | '`' | '<' | '>' | '(' | ')' | '{' | '}' | '*' | '?' | '[' | ']' | '#' => {
                return Err(());
            }
            ' ' | '\t' | '\r' => push(&mut out, &mut w, &mut started)?,
            ';' | '\n' | '|' | '&' => {
                push(&mut out, &mut w, &mut started)?;
                let n = match (x, c.get(i + 1).copied()) {
                    ('&', Some('&')) | ('|', Some('|')) | ('|', Some('&')) => 2,
                    (';', _) | ('\n', _) | ('|', _) => 1,
                    _ => return Err(()),
                };
                if out.last().is_none_or(Vec::is_empty) || out.len() >= 128 {
                    return Err(());
                }
                out.push(vec![]);
                i += n;
                continue;
            }
            _ => {
                w.push(x);
                started = true
            }
        }
        i += 1
    }
    if esc || q.is_some() {
        return Err(());
    }
    push(&mut out, &mut w, &mut started)?;
    if out.last().is_none_or(Vec::is_empty) {
        Err(())
    } else {
        Ok(out)
    }
}
fn push(o: &mut [Vec<String>], w: &mut String, s: &mut bool) -> Result<(), ()> {
    if *s {
        let c = o.last_mut().ok_or(())?;
        if c.len() >= 512 {
            return Err(());
        }
        c.push(std::mem::take(w));
        *s = false
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtins() {
        for s in [
            "sudo id",
            "env A=1 command -- sudo id",
            "echo x&&nice -n 2 doas id",
            "nohup su u",
        ] {
            assert_eq!(
                evaluate_shell_command(s),
                Evaluation::Decision(Decision::Deny)
            )
        }
        for s in ["rm x", "/bin/rm -r x"] {
            assert_eq!(
                evaluate_shell_command(s),
                Evaluation::Decision(Decision::Ask)
            )
        }
        for s in [
            "rm -rf /",
            "rm -fr /",
            "rm -r -f /",
            "rm --recursive --force /",
            "/bin/rm --force -R //",
            "timeout 2 rm -rf -- /",
        ] {
            assert_eq!(
                evaluate_shell_command(s),
                Evaluation::Decision(Decision::Deny)
            )
        }
    }
    #[test]
    fn unknown() {
        for s in [
            "rm $X",
            "rm `pwd`",
            "echo $(id)",
            "x > y",
            "echo *",
            "echo & id",
            "echo ||",
        ] {
            assert_eq!(evaluate_shell_command(s), Evaluation::Unknown)
        }
    }
    #[test]
    fn custom() {
        let p = ShellPolicy::new(vec![TokenPrefixRule::new(
            Decision::Deny,
            vec![
                ArgumentPattern::matches(["git", "hg"]).unwrap(),
                ArgumentPattern::not_matches(["status", "diff"]).unwrap(),
            ],
        )
        .unwrap()]);
        assert_eq!(p.evaluate("git push"), Evaluation::Decision(Decision::Deny));
        assert_eq!(
            p.evaluate("git status"),
            Evaluation::Decision(Decision::Allow)
        )
    }

    #[tokio::test]
    async fn inspector_uses_project_policy_and_fails_closed() {
        use crate::session::SessionType;
        use rmcp::model::CallToolRequestParams;
        use rmcp::object;

        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let project_config = project.join(".config/goose");
        std::fs::create_dir_all(&project_config).unwrap();
        std::fs::write(
            project_config.join("permission.local.yaml"),
            r#"version: 2
permissions: {}
shell:
  rules:
    - id: deny-git-push
      decision: deny
      pattern: [git, push]
      reason: Git push is forbidden
      match: ["git push"]
      not_match: ["git status"]
"#,
        )
        .unwrap();
        let sessions = Arc::new(SessionManager::new(root.path().join("sessions")));
        let session = sessions
            .create_session(project, "test".into(), SessionType::User, GooseMode::Auto)
            .await
            .unwrap();
        let permissions = Arc::new(PermissionManager::new(root.path().join("permissions")));
        let inspector = ShellPolicyInspector::new(permissions, sessions);
        let request = ToolRequest {
            id: "deny".into(),
            tool_call: Ok(CallToolRequestParams::new("shell").with_arguments(object!({
                "command": "git push"
            }))),
            metadata: None,
            tool_meta: None,
        };
        let results = inspector
            .inspect(
                &session.id,
                std::slice::from_ref(&request),
                &[],
                GooseMode::Auto,
            )
            .await
            .unwrap();
        assert_eq!(results[0].action, InspectionAction::Deny);

        let results = inspector
            .inspect("missing-session", &[request], &[], GooseMode::Auto)
            .await
            .unwrap();
        assert!(matches!(
            results[0].action,
            InspectionAction::RequireApproval(_)
        ));
    }

    #[test]
    fn validation() {
        assert_eq!(
            ArgumentPattern::matches(Vec::<String>::new()),
            Err(PatternValidationError::NoAlternatives)
        );
        assert_eq!(
            ArgumentPattern::not_matches([""]),
            Err(PatternValidationError::EmptyAlternative)
        )
    }
}
