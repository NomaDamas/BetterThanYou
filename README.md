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
- Judge modes: `auto`, `heuristic`, `openai`

## Install

### Best current local install

```bash
cd /Users/jinminseong/Desktop/BetterThanYou
pnpm install
pnpm cli
```

### If `pnpm link --global` fails

That error means `PNPM_HOME` is not set up yet. Run this once:

```bash
pnpm setup
exec $SHELL -l
```

Then retry:

```bash
pnpm link --global
better-than-you
```

### Homebrew formula path

A working formula is included in this repo now. You can install it from a local
Homebrew tap or after publishing a tap repository.

Local tap verification flow:

```bash
brew tap-new NomaDamas/better-than-you
cp ./Formula/better-than-you.rb $(brew --repository)/Library/Taps/nomadamas/homebrew-better-than-you/Formula/better-than-you.rb
brew reinstall NomaDamas/better-than-you/better-than-you
better-than-you --help
```

## Best CLI Flows

Direct drag-and-drop path flow:

```bash
better-than-you /absolute/path/to/left.png /absolute/path/to/right.png
```

OpenAI VLM judge mode:

```bash
export OPENAI_API_KEY=your_key
better-than-you /absolute/path/to/left.png /absolute/path/to/right.png --judge openai --model gpt-4.1-mini
```

Automatic mode will use OpenAI when a key is set, otherwise it falls back to the
local heuristic engine:

```bash
better-than-you /absolute/path/to/left.png /absolute/path/to/right.png --judge auto
```

Guided mode for drag-and-drop after launch:

```bash
better-than-you
```

Then drag the left file into the terminal, press Enter, drag the right file,
and press Enter again.

Clipboard-assisted flow on macOS:

```bash
better-than-you battle --left-clipboard --right-clipboard --judge openai
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
better-than-you battle <left> <right> [--judge auto|heuristic|openai] [--model name]
better-than-you battle <left> <right> [--out-dir path] [--json] [--open] [--no-app]
better-than-you battle --left-clipboard --right-clipboard
better-than-you report <battle-json-path> [--out-dir path] [--open]
better-than-you open [latest|path] [--out-dir path]
```

## Judge Modes

### Heuristic

Local, deterministic, no API needed. Fast but not semantic.

### OpenAI

Uses the OpenAI Responses API with image input and structured JSON output to
score both portraits on the same six axes and return a winner plus qualitative
analysis. Default model is `gpt-4.1-mini`.

Relevant official docs:
- https://platform.openai.com/docs/guides/images-vision?api-mode=responses&format=file
- https://platform.openai.com/docs/guides/structured-outputs?api-mode=responses&lang=python

## Product Surfaces

- `packages/core`: one shared scoring engine and report model
- `packages/cli`: primary product surface
- `packages/mcp-server`: automation adapter for agents and toolchains
- `apps/web`: optional helper UI for non-developers
- `reports/`: generated HTML and JSON artifacts
- `Formula/better-than-you.rb`: Homebrew install structure
- `.github/workflows/ci.yml`: automated build and test checks on GitHub

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
