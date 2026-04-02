---
name: better-than-you-battle
description: Use this skill when working on BetterThanYou battle flows, shared scoring, CLI renderer, MCP tools, or the web report viewer. Keep the shared engine in packages/core and avoid duplicating scoring logic across surfaces.
---

# BetterThanYou Battle

Use this skill for work on BetterThanYou.

## Architecture

- `packages/core` owns image ingestion, scoring, result schema, and HTML report generation.
- `packages/cli` owns the tmux-friendly terminal battle experience.
- `packages/mcp-server` exposes battle and report tools using the same core engine.
- `apps/web` provides upload and report viewing on top of the same result model.

## Rules

- Keep winner-first output across terminal, MCP, and web.
- Treat the product as entertainment for fictional AI-generated adults only.
- Do not duplicate scoring logic outside `packages/core`.
- Prefer deterministic heuristics first; API-backed jury logic can come later.
- Keep generated reports shareable and visually strong.

## Quick Commands

```bash
pnpm test
pnpm battle -- <left> <right>
pnpm web
pnpm mcp
```
