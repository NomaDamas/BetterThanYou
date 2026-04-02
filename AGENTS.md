# Local ECC Setup

This repository uses a project-local installation of Everything Claude Code
(ECC) for Codex.

Scope:
- Apply these instructions only inside this repository.
- Load project skills from `.agents/skills/`.
- Use `.codex/AGENTS.md` and `.codex/config.toml` as the local Codex baseline.
- Do not sync this setup into `~/.codex` and do not treat it as a global default.

Working principles:
- Plan before large changes.
- Prefer TDD for non-trivial implementation work.
- Review correctness, security, and missing tests before declaring completion.
- Validate inputs at system boundaries and never hardcode secrets.
- Prefer small, reversible changes.

When a task matches an installed skill, use the skill from `.agents/skills/`.
