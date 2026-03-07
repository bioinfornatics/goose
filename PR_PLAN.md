# PR Split Plan — Revised for v1.27.2 Architecture

## Architecture Delta: Our Branch vs Upstream v1.27.2

### Upstream v1.27.2 Has
- **Single Agent** model with `platform_extensions` (apps, summon, developer, chatrecall, code_execution, todo, tom)
- **Subagent system** (`subagent_handler`) for sub-task delegation
- **`reasoning: Option<bool>`** on ModelConfig (simple on/off)
- **`PromptManager`** for system prompts
- **`execution/` module** for agent lifecycle
- **No multi-agent routing**, no specialist agents, no agent modes
- **No pipeline/workflow**, no eval framework, no A2A crate, no agent catalog

### Our Branch Has (that upstream doesn't)
- Multi-agent architecture (Developer, QA, PM, Security, Research)
- AgentSlotRegistry with SQLite backing
- LLM-based OrchestratorAgent routing
- IntentRouter with semantic TF-IDF
- Agent modes (ask, plan, write, review, debug)
- DAG dispatch (dispatch.rs) for compound tasks
- Pipeline model + executor + visual editor
- A2A protocol crate for remote agents
- ReasoningEffort enum (Low/Medium/High) with per-agent/mode overrides
- Eval framework with SSE streaming
- Agent catalog API + UI

### Strategy: Additive PRs Only

Instead of replacing upstream's architecture, we contribute **additive features** that
enhance the existing single-agent model. Features requiring deep architectural changes
are deferred to a design RFC.

---

## Revised PR Plan (5 PRs, additive)

### PR-01: Dynamic Reasoning Effort (CLEANEST, START HERE)
**Risk: 🟢 LOW** — Pure addition to existing ModelConfig, no conflicts

Upstream has `reasoning: Option<bool>` (on/off). We add `ReasoningEffort` enum
(Low/Medium/High) with provider-specific mapping and dynamic control.

**Rust changes:**
- `model.rs`: Add `ReasoningEffort` enum, `reasoning_effort: Option<ReasoningEffort>` field,
  `parse_reasoning_effort()`, `with_reasoning_effort()` builder
- `providers/formats/openai.rs`: Live env var check in `create_request` for O-series models
- `providers/formats/anthropic.rs`: Live env var check → `thinking.budget_tokens` override
- `config_management.rs`: `GET/POST /config/reasoning-effort` endpoints
- `state.rs`: `reasoning_effort` field in AppState
- `openapi.rs`: Register new types/paths

**UI changes:**
- `ReasoningEffortSection.tsx`: Global selector (Low/Medium/High)
- `ReasoningEffortSelectionItem.tsx`: Radio button component
- `ChatSettingsSection.tsx`: Wire in new section
- `reasoningEffortUtils.ts`: Model support detection

**Tests:** ~15 (Rust unit + server integration)
**ETA:** 1-2 days
**Conflict risk:** LOW — only touches model.rs (adds field), format files (adds logic), new routes

### PR-02: Pipeline Model & Visual Editor (STANDALONE)
**Risk: 🟡 MEDIUM** — New module, no conflicts with existing code

Add pipeline YAML model, validation, CRUD API, and visual DAG editor.
No execution engine yet (that requires dispatch infrastructure).

**Rust changes:**
- `pipeline.rs`: NEW — Pipeline, PipelineNode, NodeKind, validation, cycle detection, CRUD
- `lib.rs`: Add `pub mod pipeline;`
- `routes/pipeline.rs`: NEW — list, get, save, update, delete, validate endpoints
- `routes/mod.rs`: Register pipeline routes
- `openapi.rs`: Register pipeline types/paths

**UI changes:**
- `workflows/types.ts`: Pipeline/node types
- `workflows/serialization.ts`: Flow ↔ Pipeline conversion
- `workflows/DagEditor.tsx`: ReactFlow canvas with drag-drop, undo/redo, save/export
- `workflows/nodes/index.tsx`: Styled node components
- `workflows/panels/NodePalette.tsx`: Draggable node palette with templates
- `workflows/panels/PropertiesPanel.tsx`: Node property editor
- `workflows/panels/TemplateGallery.tsx`: Pre-built pipeline templates
- `PipelineManager.tsx`: Pipeline listing CRUD
- `WorkflowsPage.tsx`: Page wrapper

**Tests:** ~50 (22 Rust unit + 9 Rust integration + 34 TypeScript)
**ETA:** 2-3 days
**Conflict risk:** LOW — entirely new files, only touches lib.rs and routes/mod.rs

### PR-03: Pipeline Execution Engine (DEPENDS ON PR-02)
**Risk: 🟡 MEDIUM** — Uses subagent_handler for execution

Maps pipeline nodes to subagent tasks and executes via existing subagent system.
Uses upstream's `run_subagent_task` instead of our custom dispatch.

**Rust changes:**
- `pipeline_executor.rs`: NEW — pipeline_to_tasks, execute_pipeline, PipelineEvent SSE
- `routes/pipeline.rs`: Add `POST /pipelines/{id}/run` SSE endpoint

**UI changes:**
- `DagEditor.tsx`: Run/Stop button, SSE streaming, node status updates

**Tests:** ~25 (unit + integration + E2E)
**ETA:** 2-3 days
**Conflict risk:** LOW — new files only, integrates with existing subagent system

### PR-04: A2A Agent Discovery (STANDALONE)
**Risk: 🟡 MEDIUM** — New crate + routes

Add A2A (Agent-to-Agent) protocol support for discovering and delegating to remote agents.

**Rust changes:**
- `crates/a2a/`: NEW crate — types, client (HTTP), server (transport, task store)
- `routes/a2a.rs`: NEW — discover endpoint, persona listing
- `Cargo.toml`: Add a2a dependency

**UI changes:**
- Enhanced A2A node in pipeline editor (discovery + skill selector)

**Tests:** ~14 (A2A server tests)
**ETA:** 2-3 days
**Conflict risk:** LOW — new crate, new route file

### PR-05: Eval & Analytics Framework (STANDALONE)
**Risk: 🟡 MEDIUM** — New routes + UI

Add routing evaluation datasets, runs, and analytics dashboard.

**Rust changes:**
- `session/eval_storage.rs`: NEW — SQLite-backed eval storage
- `agents/routing_eval.rs`: NEW — evaluate orchestrator with streaming
- `routes/analytics.rs`: NEW — eval endpoints with SSE streaming

**UI changes:**
- `analytics/DatasetsTab.tsx`: Dataset CRUD, eval run with SSE, tabbed results

**Tests:** ~23 (unit + integration + E2E)
**ETA:** 2-3 days
**Conflict risk:** LOW — entirely new files

---

## Dependency Graph

```
PR-01 Reasoning Effort    (standalone, start immediately)
PR-02 Pipeline Model      (standalone, start immediately)
PR-04 A2A Discovery       (standalone, start immediately)
PR-05 Eval Framework      (standalone, start immediately)
  │
  └── PR-03 Pipeline Execution  (depends on PR-02)
```

## Execution Order

| Wave | PRs | Parallelizable? | ETA |
|------|-----|-----------------|-----|
| **Wave 1** | PR-01, PR-02, PR-04, PR-05 | YES — all independent | 2-3 days |
| **Wave 2** | PR-03 | After PR-02 lands | 2-3 days |

**Total: 4-6 days**

## What's Deferred (needs RFC)

| Feature | Why |
|---------|-----|
| Multi-agent routing (OrchestratorAgent, IntentRouter) | Architectural — upstream uses single-agent + subagents |
| Agent Slot Registry | Architectural — upstream has no multi-agent slots |
| Developer/QA/PM/Security/Research agents | Architectural — upstream has single agent with extensions |
| Agent modes (ask/plan/write/review/debug) | Architectural — no mode concept in upstream |
| Per-agent/mode reasoning effort overrides | Depends on agent modes (not in upstream) |
| Design System overhaul (59 commits) | Too divergent from upstream UI |
| Auth/Identity (31 commits) | Upstream may have different approach |
| Sidebar/Navigation (45 commits) | UI-specific, high conflict |

## Key Principle

**Additive, not replacing.** Each PR adds new capability without modifying
upstream's core agent architecture. The multi-agent system can be proposed
separately via an RFC once the foundation PRs are merged.
