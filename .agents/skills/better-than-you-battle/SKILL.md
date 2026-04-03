---
name: better-than-you-battle
description: Use this skill when working on BetterThanYou battle flows, shared scoring, the CLI HUD, Homebrew packaging, or the optional web helper. Keep the CLI as the primary product surface and keep scoring logic in packages/core.
---

# BetterThanYou Battle

Use this skill for BetterThanYou work.

## Product Priority

- `packages/cli` is the primary product surface.
- `packages/core` owns scoring, ingestion, result schema, static HTML report generation, and judge mode selection.
- `packages/mcp-server` is an automation adapter.
- `apps/web` is an optional helper for non-developers and report viewing.
- `Formula/better-than-you.rb` exists for Homebrew-based installation work.

## Rules

- Keep BetterThanYou CLI-first.
- Prefer drag-and-drop file paths, pasted URLs, and clipboard flows in the terminal.
- Keep winner-first output in terminal, MCP, and HTML reports.
- Do not duplicate scoring logic outside `packages/core`.
- Support both deterministic heuristic judging and OpenAI VLM judging.
- Keep the web helper secondary to the CLI experience.

## Judge Modes

- `heuristic`: local deterministic image scoring
- `auto`: OpenAI when API key exists, otherwise heuristic fallback
- `openai`: force OpenAI image judging through the Responses API

## Quick Commands

```bash
pnpm test
pnpm build
pnpm cli
pnpm battle -- <left> <right>
pnpm battle -- <left> <right> --judge openai --model gpt-4.1-mini
pnpm report -- ./reports/latest-battle.json
pnpm web
pnpm mcp
brew reinstall NomaDamas/better-than-you/better-than-you
```
