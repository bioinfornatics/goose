# Memory Bank Map

Last updated: 2026-01-31

## Pointers (deeper references)
- Recipe discovery (local): crates/goose/src/recipe/local_recipes.rs
- Recipe file loading: crates/goose/src/recipe/read_recipe_file_content.rs
- Recipe template parsing: crates/goose/src/recipe/template_recipe.rs
- Recipe template validation: crates/goose/src/recipe/validate_recipe.rs
- Recipe build/sub-recipe resolution: crates/goose/src/recipe/build_recipe/mod.rs
- CLI recipe search (local + GitHub): crates/goose-cli/src/recipes/search_recipe.rs
- CLI GitHub recipe support: crates/goose-cli/src/recipes/github_recipe.rs
- Slash command → recipe mapping: crates/goose/src/slash_commands.rs
- Skill discovery + SKILL.md parsing: crates/goose/src/agents/skills_extension.rs
- Scheduler implementation + persistence + execution: crates/goose/src/scheduler.rs
- Scheduler trait: crates/goose/src/scheduler_trait.rs
- Agent manager / session isolation: crates/goose/src/execution/manager.rs
- Session storage: crates/goose/src/session/session_manager.rs
- Server endpoints: crates/goose-server/src/routes/{schedule,recipe}.rs
