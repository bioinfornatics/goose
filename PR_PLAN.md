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

### Implementation Rule: Reuse Before Rewrite

**Always check `feature/cli-via-goosed` first** before writing any new code.

1. **Extract** — `git show feature/cli-via-goosed:path/to/file` to retrieve existing implementations
2. **Assess** — Identify what needs adaptation for upstream v1.27.2 compatibility (imports, state shape, API patterns, removed multi-agent dependencies)
3. **Adapt minimally** — Only change what doesn't compile against upstream; preserve original design decisions, naming, structure
4. **Write from scratch only** when no equivalent exists on the feature branch

This preserves `git blame` continuity, respects original design intent, and avoids
introducing unnecessary divergence between the feature branch and upstream PRs.

---

## PR Submission Order

> **Note (updated):** Pipeline PRs (02, 03, 06) are deferred to after all
> infrastructure PRs land. Pipeline nodes cannot link to real agents until
> the agent architecture, registry, and routing infrastructure are in place.
> Branch names are unchanged — only submission order is affected.

### Wave 1: Standalone Foundation PRs (submit in parallel)

| Branch | Feature | Risk | Status |
|--------|---------|------|--------|
| `pr/01-reasoning-effort` | Dynamic reasoning effort (Low/Medium/High) for OpenAI + Anthropic | 🟢 LOW | Ready |
| `pr/04-a2a-discovery` | A2A Agent Discovery protocol crate | 🟡 MED | Ready |
| `pr/05-eval-analytics` | Eval & Analytics framework | 🟡 MED | Ready |
| `pr/07-analyze-tool` | Developer Analyze Tool — multi-language AST | 🟡 MED | Ready |

### Wave 2: Core Infrastructure PRs (submit sequentially)

| Branch | Feature | Risk |
|--------|---------|------|
| `pr/08-editor-models` | Editor models (MorphLLM, OpenAI-compat, Relace) | 🟡 MED |
| `pr/09-acp-compat` | ACP v0.2.0 compatibility layer | 🟡 MED |
| `pr/10-policy-quotas` | Identity, Policy & Quotas framework | 🟡 MED |
| `pr/11-goosed-client` | GoosedClient for CLI-to-goosed | 🟡 MED |
| `pr/12-genui-workspace` | GenUI MCP service and Workspace | 🟡 MED |
| `pr/13-audit` | Audit event system | 🟡 MED |
| `pr/14-registry` | Extension Registry system | 🟡 MED |
| `pr/15-prompts` | Prompt templates for specialist agents | 🟢 LOW |
| `pr/16-developer-mcp` | Developer MCP server | 🟡 MED |
| `pr/17-session-analytics` | Eval storage and tool analytics | 🟡 MED |
| `pr/18-tunnel-refactor` | Tunnel proxy refactor + integration tests | 🟡 MED |
| `pr/19-computercontroller-simplify` | Simplify ComputerController | 🟢 LOW |
| `pr/20-server-fixes` | Server route improvements | 🟢 LOW |
| `pr/21-token-counter-simplify` | Simplify tokenizer initialization | 🟢 LOW |
| `pr/22-provider-trait-v2` | Provider trait v2 — model.rs + base.rs | 🔴 HIGH |
| `pr/23-message-model` | Message model — JsonRenderSpec, remove Reasoning | 🟡 MED |
| `pr/24-auth-oidc` | OIDC authentication and session tokens | 🟡 MED |
| `pr/25-providers-update` | Update all providers for Provider trait v2 | 🔴 HIGH |
| `pr/26-appstate-infra` | Extend AppState with auth, policy, registry, agent slots | 🟡 MED |
| `pr/27-agent-architecture` | Multi-agent architecture — orchestrator, routing | 🔴 HIGH |
| `pr/28-server-routes` | All server route handlers for multi-agent goosed | 🟡 MED |
| `pr/29-goose-cli` | CLI-via-goosed — rewrite CLI to use HTTP | 🔴 HIGH |

### Wave 3: Tests, Config, UI, Docs

| Branch | Feature | Risk |
|--------|---------|------|
| `pr/30-tests-examples` | Updated tests, examples, MCP replay data | 🟢 LOW |
| `pr/31-remaining-modules` | Remaining modules — dictation, gateway, security, tracing | 🟡 MED |
| `pr/32-goose-acp` | Remove goose-acp crate (moved to acp_compat) | 🟢 LOW |
| `pr/33-root-config` | Root config files — Cargo.toml, Justfile, deny.toml | 🟢 LOW |
| `pr/34-ci-workflows` | CI/CD workflows | 🟢 LOW |
| `pr/35-scripts` | Scripts and build tools | 🟢 LOW |
| `pr/36-docs` | Documentation for multi-agent architecture | 🟢 LOW |
| `pr/37-ui-atoms` | UI Atomic Design system — atoms, molecules, icons | 🟡 MED |
| `pr/38-ui-organisms` | UI organisms, pages, layouts, templates | 🟡 MED |
| `pr/39-ui-infra` | UI infrastructure — utils, hooks, contexts, API, tests | 🟡 MED |
| `pr/40-ui-config` | UI config, core modules, remaining files | 🟡 MED |
| `pr/41-arch-docs` | Architecture docs, design docs, reviews | 🟢 LOW |
| `pr/42-misc` | Miscellaneous — config, evals, observability, recipes | 🟢 LOW |

### Wave 4: Pipeline PRs (DEFERRED — after agent infrastructure lands)

> **Rationale:** Pipeline nodes (Agent, Condition, Trigger, A2A) need to link
> to real agents via the agent registry, routing infrastructure, and A2A
> discovery. Without those, the pipeline editor is a visual shell with no
> execution capability. These PRs are ready but should be submitted last.

| Branch | Feature | Depends On | Risk |
|--------|---------|-----------|------|
| `pr/02-pipeline-model` | Pipeline model, DAG editor, CRUD API | Agent registry, A2A | 🟡 MED |
| `pr/06-pipeline-templates` | Pipeline templates, types, serialization | pr/02 | 🟡 MED |
| `pr/03-pipeline-executor` | Pipeline execution engine with DAG scheduler + SSE | pr/02, pr/06, agent routing | 🟡 MED |

---

## Dependency Graph

```
Wave 1 (parallel, standalone):
  PR-01 Reasoning Effort
  PR-04 A2A Discovery
  PR-05 Eval Framework
  PR-07 Analyze Tool

Wave 2 (sequential, infrastructure):
  PR-08..PR-29 (core infra, providers, agent architecture, CLI)

Wave 3 (tests, UI, docs):
  PR-30..PR-42

Wave 4 (after agent infra lands):
  PR-02 Pipeline Model
    └── PR-06 Pipeline Templates
        └── PR-03 Pipeline Executor
```

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
