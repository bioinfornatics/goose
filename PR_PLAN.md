# PR Split Plan — feature/cli-via-goosed → upstream/main

## Context

- **Source branch:** `feature/cli-via-goosed` (574 commits, 1194 files since v1.23.2)
- **Target:** `upstream/main` (currently v1.27.2)
- **Overlap:** 239 files modified by both us and upstream → expect merge conflicts
- **Upstream delta:** v1.23.2 → v1.27.2 = 412 commits, 587 files changed

## Strategy

1. **Update local main** to upstream v1.27.2
2. **Create fresh feature branches** from updated main (not from our messy 574-commit history)
3. **Cherry-pick or rewrite** each PR's changes cleanly onto the fresh branch
4. **Resolve conflicts** at PR creation time, not at merge time
5. **Order PRs by dependency** — foundational infrastructure first, UI features last

## PR Dependency Graph

```
PR-01 (Agent Infrastructure)
  ├─ PR-02 (Routing Engine)
  │   └─ PR-05 (Eval Framework)
  ├─ PR-03 (Developer Agent)
  │   └─ PR-04 (Reasoning Effort)
  ├─ PR-06 (A2A Protocol)
  │   └─ PR-08 (Workflow DAG Builder)
  └─ PR-07 (Agent Catalog UI)
      └─ PR-08 (Workflow DAG Builder)
```

---

## PR-01: Agent Slot Registry & Multi-Agent Infrastructure
**Risk: HIGH** (56 upstream commits in agents/, heavy overlap)

### Commits (~25)
- Agent slot registry with SQLite backing
- Universal modes (ask/plan/write/review) across all agents
- CodingAgent → DeveloperAgent migration
- QA/PM/Security/Research agent definitions
- Extension registry refactoring (ToolRegistry, ExtensionRegistry)
- All 6 agents as builtin in agent management

### Key Files
| File | Conflict Risk |
|------|--------------|
| `crates/goose/src/agents/agent.rs` | 🔴 HIGH — upstream added platform extensions |
| `crates/goose/src/agents/mod.rs` | 🔴 HIGH |
| `crates/goose/src/agents/goose_agent.rs` | 🟡 MEDIUM |
| `crates/goose/src/agents/intent_router.rs` | 🟡 MEDIUM |
| `crates/goose-server/src/routes/agent_management.rs` | 🟡 MEDIUM |
| `crates/goose/src/registry/` | 🟢 NEW (no upstream) |

### Why First
Everything depends on the agent slot registry. Routing, eval, catalog, and workflows all query `configured_slots()`.

---

## PR-02: LLM-Primary Routing Engine
**Risk: MEDIUM** (routing files are ours, prompts overlap)

### Commits (~30)
- Remove keyword layer, make LLM primary router
- TF-IDF semantic router (Layer 2)
- Routing feedback loop
- XML-structured orchestrator prompts
- Dynamic routing guidelines from agent catalog
- Common verb penalty + PM enrichment
- GraphRAG-lite knowledge extraction

### Key Files
| File | Conflict Risk |
|------|--------------|
| `crates/goose/src/agents/orchestrator_agent.rs` | 🟡 MEDIUM |
| `crates/goose/src/agents/semantic_router.rs` | 🟢 NEW |
| `crates/goose/src/prompts/orchestrator/` | 🟡 MEDIUM — upstream changed system prompt |
| `crates/goose/src/agents/intent_router.rs` | 🟡 MEDIUM |

### Depends On
PR-01 (agent slots for catalog-driven guidelines)

---

## PR-03: Developer Agent Modes & Apps Integration
**Risk: LOW** (mostly new files + config changes)

### Commits (~12)
- Move app_maker/app_iterator to Developer Agent
- Create/remove AppBuilderAgent
- Developer ask mode gets diagnostics + figma
- Deduplicate tool groups
- 5 universal modes (ask/plan/write/review/debug)

### Key Files
| File | Conflict Risk |
|------|--------------|
| `crates/goose/src/agents/developer_agent.rs` | 🟢 NEW |
| `crates/goose/src/agents/goose_agent.rs` | 🟡 MEDIUM |
| `crates/goose/src/prompt_template.rs` | 🟡 MEDIUM |

### Depends On
PR-01 (universal modes definition)

---

## PR-04: Dynamic Reasoning Effort
**Risk: LOW** (mostly new files, minimal overlap)

### Commits (~6)
- ReasoningEffort enum + ModelConfig integration
- OpenAI + Anthropic live env var override
- GET/POST /config/reasoning-effort API
- Per-agent/mode overrides API
- Settings UI (global + per-mode)
- Model support detection (disable UI for unsupported models)

### Key Files
| File | Conflict Risk |
|------|--------------|
| `crates/goose/src/model.rs` | 🟡 MEDIUM (4 upstream commits) |
| `crates/goose/src/providers/formats/openai.rs` | 🟡 MEDIUM |
| `crates/goose/src/providers/formats/anthropic.rs` | 🟡 MEDIUM |
| `crates/goose-server/src/routes/config_management.rs` | 🟢 LOW |
| `ui/desktop/src/components/settings/reasoning_effort/` | 🟢 NEW |

### Depends On
PR-03 (Developer Agent modes for per-mode effort)

---

## PR-05: Eval & Analytics Framework
**Risk: MEDIUM** (analytics routes overlap with upstream)

### Commits (~20)
- Routing eval with real OrchestratorAgent
- Golden routing dataset (50 cases)
- Streaming eval via SSE
- DatasetsTab redesign with tabbed results
- SQLite timestamp parsing fix
- Dataset editor YAML bidirectional parsing
- 23 UI tests for DatasetsTab

### Key Files
| File | Conflict Risk |
|------|--------------|
| `crates/goose/src/agents/routing_eval.rs` | 🟢 LOW |
| `crates/goose/src/session/eval_storage.rs` | 🟢 LOW |
| `crates/goose-server/src/routes/analytics.rs` | 🟡 MEDIUM |
| `ui/desktop/src/components/organisms/analytics/DatasetsTab.tsx` | 🟢 LOW |
| `evals/routing-golden-dataset.yaml` | 🟢 NEW |

### Depends On
PR-02 (routing engine for eval)

---

## PR-06: A2A Protocol & Integration
**Risk: MEDIUM** (A2A crate is ours, but routes overlap)

### Commits (~30)
- Standalone A2A protocol crate
- A2A server routes + push notifications
- Agent card discovery endpoint (POST /a2a/discover)
- Per-persona A2A endpoints
- RemoteA2AAgent delegation strategy
- A2A dispatch in compound execution
- Agent card UI fixes (nesting, colors, dedup, description)

### Key Files
| File | Conflict Risk |
|------|--------------|
| `crates/a2a/` | 🟢 NEW (entire crate) |
| `crates/goose-server/src/routes/a2a.rs` | 🟡 MEDIUM |
| `crates/goose/src/agents/dispatch.rs` | 🟡 MEDIUM |
| `ui/desktop/src/components/organisms/agents/AgentsView.tsx` | 🟢 NEW |

### Depends On
PR-01 (agent slots for persona mapping)

---

## PR-07: Agent Catalog UI
**Risk: LOW** (mostly new UI components)

### Commits (~10)
- AgentCatalog redesign with atomic design system
- 12 tool group colors
- Agent card layout fixes
- Built-in agent toggle (On/Off)

### Key Files
| File | Conflict Risk |
|------|--------------|
| `ui/desktop/src/components/organisms/analytics/AgentCatalog.tsx` | 🟢 NEW |
| `ui/desktop/src/components/organisms/agents/AgentsView.tsx` | 🟢 NEW |

### Depends On
PR-01 (agent catalog API)

---

## PR-08: Visual Workflow DAG Builder
**Risk: LOW** (entirely new feature, no upstream overlap)

### Commits (~25)
- Pipeline executor (pipeline_to_tasks + DAG dispatch)
- Execute endpoint (POST /pipelines/{id}/run with SSE)
- Agent catalog binding on workflow nodes
- A2A discovery in workflow nodes
- Template gallery (5 templates)
- Auto-layout, keyboard shortcuts, edge validation
- 94 tests (22 Rust unit + 23 Rust integration + 49 TypeScript)

### Key Files
| File | Conflict Risk |
|------|--------------|
| `crates/goose/src/pipeline_executor.rs` | 🟢 NEW |
| `crates/goose/src/pipeline.rs` | 🟢 LOW (existing, our additions) |
| `crates/goose-server/src/routes/pipeline.rs` | 🟢 LOW |
| `ui/desktop/src/components/organisms/workflows/` | 🟢 NEW |

### Depends On
PR-06 (A2A discovery), PR-07 (Agent catalog dropdown)

---

## Execution Steps

### Step 0: Prepare
```bash
# Update main to upstream
git fetch upstream
git checkout main
git reset --hard upstream/main  # v1.27.2

# Verify clean state
cargo build
cargo test
```

### Step 1: Create PR branches (in order)
```bash
# For each PR:
git checkout -b pr/01-agent-infrastructure main
# Cherry-pick or manually apply commits
# Resolve conflicts
# Run quality gates: cargo fmt, clippy, tsc, tests
# Push and create PR

git checkout -b pr/02-routing-engine main
# Merge pr/01 first (or wait for it to land)
# ...
```

### Step 2: Quality gate per PR
```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p goose
cargo test -p goose-server
cd ui/desktop && npx tsc --noEmit && npx vitest run
```

### Step 3: PR review order
1. PR-01 → land first (foundation)
2. PR-02 + PR-03 + PR-06 → can go in parallel (independent)
3. PR-04 + PR-05 + PR-07 → after their dependencies land
4. PR-08 → last (depends on everything)

---

## Risk Matrix

| PR | Size | Conflict Risk | Review Complexity | ETA |
|----|------|--------------|-------------------|-----|
| PR-01 | L | 🔴 HIGH | Hard (core infra) | 2-3 days |
| PR-02 | L | 🟡 MEDIUM | Medium (routing) | 1-2 days |
| PR-03 | M | 🟢 LOW | Easy | 0.5 day |
| PR-04 | S | 🟢 LOW | Easy | 0.5 day |
| PR-05 | M | 🟡 MEDIUM | Medium | 1 day |
| PR-06 | L | 🟡 MEDIUM | Medium (new crate) | 1-2 days |
| PR-07 | S | 🟢 LOW | Easy | 0.5 day |
| PR-08 | L | 🟢 LOW | Medium (new feature) | 1 day |

**Total estimated effort: 7-10 days**

---

## Excluded from PRs (parking lot)

These features from our branch are **not included** in the PR plan because they're either experimental, UI-only polish that diverges significantly from upstream's direction, or would create too many conflicts:

- **Design System overhaul** (59 commits) — Too much UI churn, upstream has its own direction
- **Auth/Identity system** (31 commits) — Full OIDC stack, needs separate discussion
- **Sidebar navigation** (45 commits) — Heavy UI rework, likely conflicts everywhere
- **PromptBar/UnifiedInput** (26 commits) — Core chat input rewrite, risky
- **Activity panel** (24 commits) — Depends on sidebar + design system
- **Observatory/Monitoring** (10 commits) — Depends on agent pool
- **Knowledge Graph docs** (13 commits) — Documentation only, low priority
- **CLI service subcommand** (11 commits) — Needs discussion on approach

These can be separate follow-up PRs after the core 8 land.
