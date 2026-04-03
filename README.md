# BetterThanYou

BetterThanYou is now Rust-first. The Rust CLI/TUI is the real product runtime.
The JavaScript code remains in the repository as a legacy prototype path for
comparison and migration continuity.

## Product State

- Primary runtime: Rust binary in `Cargo.toml` and `src/`
- Primary UX: terminal-first battle app
- Optional judge modes: `heuristic`, `auto`, `openai`
- Optional helper surfaces: legacy JS web helper and MCP bridge
- Durable outputs: HTML and JSON battle reports in `reports/`

## Fastest Run Path

```bash
cd /Users/jinminseong/Desktop/BetterThanYou
cargo run -- /absolute/path/to/left.png /absolute/path/to/right.png --judge heuristic --no-app
```

## Fullscreen Terminal App

```bash
cargo run -- /absolute/path/to/left.png /absolute/path/to/right.png --judge auto
```

Keys in app mode:
- `o` open the generated HTML report
- `q` quit the fullscreen result screen

Use `--no-app` to force plain terminal output.

## OpenAI VLM Judge

```bash
export OPENAI_API_KEY=your_key
cargo run -- /absolute/path/to/left.png /absolute/path/to/right.png --judge openai --model gpt-4.1-mini --no-app
```

`--judge auto` uses OpenAI when an API key is set. Otherwise it falls back to
local heuristic scoring.

## Rust Test and Build

```bash
cargo check
cargo test
```

## Legacy JS Path

The JavaScript path still exists for continuity:

```bash
pnpm install
pnpm build
pnpm test
```

But Rust is the product direction.

## Homebrew Direction

The formula now targets the Rust binary release path. For a stable public brew
install, publish a tag and a tap, then point the formula at the release tarball.

Current local-tap pattern:

```bash
brew tap-new NomaDamas/better-than-you
cp ./Formula/better-than-you.rb $(brew --repository)/Library/Taps/nomadamas/homebrew-better-than-you/Formula/better-than-you.rb
brew reinstall NomaDamas/better-than-you/better-than-you
better-than-you --help
```

## Judge Logic

### Heuristic

Fast local image scoring using six axes:
- symmetry_harmony
- lighting_contrast
- sharpness_detail
- color_vitality
- composition_presence
- style_aura

### OpenAI

Image-to-JSON VLM judging through the OpenAI Responses API. The model returns a
winner, per-axis scores, and qualitative sections.

## Outputs

Each battle writes:
- one HTML report
- one JSON result
- `latest-battle.html`
- `latest-battle.json`
