# Tech (Goose core — facts)

Last updated: 2026-01-31

## Recipe discovery
### Local recipes (core)
Local recipe lookup and listing are implemented in `crates/goose/src/recipe/local_recipes.rs`.

Search directories (deduped after canonicalization):
- Current directory: `./`
- Extra dirs from `GOOSE_RECIPE_PATH` (split by `:` on Unix, `;` on Windows)
- Global recipe library: `Paths::config_dir()/recipes`
- Working-dir recipe library: `$PWD/.goose/recipes`
(`crates/goose/src/recipe/local_recipes.rs:L11-L37`)

Global vs local library paths:
- Global: `Paths::config_dir().join("recipes")`
- Local: `env::current_dir()?.join(".goose/recipes")`
(`crates/goose/src/recipe/local_recipes.rs:L13-L18`)

### GitHub recipes (CLI)
The CLI first attempts local load, then (if configured) falls back to GitHub:
- Config key: `GOOSE_RECIPE_GITHUB_REPO` (stored in `Config` and optionally sourced from env). (`crates/goose-cli/src/recipes/github_recipe.rs:L31-L35`, `crates/goose-cli/src/commands/configure.rs:L1572-L1591`)
- Load pipeline: `load_local_recipe_file(...)` then `retrieve_recipe_from_github(...)` when repo is configured. (`crates/goose-cli/src/recipes/search_recipe.rs:L11-L18`, `crates/goose-cli/src/recipes/search_recipe.rs:L21-L26`)

## Recipe file handling
- File load helper: `read_recipe_file(...) -> RecipeFile { content, parent_dir, file_path }`. (`crates/goose/src/recipe/read_recipe_file_content.rs:L6-L37`)
- Template validation parses template variables and validates constraints (e.g., requires at least one of `instructions` or `prompt`). (`crates/goose/src/recipe/validate_recipe.rs:L39-L75`)
- Sub-recipe resolution: absolute `sub_recipes[].path` stays absolute; relative paths are joined to the parent recipe directory; missing paths error. (`crates/goose/src/recipe/build_recipe/mod.rs:L158-L174`)

## Skill discovery (SKILL.md)
### Default directories
Skills discovery scans these directories (when present):
- Home dirs: `~/.claude/skills`, `~/.config/agents/skills`
- Goose config dir: `Paths::config_dir()/skills`
- Working-dir dirs: `$PWD/.claude/skills`, `$PWD/.goose/skills`, `$PWD/.agents/skills`
(`crates/goose/src/agents/skills_extension.rs:L101-L118`)

### Skill format + supporting files
- A skill is a directory containing `SKILL.md`. (`crates/goose/src/agents/skills_extension.rs:L179-L199`)
- `SKILL.md` is parsed as YAML frontmatter delimited by `---`, producing `SkillMetadata { name, description }` and a body string. (`crates/goose/src/agents/skills_extension.rs:L140-L152`)
- Supporting files are collected from the skill directory (excluding `SKILL.md`), including files in immediate subdirectories. (`crates/goose/src/agents/skills_extension.rs:L155-L176`)

## Scheduler job model
### Persistence
- Default scheduler storage path: `Paths::data_dir()/schedule.json`. (`crates/goose/src/scheduler.rs:L31-L35`)
- Persistence writes the full list of `ScheduledJob` as pretty JSON to `storage_path`. (`crates/goose/src/scheduler.rs:L119-L131`)
- Diagnostics bundle includes `schedule.json` and any files in `scheduled_recipes/` under `Paths::data_dir()`. (`crates/goose/src/session/diagnostics.rs:L114-L131`)

### Job record (what is stored)
- `ScheduledJob` includes `id`, `source` (string path), `cron`, and runtime fields like `currently_running`, `paused`, `current_session_id`, `process_start_time`. (`crates/goose/src/scheduler.rs:L103-L117`)

### Recipe copying behavior
- When adding a scheduled job with `make_copy=true`, Goose copies the original recipe file at `stored_job.source` into `Paths::data_dir()/scheduled_recipes/<id>.<ext>` and rewrites `stored_job.source` to the copied file path. (`crates/goose/src/scheduler.rs:L278-L313`)
- The CLI schedule add flow passes the user-provided recipe path as `job.source` and calls `scheduler.add_scheduled_job(job, true)` (copying happens in the scheduler). (`crates/goose-cli/src/commands/schedule.rs:L68-L101`)

## Scheduled execution (end-to-end)
- Execution reads the recipe at `job.source`, parses it as YAML or JSON based on file extension, then creates an `Agent`. (`crates/goose/src/scheduler.rs:L721-L738`)
- Provider + model are read from `Config::global()` and applied via `agent.update_provider(...)`. (`crates/goose/src/scheduler.rs:L739-L745`, `crates/goose/src/scheduler.rs:L761-L766`)
- Extensions for the session are resolved from `recipe.extensions` and added to the agent. (`crates/goose/src/scheduler.rs:L746-L749`)
- A new scheduled `Session` is created via `session_manager.create_session(..., SessionType::Scheduled)`. (`crates/goose/src/scheduler.rs:L751-L759`)
- The recipe prompt/instructions are used as the initial user message, and `agent.reply(...)` is invoked with `SessionConfig { id, schedule_id, ... }`. (`crates/goose/src/scheduler.rs:L781-L799`)
- After completion, the session is updated to record `schedule_id` and the parsed `Recipe`. (`crates/goose/src/scheduler.rs:L822-L829`)

## Config/data directories (Paths)
- `Paths` derives config/data/state dirs via `etcetera::choose_app_strategy` for app `goose` (author/top-level domain `Block`) unless overridden. (`crates/goose/src/config/paths.rs:L16-L27`)
- If `GOOSE_PATH_ROOT` is set, it is used as a base and maps to `config/`, `data/`, `state/`. (`crates/goose/src/config/paths.rs:L8-L14`)
