# Architecture (Goose core — facts)

Last updated: 2026-01-31

## Crate / module layout
- Core library crate is `crates/goose`, which exports modules including `agents`, `config`, `execution`, `recipe`, `scheduler`, `scheduler_trait`, and `session`. (`crates/goose/src/lib.rs:L1-L26`)
- CLI binary entrypoint: `crates/goose-cli/src/main.rs` calls `goose_cli::cli::cli()`. (`crates/goose-cli/src/main.rs:L4-L11`)
- Server binary entrypoint: `crates/goose-server/src/main.rs` dispatches to `commands::agent::run()` for the HTTP server, and can also run bundled MCP servers. (`crates/goose-server/src/main.rs:L25-L64`)

## Execution wiring (Agent / sessions)
- `Agent` is the core runtime object used to run conversation turns, tool calls, and extensions. Construction uses `AgentConfig` which includes a shared `Arc<SessionManager>` and optional scheduler handle. (`crates/goose/src/agents/agent.rs:L182-L215`, `crates/goose/src/execution/manager.rs:L85-L93`)
- `AgentManager` caches per-session agents and creates them on demand via `get_or_create_agent(session_id)`. If a default provider is set, it is applied to newly created agents via `agent.update_provider(...)`. (`crates/goose/src/execution/manager.rs:L77-L107`)
- Sessions are persisted through `SessionManager`, which wraps `SessionStorage`. (`crates/goose/src/session/session_manager.rs:L243-L258`)
- Server state delegates to the core `AgentManager` for access to the scheduler and session manager. (`crates/goose-server/src/state.rs:L28-L84`)

## Recipe discovery / loading / build
### Local discovery
- Local recipe discovery is implemented in `crates/goose/src/recipe/local_recipes.rs` and searches:
  - current working directory `./`
  - optional extra dirs from `GOOSE_RECIPE_PATH`
  - global library `Paths::config_dir()/recipes`
  - working-dir library `$PWD/.goose/recipes`
  (`crates/goose/src/recipe/local_recipes.rs:L11-L37`)

### GitHub-backed discovery (CLI)
- The CLI can fall back to loading recipes from a configured GitHub repository (`GOOSE_RECIPE_GITHUB_REPO`) after failing to load locally. (`crates/goose-cli/src/recipes/search_recipe.rs:L11-L18`, `crates/goose-cli/src/recipes/github_recipe.rs:L31-L35`)

### Recipe file handling
- Reading a recipe file yields `RecipeFile { content, parent_dir, file_path }` via `read_recipe_file`. (`crates/goose/src/recipe/read_recipe_file_content.rs:L6-L37`)
- Template validation entrypoints: `validate_recipe_template_from_file` and `validate_recipe_template_from_content`. (`crates/goose/src/recipe/validate_recipe.rs:L29-L54`)
- Building/rendering recipes from templates is handled by `build_recipe_from_template` and friends; sub-recipe paths are resolved relative to the parent recipe directory unless absolute. (`crates/goose/src/recipe/build_recipe/mod.rs:L80-L118`, `crates/goose/src/recipe/build_recipe/mod.rs:L158-L174`)

### Slash command → recipe mapping
- Slash command mappings are stored in config under key `slash_commands` and resolve a command to a recipe path via `get_recipe_for_command`. (`crates/goose/src/slash_commands.rs:L10-L33`, `crates/goose/src/slash_commands.rs:L55-L62`)

## Skill discovery (SKILL.md)
- Skills are exposed via the "Skills" extension (`SkillsClient`) which loads built-in skills, then scans default skill directories for subdirectories containing `SKILL.md`. (`crates/goose/src/agents/skills_extension.rs:L69-L80`, `crates/goose/src/agents/skills_extension.rs:L101-L118`, `crates/goose/src/agents/skills_extension.rs:L179-L199`)

## Scheduler model (built-in)
- Scheduler API is abstracted by `SchedulerTrait` (add/list/remove/pause/unpause/run-now/schedule-recipe/etc.). (`crates/goose/src/scheduler_trait.rs:L9-L41`)
- Default scheduler persistence is a JSON file at `Paths::data_dir()/schedule.json`. (`crates/goose/src/scheduler.rs:L31-L35`)
- `ScheduledJob` records include `id`, `source` (recipe file path), `cron`, and runtime metadata (paused/running/session id). (`crates/goose/src/scheduler.rs:L103-L117`)
- When adding a schedule with `make_copy=true`, the scheduler copies the source recipe into `Paths::data_dir()/scheduled_recipes/<job_id>.<ext>` and updates `stored_job.source` to point at the copy. (`crates/goose/src/scheduler.rs:L278-L313`)
- Server schedule creation endpoint builds a `ScheduledJob` and calls `scheduler.add_scheduled_job(job, true)`. (`crates/goose-server/src/routes/schedule.rs:L87-L120`)

## Scheduled execution wiring
- Scheduler execution reads the recipe file at `job.source`, parses it (YAML by default; JSON/JSONL if extension matches), creates an `Agent`, resolves extensions, creates a `Session` of type `Scheduled`, then runs `agent.reply(...)` with a `SessionConfig` containing `schedule_id`. (`crates/goose/src/scheduler.rs:L710-L799`)
- After completion, the scheduler updates the stored session record with `schedule_id` and the parsed `Recipe`. (`crates/goose/src/scheduler.rs:L822-L829`)
