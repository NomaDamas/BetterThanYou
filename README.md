# BetterThanYou

BetterThanYou is a CLI-first portrait battle tool for fictional AI-generated
adult portraits. The terminal flow is the product. Web is an optional helper
for non-developers and static report viewing.

## Core Idea

- Primary surface: CLI in tmux, Terminal, or iTerm
- Fastest input: drag two files into the terminal or paste two URLs
- Primary output: winner-first HUD with stat-by-stat comparison
- Durable artifact: shareable static HTML report in `reports/`
- Optional surfaces: web helper for non-devs, MCP for automation/agents

## Quick Start

```bash
cd /Users/jinminseong/Desktop/BetterThanYou
pnpm install
pnpm link --global
better-than-you
```

If you do not want a global link, you can still run it directly:

```bash
pnpm cli
pnpm battle -- ./left.png ./right.png
```

## Best CLI Flows

Direct drag-and-drop path flow:

```bash
better-than-you battle /absolute/path/to/left.png /absolute/path/to/right.png
```

Implicit battle mode without typing the subcommand:

```bash
better-than-you /absolute/path/to/left.png /absolute/path/to/right.png
```

Guided mode for drag-and-drop after launch:

```bash
better-than-you
```

Then drag the left file into the terminal, press Enter, drag the right file,
and press Enter again.

Clipboard-assisted flow on macOS:

```bash
better-than-you battle --left-clipboard --right-clipboard
```

Open the latest generated HTML report:

```bash
better-than-you open
```

Rebuild an HTML report from a saved battle JSON file:

```bash
better-than-you report ./reports/latest-battle.json --open
```

## CLI Options

```bash
better-than-you battle <left> <right> [--left-label name] [--right-label name]
better-than-you battle <left> <right> [--out-dir path] [--json] [--open]
better-than-you battle --left-clipboard --right-clipboard
better-than-you report <battle-json-path> [--out-dir path] [--open]
better-than-you open [latest|path] [--out-dir path]
```

## Product Surfaces

- `packages/core`: one shared scoring engine and report model
- `packages/cli`: primary product surface
- `packages/mcp-server`: automation adapter for agents and toolchains
- `apps/web`: optional helper UI for non-developers
- `reports/`: generated HTML and JSON artifacts

## Commands

```bash
pnpm build
pnpm test
pnpm cli
pnpm battle -- ./left.png ./right.png
pnpm report -- ./reports/latest-battle.json
pnpm open
pnpm web
pnpm mcp
```

## Brew Direction

The repo is now structured so the root package exposes a real `better-than-you`
bin. That makes Homebrew packaging straightforward once this repository has a
stable remote URL and versioned release tarballs. Until then, `pnpm link --global`
is the quickest install path.
