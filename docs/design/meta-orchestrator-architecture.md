# Meta-Orchestrator Architecture: Multi-Agent Goose

## Status: RFC / Design Document
**Author:** goose session  
**Date:** 2026-02-13  
**Branch:** feature/agent_registry

---

## 1. Executive Summary

Transform Goose from a **single-agent-with-extensions** architecture into a **meta-orchestrator** that:

1. **Intercepts** every user message at a routing layer
2. **Understands intent** — classifies what the user wants
3. **Splits compound requests** into sub-tasks when needed
4. **Routes** each sub-task to the optimal agent/mode combination
5. **Aggregates** results back into a coherent response

Extensions become **bound to agents** (0 or more per agent), not globally pooled.

---

## 2. Current Architecture (What Exists)

```
User Message
     │
     ▼
┌─────────────┐
│   Agent     │  ← Single instance, one system prompt
│  (agent.rs) │
│             │
│ ┌─────────┐ │
│ │Extension│ │  ← ALL extensions pooled into one flat tool list
│ │ Manager │ │
│ └─────────┘ │
│      │      │
│      ▼      │
│ ┌─────────┐ │
│ │  LLM    │ │  ← One model decides everything
│ │Provider │ │
│ └─────────┘ │
└─────────────┘
```

### Key components already built:

| Component | File | What it does |
|-----------|------|-------------|
| `Agent` | `agent.rs` | Single agent loop: LLM → tool calls → repeat |
| `ExtensionManager` | `extension_manager.rs` | Manages MCP tool extensions (add/remove/dispatch) |
| `GooseAgent` | `goose_agent.rs` | 8 builtin modes: assistant, specialist, judge, planner, compactor, app_creator, app_iterator, rename |
| `CodingAgent` | `coding_agent.rs` | 8 SDLC modes: pm, architect, backend, frontend, qa, security, sre, devsecops |
| `AgentClientManager` | `agent_manager/client.rs` | Connect/prompt external ACP agents via stdio |
| `AgentSpawner` | `agent_manager/spawner.rs` | Spawn agents: binary, npx, uvx, cargo, docker |
| `TaskManager` | `agent_manager/task.rs` | Track task lifecycle: submitted→working→completed/failed |
| `AgentHealth` | `agent_manager/health.rs` | Health monitoring: healthy→degraded→dead |
| `RegistryEntry` | `registry/manifest.rs` | Agent/Tool/Skill/Recipe definitions with modes, skills, distribution |
| `RegistryManager` | `registry/mod.rs` | Multi-source registry (local, HTTP, GitHub, A2A) |
| Server routes | `agent_management.rs` | REST API: connect/disconnect/prompt/set_mode for external agents |
| UI | `AgentsView.tsx` | Lists builtin + external agents |

**The infrastructure for multi-agent is 80% built.** What's missing is the **routing/orchestration layer** that sits between the user and the agents.

---

## 3. Target Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Interfaces                                   │
│  Desktop (Agents tab) ←→ CLI (goose agents run) ←→ Web             │
└──────────────────────┬──────────────────────────────────────────────┘
                       │ REST API (/agents/*)
                       ▼
┌─────────────────────────────────────────────────────────────────────┐
│              Goose Server (Meta-Orchestrator)                        │
│  ┌─────────────┐ ┌──────────────┐ ┌─────────────┐ ┌─────────────┐ │
│  │ Registry    │ │ IntentRouter │ │ AgentSpawner│ │ Health      │ │
│  │ (local/     │ │ (classify +  │ │ (bin/npx/   │ │ Monitor     │ │
│  │  HTTP/A2A)  │ │  split +     │ │  uvx/docker)│ │ (heartbeat) │ │
│  │             │ │  route)      │ │             │ │             │ │
│  ├─────────────┤ ├──────────────┤ ├─────────────┤ ├─────────────┤ │
│  │ AgentClient │ │ Delegation   │ │ TaskManager │ │ AgentCard   │ │
│  │ Manager     │ │ Tool         │ │ (A2A tasks) │ │ Endpoint    │ │
│  └─────────────┘ └──────────────┘ └─────────────┘ └─────────────┘ │
└──────────┬─────────────────┬─────────────────┬──────────────────────┘
           │ builtin          │ ACP/stdio       │ A2A/HTTP
           ▼                 ▼                 ▼
     ┌──────────┐     ┌──────────┐      ┌──────────┐
     │ Agent A  │     │ Agent B  │      │ Agent C  │
     │ (builtin │     │ (local   │      │ (remote  │
     │  +exts)  │     │  ACP)    │      │  A2A)    │
     └──────────┘     └──────────┘      └──────────┘
```

### Key Difference: Extensions Bound to Agents

**Before:** All extensions → one flat pool → one Agent  
**After:** Each agent has its own extension set (0 or more)

```rust
// BEFORE: Agent has one global ExtensionManager
pub struct Agent {
    pub extension_manager: Arc<ExtensionManager>,  // ALL extensions
    // ...
}

// AFTER: Each AgentSlot has its own extension binding
pub struct AgentSlot {
    pub agent: Arc<Agent>,
    pub bound_extensions: Vec<String>,  // subset of available extensions
    pub modes: Vec<AgentMode>,
    pub skills: Vec<AgentSkill>,
    pub health: AgentHealth,
}
```

---

## 4. The IntentRouter: Heart of the Meta-Orchestrator

### 4.1 What It Does

When a user sends a message, the IntentRouter:

1. **Classifies** the intent (using the LLM in "planner" mode)
2. **Matches** against available agents/modes using `when_to_use` hints + `skills`
3. **Splits** compound requests into sub-tasks if needed
4. **Routes** each sub-task to the best agent/mode
5. **Streams** results back, with attribution

### 4.2 Intent Classification

```rust
pub struct IntentRouter {
    provider: Arc<dyn Provider>,
    agents: Vec<AgentSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub sub_tasks: Vec<SubTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub description: String,
    pub target_agent: String,      // agent name
    pub target_mode: Option<String>, // mode slug
    pub query: String,              // rewritten query for this agent
    pub depends_on: Vec<usize>,     // dependency ordering
}
```

### 4.3 Routing Prompt (Uses Existing Planner Mode)

The `GooseAgent` already has a `planner` mode (`plan.md` template, `ModeCategory::PromptOnly`). We extend it:

```markdown
You are Goose's intent router. Given a user message and a list of available agents,
decide which agent(s) should handle the request.

## Available Agents:
{{#each agents}}
### {{name}} ({{kind}})
{{description}}
{{#if modes}}
Modes:
{{#each modes}}
  - **{{slug}}**: {{description}}
    When to use: {{when_to_use}}
    Extensions: {{tool_groups}}
{{/each}}
{{/if}}
{{#if skills}}
Skills: {{#each skills}}{{name}}, {{/each}}
{{/if}}
{{/each}}

## Rules:
1. If the request maps to exactly ONE agent/mode, route directly
2. If the request has multiple distinct parts, split into sub-tasks
3. If no agent matches well, use the default "assistant" mode
4. Simple conversational messages (greetings, clarifications) → assistant
5. Return JSON with routing decision

## User Message:
{{user_message}}
```

### 4.4 Fast-Path Optimization

Not every message needs LLM routing. Implement a **fast-path classifier**:

```rust
impl IntentRouter {
    fn fast_path_route(&self, message: &str) -> Option<RoutingDecision> {
        // 1. If only ONE agent is registered → route to it (no LLM needed)
        if self.agents.len() == 1 {
            return Some(single_agent_route(&self.agents[0]));
        }
        
        // 2. Slash commands → always to assistant
        if message.starts_with('/') {
            return Some(assistant_route());
        }
        
        // 3. Short conversational messages → assistant
        if message.split_whitespace().count() <= 5 
           && !contains_action_keywords(message) {
            return Some(assistant_route());
        }
        
        // 4. Follow-up to current agent → stay with it
        // (session context tracking)
        
        None // Fall through to LLM routing
    }
}
```

---

## 5. Extension-Agent Binding

### 5.1 Data Model

```rust
/// An agent slot in the orchestrator — can be builtin, local ACP, or remote
#[derive(Clone)]
pub struct AgentSlot {
    pub id: String,
    pub name: String,
    pub kind: AgentSlotKind,
    pub description: String,
    pub bound_extensions: Vec<String>,
    pub modes: Vec<AgentMode>,
    pub skills: Vec<AgentSkill>,
    pub when_to_use: Vec<String>,  // aggregated from modes
    pub health: Arc<AgentHealth>,
    pub enabled: bool,
}

pub enum AgentSlotKind {
    /// A builtin agent running in-process (GooseAgent, CodingAgent)
    Builtin {
        agent: Arc<Agent>,  // shared Agent instance
        active_mode: String,
    },
    /// A local ACP agent spawned as a child process
    LocalAcp {
        handle: Arc<AgentHandle>,
    },
    /// A remote A2A agent accessed via HTTP
    RemoteA2a {
        endpoint: String,
        auth: Option<SecurityScheme>,
    },
}
```

### 5.2 Extension Binding Rules

```
┌─────────────────────────────────────────────┐
│ Orchestrator has ALL extensions available    │
│                                             │
│ ┌─────────────────────────────────────────┐ │
│ │ Goose Agent (assistant mode)            │ │
│ │   extensions: [developer, memory, fetch]│ │
│ └─────────────────────────────────────────┘ │
│                                             │
│ ┌─────────────────────────────────────────┐ │
│ │ Coding Agent (backend mode)             │ │
│ │   extensions: [developer, github,       │ │
│ │                code_execution]           │ │
│ └─────────────────────────────────────────┘ │
│                                             │
│ ┌─────────────────────────────────────────┐ │
│ │ External Agent (remote-analyzer)        │ │
│ │   extensions: [] (self-contained)       │ │
│ └─────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

Key rules:
1. **Builtin agents** use `recommended_extensions` from their mode definitions (already in CodingAgent)
2. **External ACP agents** are self-contained — they bring their own tools
3. **Users can override** bindings via UI (the existing extension toggle, scoped to agents)
4. **An extension can be bound to multiple agents** (e.g., `developer` used by both Goose and Coding agents)
5. **An agent can have 0 extensions** (e.g., a pure-LLM agent that only does text)

### 5.3 Implementation: Scoped ExtensionManager

```rust
// When routing to a builtin agent, create a scoped view of extensions
impl Agent {
    pub async fn reply_with_scoped_extensions(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        allowed_extensions: &[String],
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        // Temporarily set active_tool_groups to filter tools
        // This mechanism ALREADY EXISTS via active_tool_groups + tool_filter
        // We just need to wire it to the agent slot's bound_extensions
        
        // The existing prepare_reply_context already calls get_prefixed_tools
        // which respects active_tool_groups — we leverage this
        self.set_active_tool_groups(
            allowed_extensions.iter()
                .map(|e| ToolGroupAccess::Full(e.clone()))
                .collect()
        ).await;
        
        self.reply(user_message, session_config, cancel_token).await
    }
}
```

---

## 6. New AgentEvent Variants

```rust
#[derive(Clone, Debug)]
pub enum AgentEvent {
    // Existing
    Message(Message),
    McpNotification(AgentNotification),
    ModelChange { model: String, mode: String },
    HistoryReplaced(Conversation),
    
    // NEW: Routing transparency
    RoutingDecision {
        sub_tasks: Vec<SubTaskInfo>,
    },
    AgentDelegation {
        agent_name: String,
        agent_mode: Option<String>,
        task_description: String,
    },
    AgentDelegationComplete {
        agent_name: String,
        duration_ms: u64,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct SubTaskInfo {
    pub description: String,
    pub target_agent: String,
    pub target_mode: Option<String>,
}
```

---

## 7. UI Changes

### 7.1 Desktop: Agent Attribution Per Message

In `GooseMessage.tsx`, extend the footer to show which agent handled this part:

```
┌──────────────────────────────────────────────────┐
│  Here's the security analysis...                 │
│                                                  │
│  2:34 PM · gpt-4o · coding/security              │
│           ↑ model   ↑ agent/mode                 │
└──────────────────────────────────────────────────┘
```

### 7.2 Desktop: Routing Visualization

When the orchestrator splits a request, show a routing card:

```
┌─ 🧭 Routing Decision ────────────────────────┐
│                                                │
│ Your request was split into 2 tasks:           │
│                                                │
│ 1. 🛡️ Coding Agent (security)                 │
│    "Review auth.rs for SQL injection"          │
│                                                │
│ 2. ⚙️ Coding Agent (backend)                  │
│    "Implement the fix in auth.rs"              │
│    depends on: task 1                          │
│                                                │
└────────────────────────────────────────────────┘
```

### 7.3 Desktop: Agent Enable/Disable

The `AgentsView.tsx` already lists agents. Add a toggle:

```tsx
// In AgentsView.tsx, add an enable/disable toggle per agent
<Switch
  checked={agent.enabled}
  onCheckedChange={() => toggleAgent(agent.id)}
/>
```

This requires a new API endpoint: `POST /agents/builtin/{agent_id}/toggle`

### 7.4 CLI: Agent Attribution

```
─── coding/security · gpt-4o ───

Found 2 potential SQL injection vulnerabilities in auth.rs...

─── coding/backend · gpt-4o ───

I've fixed the SQL injection by using parameterized queries...
```

---

## 8. Implementation Phases

### Phase 1: Agent Slot Registry (2-3 days)
**Goal:** Agents can be enabled/disabled from UI

- [ ] Create `AgentSlotRegistry` in `crates/goose/src/orchestrator/`
- [ ] Populate from builtin agents (`GooseAgent`, `CodingAgent`) + external
- [ ] Add `POST /agents/{id}/enable`, `POST /agents/{id}/disable` routes
- [ ] Wire `AgentsView.tsx` toggle to new endpoints
- [ ] Add extension binding field to agent slots
- [ ] `just generate-openapi` + update UI types

### Phase 2: IntentRouter (3-5 days)
**Goal:** Messages are routed to the right agent/mode

- [ ] Create `IntentRouter` struct with fast-path + LLM routing
- [ ] Create routing prompt template (`router.md`)
- [ ] Insert router before `Agent::reply()` in the server reply path
- [ ] Emit `AgentEvent::RoutingDecision` for UI transparency
- [ ] Handle single-task (direct route) and multi-task (split) paths
- [ ] Fast-path: single agent, slash commands, short messages

### Phase 3: Scoped Extension Binding (2-3 days)
**Goal:** Each agent only sees its bound extensions

- [ ] Extend `AgentSlot` with `bound_extensions: Vec<String>`
- [ ] When routing to a builtin agent, call `set_active_tool_groups` with scoped extensions
- [ ] UI: per-agent extension binding in `AgentsView.tsx`
- [ ] Persist bindings in session metadata

### Phase 4: UI Visualization (2-3 days)
**Goal:** Users see routing decisions and agent attribution

- [ ] Handle `RoutingDecision` SSE event in `useChatStream.ts`
- [ ] Create `RoutingCard` component for split visualization
- [ ] Extend `GooseMessage.tsx` footer with agent/mode attribution
- [ ] CLI: show agent attribution line (`─── agent/mode · model ───`)

### Phase 5: Multi-Task Orchestration (3-5 days)
**Goal:** Compound requests are split and executed (parallel or sequential)

- [ ] Implement dependency-aware task execution in `IntentRouter`
- [ ] Parallel execution for independent sub-tasks
- [ ] Sequential execution respecting `depends_on`
- [ ] Result aggregation into coherent response
- [ ] Error handling: partial failure of sub-tasks

### Phase 6: External Agent Integration (2-3 days)
**Goal:** External ACP/A2A agents participate in routing

- [ ] Register external agents in `AgentSlotRegistry` alongside builtins
- [ ] Router considers external agent skills for matching
- [ ] `AgentClientManager` handles delegation to external agents
- [ ] Health monitoring affects routing decisions (avoid degraded agents)

---

## 9. File Changes Map

| File | Change |
|------|--------|
| **NEW** `crates/goose/src/orchestrator/mod.rs` | Module root |
| **NEW** `crates/goose/src/orchestrator/intent_router.rs` | Intent classification + routing |
| **NEW** `crates/goose/src/orchestrator/agent_slot.rs` | AgentSlot, AgentSlotRegistry |
| **NEW** `crates/goose/src/orchestrator/routing_prompt.rs` | Prompt template for routing |
| `crates/goose/src/agents/agent.rs` | Add `reply_with_scoped_extensions`, new `AgentEvent` variants |
| `crates/goose/src/agents/mod.rs` | Export orchestrator module |
| `crates/goose-server/src/routes/agent_management.rs` | Add enable/disable/bind-extension routes |
| `crates/goose-server/src/routes/reply.rs` | Insert router before agent reply |
| `crates/goose-server/src/state.rs` | Add `AgentSlotRegistry` to `AppState` |
| `ui/desktop/src/components/agents/AgentsView.tsx` | Enable/disable toggle + extension binding |
| `ui/desktop/src/hooks/useChatStream.ts` | Handle `RoutingDecision`, `AgentDelegation` events |
| `ui/desktop/src/components/GooseMessage.tsx` | Agent/mode in footer attribution |
| **NEW** `ui/desktop/src/components/RoutingCard.tsx` | Routing visualization component |
| `crates/goose-cli/src/session/mod.rs` | CLI: agent attribution, routing display |

---

## 10. Migration Strategy

### Backward Compatibility

The meta-orchestrator is **additive**. When only one agent is registered (the default), the fast-path routes everything to it — behavior is identical to today.

```rust
impl IntentRouter {
    pub async fn route(&self, message: &Message) -> RoutingDecision {
        // Fast path: single agent = no routing needed
        if self.agents.len() == 1 {
            return RoutingDecision::single(self.agents[0].clone());
        }
        // ... LLM routing for multi-agent
    }
}
```

### Opt-In Activation

Multi-agent routing is only active when:
1. Multiple agents are enabled (builtin or external)
2. Config flag `GOOSE_ENABLE_ROUTING=true` (default: false initially)

---

## 11. Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Routing adds latency (extra LLM call) | Fast-path bypasses LLM for simple cases; cache routing decisions for follow-up messages |
| Incorrect routing sends task to wrong agent | Fallback: if agent fails, re-route to default assistant |
| Extension conflicts (same tool in multiple agents) | Tool names are already prefixed (`developer__shell`); scoping prevents conflicts |
| Context loss between agents | Orchestrator maintains a shared conversation; agents see relevant context |
| Breaking existing single-agent users | Fast-path = identical behavior when one agent |

---

## 12. Open Questions

1. **Should the router use the same LLM as the agents, or a cheaper/faster model?**
   - Recommendation: Use the same provider but with a simpler prompt (PromptOnly mode)

2. **How to handle session state across agent switches?**
   - Recommendation: Shared conversation in session; each agent sees the full history

3. **Should routing decisions be persisted in session metadata?**
   - Recommendation: Yes, for replay and debugging

4. **What happens when an external agent goes unhealthy mid-conversation?**
   - Recommendation: Fall back to builtin assistant with a notification to the user
