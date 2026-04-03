# BetterThanYou

BetterThanYou is now moving to a Rust-first CLI/TUI runtime for the real product.
The JavaScript code is still in the repository as a legacy prototype path, but
Rust is the intended long-term binary and Homebrew target.

## Core Idea

- Primary surface: Rust CLI/TUI in tmux, Terminal, or iTerm
- Fastest input: drag two files into the terminal or paste two URLs
- Primary output: winner-first battle screen with stat-by-stat comparison
- Durable artifact: shareable static HTML report in `reports/`
- Optional surfaces: web helper for non-devs, MCP for automation/agents
- Judge modes: `auto`, `heuristic`, `openai`

## Rust Quick Start

```bash
cd /Users/jinminseong/Desktop/BetterThanYou
cargo run -- /absolute/path/to/left.png /absolute/path/to/right.png --judge heuristic --no-app
```

Run tests:

```bash
cargo test
```

## OpenAI VLM Judge

```bash
export OPENAI_API_KEY=your_key
cargo run -- /absolute/path/to/left.png /absolute/path/to/right.png --judge openai --model gpt-4.1-mini --no-app
```

`--judge auto` will use OpenAI when an API key exists, otherwise it falls back
to the local heuristic engine.

## App-Like Terminal Mode

```bash
cargo run -- /absolute/path/to/left.png /absolute/path/to/right.png --judge auto
```

In terminal app mode:
- `o` opens the generated HTML report
- `q` exits the fullscreen result screen

Use `--no-app` to force plain terminal output.

## Homebrew Direction

The formula is being shifted toward the Rust binary path. For a proper public
`brew install better-than-you` experience, the next step is to publish a tap or
release tarballs for the Rust crate version.

## Legacy JS Path

The previous Node/JS prototype still exists and can still be inspected, but the
Rust crate in `Cargo.toml` and `src/` is now the primary migration target.
