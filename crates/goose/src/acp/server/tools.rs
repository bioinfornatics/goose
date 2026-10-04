use super::*;
use crate::agents::extension_manager::{get_parameter_names, is_tool_owned_by_extension};
use crate::agents::reply_parts::is_tool_visible_to_app;
use crate::config::permission::PermissionLevel;
use crate::config::scoped_permissions::{
    PermissionEffect, PermissionPrincipal, PermissionRequest, PermissionRule, PermissionScope,
};
use goose_sdk_types::custom_requests::{
    ToolListItem, ToolMetadataHints, ToolPermissionLevel, ToolPermissionPrincipal,
    ToolPermissionRule, ToolPermissionScope, ToolPermissionSource,
};
use rmcp::model::CallToolRequestParams;
use std::collections::BTreeSet;

fn permission_effect_to_sdk(effect: PermissionEffect) -> ToolPermissionLevel {
    match effect {
        PermissionEffect::Allow => ToolPermissionLevel::AlwaysAllow,
        PermissionEffect::Ask => ToolPermissionLevel::AskBefore,
        PermissionEffect::Deny => ToolPermissionLevel::NeverAllow,
    }
}
fn permission_scope_to_sdk(scope: PermissionScope) -> ToolPermissionScope {
    match scope {
        PermissionScope::User => ToolPermissionScope::User,
        PermissionScope::ProjectShared => ToolPermissionScope::ProjectShared,
        PermissionScope::ProjectLocal => ToolPermissionScope::ProjectLocal,
        PermissionScope::Session => ToolPermissionScope::Session,
        PermissionScope::Managed => ToolPermissionScope::Managed,
    }
}
fn permission_scope_from_sdk(scope: ToolPermissionScope) -> PermissionScope {
    match scope {
        ToolPermissionScope::User => PermissionScope::User,
        ToolPermissionScope::ProjectShared => PermissionScope::ProjectShared,
        ToolPermissionScope::ProjectLocal => PermissionScope::ProjectLocal,
        ToolPermissionScope::Session => PermissionScope::Session,
        ToolPermissionScope::Managed => PermissionScope::Managed,
    }
}
fn principal_to_sdk(principal: &PermissionPrincipal) -> ToolPermissionPrincipal {
    match principal {
        PermissionPrincipal::Function {
            extension,
            function,
        } => ToolPermissionPrincipal::Function {
            extension: extension.clone(),
            function: function.clone(),
        },
        PermissionPrincipal::Extension { extension } => ToolPermissionPrincipal::Extension {
            extension: extension.clone(),
        },
        PermissionPrincipal::Capability { capability } => ToolPermissionPrincipal::Capability {
            capability: capability.clone(),
        },
    }
}
fn rule_to_sdk(rule: &PermissionRule) -> ToolPermissionRule {
    ToolPermissionRule {
        scope: permission_scope_to_sdk(rule.origin.scope),
        effect: permission_effect_to_sdk(rule.effect),
        principal: principal_to_sdk(&rule.principal),
        origin: rule.origin.source.clone(),
    }
}
fn principal_applies(principal: &PermissionPrincipal, extension: &str, function: &str) -> bool {
    match principal {
        PermissionPrincipal::Function {
            extension: e,
            function: f,
        } => e == extension && f == function,
        PermissionPrincipal::Extension { extension: e } => e == extension,
        PermissionPrincipal::Capability { .. } => false,
    }
}

fn humanize_tool_name(name: &str) -> String {
    name.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl GooseAcpAgent {
    pub(super) async fn on_get_tools(
        &self,
        req: GetToolsRequest,
    ) -> Result<GetToolsResponse, agent_client_protocol::Error> {
        let session_id = &req.session_id;
        let agent = self.get_session_agent(&req.session_id).await?;
        let goose_mode = agent.goose_mode().await;
        let permission_manager = self.permission_manager();
        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.clone()))
            })?;
        let scoped_rules = permission_manager
            .scoped_rules(Some(&session.working_dir), Some(session_id))
            .map_err(|error| {
                agent_client_protocol::Error::internal_error().data(error.to_string())
            })?;

        let mut tools: Vec<ToolListItem> = agent
            .list_tools(session_id, req.extension_name)
            .await
            .into_iter()
            .map(|tool| {
                let (extension_name, function_name) = tool
                    .name
                    .split_once("__")
                    .unwrap_or(("unknown", tool.name.as_ref()));
                let request = PermissionRequest {
                    extension: extension_name.to_string(),
                    function: function_name.to_string(),
                    capabilities: BTreeSet::new(),
                };
                let resolution =
                    crate::config::scoped_permissions::resolve_permission(&scoped_rules, &request);
                let applicable_rules = scoped_rules
                    .iter()
                    .filter(|rule| {
                        principal_applies(&rule.principal, extension_name, function_name)
                    })
                    .map(rule_to_sdk)
                    .collect();
                let explicit_permission = resolution
                    .matched_rule
                    .as_ref()
                    .map(|rule| permission_effect_to_sdk(rule.effect));
                let effective_permission_scope = resolution
                    .matched_rule
                    .as_ref()
                    .map(|rule| permission_scope_to_sdk(rule.origin.scope));
                let effective_permission_origin = resolution
                    .matched_rule
                    .as_ref()
                    .map(|rule| rule.origin.source.clone());
                let (effective_permission, permission_source, permission_reason) =
                    if let Some(permission) = explicit_permission {
                        (
                            Some(permission),
                            ToolPermissionSource::ScopedRule,
                            "Resolved from an applicable scoped rule".to_string(),
                        )
                    } else {
                        match goose_mode {
                        GooseMode::Auto => (
                            Some(ToolPermissionLevel::AlwaysAllow),
                            ToolPermissionSource::ModeDefault,
                            "Auto mode allows tool calls".to_string(),
                        ),
                        GooseMode::Approve => (
                            Some(ToolPermissionLevel::AskBefore),
                            ToolPermissionSource::ModeDefault,
                            "Approve mode asks before tool calls".to_string(),
                        ),
                        GooseMode::Chat => (
                            Some(ToolPermissionLevel::NeverAllow),
                            ToolPermissionSource::ModeDefault,
                            "Chat mode does not run tools".to_string(),
                        ),
                        GooseMode::SmartApprove
                            if tool
                                .annotations
                                .as_ref()
                                .and_then(|annotations| annotations.read_only_hint)
                                == Some(true) =>
                        {
                            (
                                Some(ToolPermissionLevel::AlwaysAllow),
                                ToolPermissionSource::ToolAnnotation,
                                "Tool declares a read-only operation".to_string(),
                            )
                        }
                        GooseMode::SmartApprove => match permission_manager
                            .get_smart_approve_permission(&tool.name)
                        {
                            Some(PermissionLevel::AskBefore) => (
                                Some(ToolPermissionLevel::AskBefore),
                                ToolPermissionSource::SmartApproveCache,
                                "Smart Approve previously classified this tool as state-changing"
                                    .to_string(),
                            ),
                            _ => (
                                None,
                                ToolPermissionSource::SmartApproveRuntime,
                                "Smart Approve evaluates each concrete call at runtime".to_string(),
                            ),
                        },
                    }
                    };
                let permission = explicit_permission.or(effective_permission);
                let fallback_name = function_name;
                let annotations = tool.annotations.as_ref();
                let display_name = annotations
                    .and_then(|value| value.title.clone())
                    .unwrap_or_else(|| humanize_tool_name(fallback_name));
                let metadata_source =
                    if extension_name == "developer" || extension_name == "platform" {
                        "goose_builtin"
                    } else {
                        "mcp_declared"
                    };
                ToolListItem {
                    name: tool.name.to_string(),
                    display_name,
                    description: tool
                        .description
                        .as_ref()
                        .map(|d| d.as_ref().to_string())
                        .unwrap_or_default(),
                    extension_name: extension_name.to_string(),
                    metadata_source: metadata_source.to_string(),
                    metadata_hints: ToolMetadataHints {
                        read_only: annotations.and_then(|value| value.read_only_hint),
                        destructive: annotations.and_then(|value| value.destructive_hint),
                        idempotent: annotations.and_then(|value| value.idempotent_hint),
                        open_world: annotations.and_then(|value| value.open_world_hint),
                    },
                    parameters: get_parameter_names(&tool),
                    permission,
                    explicit_permission,
                    effective_permission,
                    permission_source,
                    permission_reason,
                    applicable_permission_rules: applicable_rules,
                    effective_permission_scope,
                    effective_permission_origin,
                    input_schema: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
                    output_schema: tool
                        .output_schema
                        .as_ref()
                        .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null)),
                }
            })
            .collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(GetToolsResponse {
            tools,
            writable_permission_scopes: vec![
                ToolPermissionScope::User,
                ToolPermissionScope::ProjectShared,
                ToolPermissionScope::ProjectLocal,
                ToolPermissionScope::Session,
            ],
        })
    }

    pub(super) async fn on_call_tool(
        &self,
        req: GooseToolCallRequest,
    ) -> Result<GooseToolCallResponse, agent_client_protocol::Error> {
        let session_id = &req.session_id;
        let agent = self.get_session_agent(&req.session_id).await?;
        let tools = agent
            .list_tools(session_id, Some(req.extension_name.clone()))
            .await;

        let Some(tool) = tools.iter().find(|tool| {
            *tool.name == req.name && is_tool_owned_by_extension(tool, &req.extension_name)
        }) else {
            return Err(agent_client_protocol::Error::invalid_params().data("tool not found"));
        };

        if !is_tool_visible_to_app(tool) {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("tool is not visible to app clients"));
        }

        let arguments = match req.arguments {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            _ => {
                return Err(agent_client_protocol::Error::invalid_params()
                    .data("tool arguments must be an object"));
            }
        };

        let tool_call = {
            let mut params = CallToolRequestParams::new(req.name);
            if let Some(args) = arguments {
                params = params.with_arguments(args);
            }
            params
        };

        if agent.goose_mode().await != GooseMode::Auto {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("app tool calls require auto mode"));
        }

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        let ctx = crate::agents::ToolCallContext::new(
            session_id.clone(),
            Some(session.working_dir),
            None,
        );
        let tool_result = agent
            .extension_manager
            .dispatch_app_tool_call(
                &ctx,
                tool_call,
                &req.extension_name,
                CancellationToken::new(),
            )
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;

        let result = tool_result
            .result
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;

        let content = result
            .content
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;

        Ok(GooseToolCallResponse {
            content,
            structured_content: result.structured_content,
            is_error: result.is_error.unwrap_or(false),
            meta: result.meta.and_then(|m| serde_json::to_value(m).ok()),
        })
    }

    pub(super) async fn on_set_tool_permissions(
        &self,
        req: SetToolPermissionsRequest,
    ) -> Result<SetToolPermissionsResponse, agent_client_protocol::Error> {
        let permission_manager = self.permission_manager();
        let session_id = req.session_id.as_deref();
        let needs_project = req.tool_permissions.iter().any(|entry| {
            matches!(
                entry.scope,
                ToolPermissionScope::ProjectShared | ToolPermissionScope::ProjectLocal
            )
        });
        let project_root = if needs_project {
            let id = session_id.ok_or_else(|| {
                agent_client_protocol::Error::invalid_params()
                    .data("project-scoped permissions require sessionId")
            })?;
            Some(
                self.session_manager
                    .get_session(id, false)
                    .await
                    .map_err(|_| {
                        agent_client_protocol::Error::resource_not_found(Some(id.to_string()))
                    })?
                    .working_dir,
            )
        } else {
            None
        };

        for entry in &req.tool_permissions {
            let scope = permission_scope_from_sdk(entry.scope);
            if scope == PermissionScope::Managed {
                return Err(agent_client_protocol::Error::invalid_params()
                    .data("managed permission scope is read-only"));
            }
            if scope == PermissionScope::Session && session_id.is_none() {
                return Err(agent_client_protocol::Error::invalid_params()
                    .data("session-scoped permissions require sessionId"));
            }
            let (extension, function) = entry.tool_name.split_once("__").ok_or_else(|| {
                agent_client_protocol::Error::invalid_params()
                    .data("toolName must be a canonical extension__function name")
            })?;
            let principal = PermissionPrincipal::Function {
                extension: extension.to_string(),
                function: function.to_string(),
            };
            if let Some(permission) = entry.permission {
                let effect = match permission {
                    ToolPermissionLevel::AlwaysAllow => PermissionEffect::Allow,
                    ToolPermissionLevel::AskBefore => PermissionEffect::Ask,
                    ToolPermissionLevel::NeverAllow => PermissionEffect::Deny,
                };
                permission_manager
                    .update_scoped_permission(
                        scope,
                        project_root.as_deref(),
                        session_id,
                        principal,
                        effect,
                    )
                    .map_err(|error| {
                        agent_client_protocol::Error::invalid_params().data(error.to_string())
                    })?;
            } else {
                let retained: Vec<_> = permission_manager
                    .scoped_rules(project_root.as_deref(), session_id)
                    .map_err(|error| {
                        agent_client_protocol::Error::internal_error().data(error.to_string())
                    })?
                    .into_iter()
                    .filter(|rule| rule.origin.scope == scope && rule.principal != principal)
                    .collect();
                permission_manager
                    .reset_scoped_permissions(scope, project_root.as_deref(), session_id)
                    .map_err(|error| {
                        agent_client_protocol::Error::invalid_params().data(error.to_string())
                    })?;
                for rule in retained {
                    permission_manager
                        .update_scoped_permission(
                            scope,
                            project_root.as_deref(),
                            session_id,
                            rule.principal,
                            rule.effect,
                        )
                        .map_err(|error| {
                            agent_client_protocol::Error::invalid_params().data(error.to_string())
                        })?;
                }
            }
        }
        Ok(SetToolPermissionsResponse {})
    }
}
