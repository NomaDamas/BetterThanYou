---
name: better-than-you-battle
description: Use this skill when working on BetterThanYou battle flows, shared scoring, the CLI HUD, Homebrew packaging, or the optional web helper. Keep the CLI as the primary product surface and keep scoring logic in packages/core.
---

# BetterThanYou Battle

Use this skill for BetterThanYou work.

## Product Priority

- `packages/cli` is the primary product surface.
- `packages/core` owns scoring, ingestion, result schema, and static HTML report generation.
- `packages/mcp-server` is an automation adapter.
- `apps/web` is an optional helper for non-developers and report viewing.
- `Formula/better-than-you.rb` exists for Homebrew-based installation work.

## Rules

- Keep BetterThanYou CLI-first.
- Prefer drag-and-drop file paths, pasted URLs, and clipboard flows in the terminal.
- Keep winner-first output in terminal, MCP, and HTML reports.
- Do not duplicate scoring logic outside `packages/core`.
- Treat the current engine as deterministic heuristic scoring unless explicitly upgrading to a VLM-backed or API-backed judge.
- Keep the web helper secondary to the CLI experience.

## Quick Commands

```bash
pnpm test
pnpm build
pnpm cli
pnpm battle -- <left> <right>
pnpm report -- ./reports/latest-battle.json
pnpm web
pnpm mcp
brew install --build-from-source ./Formula/better-than-you.rb
```
