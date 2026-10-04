use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// The lifetime and authority boundary from which a permission rule originates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    User,
    ProjectShared,
    ProjectLocal,
    Session,
    Managed,
}

impl PermissionScope {
    fn priority(self) -> u8 {
        match self {
            Self::User => 0,
            Self::ProjectShared => 1,
            Self::ProjectLocal => 2,
            Self::Session => 3,
            Self::Managed => 4,
        }
    }
}

/// Identifies where a rule came from for diagnostics and auditing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleOrigin {
    pub scope: PermissionScope,
    pub source: String,
}

/// The operation or operation class to which a permission rule applies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PermissionPrincipal {
    Function { extension: String, function: String },
    Extension { extension: String },
    Capability { capability: String },
}

impl PermissionPrincipal {
    fn specificity(&self) -> u8 {
        match self {
            Self::Capability { .. } => 0,
            Self::Extension { .. } => 1,
            Self::Function { .. } => 2,
        }
    }

    fn matches(&self, request: &PermissionRequest) -> bool {
        match self {
            Self::Function {
                extension,
                function,
            } => extension == &request.extension && function == &request.function,
            Self::Extension { extension } => extension == &request.extension,
            Self::Capability { capability } => request.capabilities.contains(capability),
        }
    }

    fn tie_break_key(&self) -> (&str, &str, &str) {
        match self {
            Self::Function {
                extension,
                function,
            } => ("function", extension, function),
            Self::Extension { extension } => ("extension", extension, ""),
            Self::Capability { capability } => ("capability", capability, ""),
        }
    }
}

/// The action goose takes when a permission rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionEffect {
    Allow,
    Ask,
    Deny,
}

impl PermissionEffect {
    fn priority(self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::Ask => 1,
            Self::Deny => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PermissionRule {
    pub origin: RuleOrigin,
    pub principal: PermissionPrincipal,
    pub effect: PermissionEffect,
}

/// A concrete function invocation and its declared capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequest {
    pub extension: String,
    pub function: String,
    pub capabilities: BTreeSet<String>,
}

/// The resolved effect and, when present, the rule that determined it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionResolution {
    pub effect: PermissionEffect,
    pub matched_rule: Option<PermissionRule>,
}

/// Resolves matching rules without consulting storage or mutable process state.
///
/// Effects are compared first (`deny > ask > allow`), then scopes
/// (`managed > session > project-local > project-shared > user`), and finally
/// principal specificity (`function > extension > capability`). An unmatched
/// request fails closed to `ask`.
pub fn resolve_permission(
    rules: &[PermissionRule],
    request: &PermissionRequest,
) -> PermissionResolution {
    let matched_rule = rules
        .iter()
        .filter(|rule| rule.principal.matches(request))
        .max_by(|left, right| compare_rules(left, right))
        .cloned();

    PermissionResolution {
        effect: matched_rule
            .as_ref()
            .map_or(PermissionEffect::Ask, |rule| rule.effect),
        matched_rule,
    }
}

fn compare_rules(left: &PermissionRule, right: &PermissionRule) -> Ordering {
    left.effect
        .priority()
        .cmp(&right.effect.priority())
        .then_with(|| {
            left.origin
                .scope
                .priority()
                .cmp(&right.origin.scope.priority())
        })
        .then_with(|| {
            left.principal
                .specificity()
                .cmp(&right.principal.specificity())
        })
        // Canonical tie-breakers make the selected origin independent of input order.
        .then_with(|| {
            right
                .principal
                .tie_break_key()
                .cmp(&left.principal.tie_break_key())
        })
        .then_with(|| right.origin.source.cmp(&left.origin.source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> PermissionRequest {
        PermissionRequest {
            extension: "developer".into(),
            function: "shell".into(),
            capabilities: BTreeSet::from(["execute".into(), "workspace_write".into()]),
        }
    }

    fn rule(
        scope: PermissionScope,
        principal: PermissionPrincipal,
        effect: PermissionEffect,
    ) -> PermissionRule {
        PermissionRule {
            origin: RuleOrigin {
                scope,
                source: format!("{scope:?}"),
            },
            principal,
            effect,
        }
    }

    fn capability(name: &str) -> PermissionPrincipal {
        PermissionPrincipal::Capability {
            capability: name.into(),
        }
    }

    fn extension(name: &str) -> PermissionPrincipal {
        PermissionPrincipal::Extension {
            extension: name.into(),
        }
    }

    fn function(extension: &str, name: &str) -> PermissionPrincipal {
        PermissionPrincipal::Function {
            extension: extension.into(),
            function: name.into(),
        }
    }

    #[test]
    fn effect_precedence_applies_across_scopes() {
        let effects = [
            PermissionEffect::Allow,
            PermissionEffect::Ask,
            PermissionEffect::Deny,
        ];

        for managed_effect in effects {
            for user_effect in effects {
                let rules = [
                    rule(
                        PermissionScope::Managed,
                        function("developer", "shell"),
                        managed_effect,
                    ),
                    rule(PermissionScope::User, capability("execute"), user_effect),
                ];
                let expected = if managed_effect.priority() >= user_effect.priority() {
                    managed_effect
                } else {
                    user_effect
                };

                assert_eq!(resolve_permission(&rules, &request()).effect, expected);
            }
        }
    }

    #[test]
    fn scope_precedence_applies_for_the_same_effect() {
        let scopes = [
            PermissionScope::User,
            PermissionScope::ProjectShared,
            PermissionScope::ProjectLocal,
            PermissionScope::Session,
            PermissionScope::Managed,
        ];

        for (index, expected_scope) in scopes.iter().copied().enumerate() {
            let rules: Vec<_> = scopes[..=index]
                .iter()
                .copied()
                .map(|scope| rule(scope, capability("execute"), PermissionEffect::Allow))
                .collect();

            assert_eq!(
                resolve_permission(&rules, &request())
                    .matched_rule
                    .unwrap()
                    .origin
                    .scope,
                expected_scope
            );
        }
    }

    #[test]
    fn principal_specificity_applies_for_the_same_effect_and_scope() {
        let cases = [
            (vec![capability("execute")], capability("execute")),
            (
                vec![capability("execute"), extension("developer")],
                extension("developer"),
            ),
            (
                vec![
                    capability("execute"),
                    extension("developer"),
                    function("developer", "shell"),
                ],
                function("developer", "shell"),
            ),
        ];

        for (principals, expected) in cases {
            let rules: Vec<_> = principals
                .into_iter()
                .map(|principal| {
                    rule(
                        PermissionScope::ProjectLocal,
                        principal,
                        PermissionEffect::Ask,
                    )
                })
                .collect();

            assert_eq!(
                resolve_permission(&rules, &request())
                    .matched_rule
                    .unwrap()
                    .principal,
                expected
            );
        }
    }

    #[test]
    fn only_matching_rules_participate() {
        let non_matches = [
            function("developer", "text_editor"),
            function("other", "shell"),
            extension("other"),
            capability("network"),
        ];

        for principal in non_matches {
            let rules = [
                rule(PermissionScope::Managed, principal, PermissionEffect::Deny),
                rule(
                    PermissionScope::User,
                    capability("execute"),
                    PermissionEffect::Allow,
                ),
            ];
            assert_eq!(
                resolve_permission(&rules, &request()).effect,
                PermissionEffect::Allow
            );
        }
    }

    #[test]
    fn unmatched_requests_fail_closed_to_ask() {
        assert_eq!(
            resolve_permission(&[], &request()),
            PermissionResolution {
                effect: PermissionEffect::Ask,
                matched_rule: None,
            }
        );
    }

    #[test]
    fn exact_ties_are_independent_of_input_order() {
        let mut first = rule(
            PermissionScope::User,
            capability("execute"),
            PermissionEffect::Allow,
        );
        first.origin.source = "a".into();
        let mut second = first.clone();
        second.origin.source = "b".into();

        let forward = resolve_permission(&[first.clone(), second.clone()], &request());
        let reverse = resolve_permission(&[second, first], &request());

        assert_eq!(forward, reverse);
        assert_eq!(forward.matched_rule.unwrap().origin.source, "a");
    }
}
