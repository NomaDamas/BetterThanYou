# ECC for Codex CLI

This repository has a project-local ECC setup for Codex.

Scope:
- This setup is local to this repository only.
- Keep `~/.codex` unchanged.
- Skills are loaded from `.agents/skills/`.

Available local capabilities:
- Repo-local skills under `.agents/skills/`
- Repo-local Codex config in `.codex/config.toml`
- Repo-local multi-agent role presets in `.codex/agents/`

Suggested usage:
- Use skills when a task clearly matches one of the installed workflows.
- Use the local `explorer`, `reviewer`, and `docs_researcher` agent roles when
  multi-agent work is enabled and useful.
- Keep heavier personal or global preferences in `~/.codex/config.toml`, not in
  this repository.
