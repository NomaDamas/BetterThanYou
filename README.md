# BetterThanYou

CLI-first AI portrait battle tool with multi-provider VLM judging, terminal UI, and Cloudflare-backed public sharing.

## Quick Start

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou && better-than-you
```

Requires Rust toolchain (`brew install rust`). First install takes ~2 minutes; afterwards just run `better-than-you`.

## Install

### Cargo

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou
```

### Homebrew

```bash
brew install NomaDamas/better-than-you/better-than-you
```

Tap repo: [`NomaDamas/homebrew-better-than-you`](https://github.com/NomaDamas/homebrew-better-than-you).

### From Source

```bash
git clone https://github.com/NomaDamas/BetterThanYou
cd BetterThanYou
make install        # = cargo install --path .  (no project-dir pollution)
```

Both `cargo install` and `brew install` build in temp directories and leave no
caches inside your local clone. Use `cargo build` only when you intend to
iterate on the code (see the **Disk Hygiene** section below).

## Usage

```bash
better-than-you
better-than-you battle left.png right.png --judge auto
better-than-you open
better-than-you publish --copy
better-than-you serve --port 8080
```

## Subcommands

| Command | What it does |
| --- | --- |
| `battle` | Run a single portrait battle and write reports. |
| `report` | Re-render an HTML report from saved battle JSON. |
| `open` | Open the latest or specified report in your browser. |
| `publish` | Upload the latest or specified report and print a public URL. |
| `serve` | Serve the reports directory over HTTP on your LAN. |

## Judge Modes

| Mode | Behavior |
| --- | --- |
| `auto` | Uses the first configured VLM provider, then falls back to `heuristic`. |
| `heuristic` | Local deterministic image scoring. No network or API key. |
| `openai` | OpenAI vision judging. |
| `anthropic` | Anthropic Claude vision judging. |
| `gemini` | Google Gemini vision judging. |

Set provider keys with `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `GEMINI_API_KEY`. The default model is `gpt-5.4-mini`; supported model lists live in `src/lib.rs` as `OPENAI_VLM_MODELS`, `ANTHROPIC_VLM_MODELS`, and `GEMINI_VLM_MODELS`.

## How `heuristic` works (no API key required)

`--judge heuristic` runs a fully **local, deterministic** image-statistics
pipeline. The same pair of images always produces the same scores — no
network, no model, no API key. It samples each portrait into a 48×60 grid
of pixel samples (R/G/B + luminance + saturation + center weight) and
derives the 10 axis scores from regional metrics:

| Axis | Primary signals (heuristic only) |
| --- | --- |
| **Facial Symmetry** | Left ↔ right luminance mirror difference across the whole frame. Lower diff → higher score. |
| **Facial Proportions** | Upper-half vs lower-half mirror balance + how centered the brightest mass is. |
| **Skin Quality** | Cheek/forehead texture variance (smoother → higher) + saturation uniformity. |
| **Eye Expression** | Eye-region (upper 28-48% of frame) contrast + edge density. |
| **Hair & Grooming** | Hair-region (top 30%) edge density + saturation consistency. |
| **Bone Structure** | Jawline-region (lower 60-90%) edge density + local contrast. |
| **Expression & Charisma** | Center weight + face warmth (R−B color tilt) + face saturation + dynamic range. |
| **Lighting & Color** | Whole-frame dynamic range + luminance/saturation deviation + color spread. |
| **Background & Framing** | Center mass strength + background calmness (low outer variance) + edge strength. |
| **Photogenic Impact** | Composite of center presence + palette mood + dynamic range + symmetry. |

A small per-axis hash signal (deterministic from the image content) adds
~0–4 points of variation so two images with similar regional statistics
don't tie. The result is a stable, fast (sub-second) baseline that runs
even without internet access. For nuanced judgement (per-axis prose
explanations, identity-specific commentary), use `--judge openai`,
`--judge anthropic`, or `--judge gemini` with the corresponding API key.

The full source lives in [`src/lib.rs`](src/lib.rs) under `score_portrait`,
`compute_mirror_difference`, `region_*` helpers.

## Scoring

| Axis key | Axis | Short | Weight |
| --- | --- | --- | --- |
| `facial_symmetry` | Facial Symmetry | SYM | 1.0 |
| `facial_proportions` | Facial Proportions | RATIO | 1.0 |
| `skin_quality` | Skin Quality | SKIN | 1.0 |
| `eye_expression` | Eye Expression | EYES | 1.1 |
| `hair_grooming` | Hair & Grooming | HAIR | 0.8 |
| `bone_structure` | Bone Structure | BONE | 0.9 |
| `expression_charisma` | Expression & Charisma | AURA | 1.2 |
| `lighting_color` | Lighting & Color | LIGHT | 1.0 |
| `background_framing` | Background & Framing | FRAME | 0.8 |
| `photogenic_impact` | Photogenic Impact | IMPACT | 1.3 |

Override weights per run with `--axis-weight KEY=WEIGHT`.

## Languages

English, 한국어, and 日本語 are supported. Switch language in Settings.

## Public Sharing On nomadamas.org

`better-than-you publish` uploads reports and share assets to public free-host providers by default. When `BTYU_PUBLISH_URL` and `BTYU_PUBLISH_TOKEN` are set, or configured through Settings, uploads first go to your own Cloudflare Worker backed by KV (or R2) and return a URL such as `https://nomadamas.org/btyu/s/<id>.html`.

```text
CLI ─POST /upload (Bearer)─▶ Cloudflare Worker ─▶ R2 (btyu-shares)
                                     │
Browser/SNS ◀── GET /btyu/s/<id>.html ┘
```

Want your own deploy? See `infra/cloudflare/README.md`.

- Cloudflare free tier covers personal use comfortably.
- Workers includes 100k requests/day; R2 includes 10 GB storage plus free egress.

## TUI Keys

| Key | Action |
| --- | --- |
| `o` | Open report. |
| `q` | Quit. |

## Outputs

- `battle-<ts>.html`
- `battle-<ts>.json`
- `latest-battle.html`
- `latest-battle.json`
- Share PNG

## Development

```bash
make check          # cargo check
make build          # cargo build --release
make run            # cargo run --release
make clean-cache    # reclaim disk (target/, node_modules/, old reports)
make size           # show project disk usage
```

## Disk Hygiene

Rust projects accumulate build artifacts in `target/` (often 1+ GB). This
repo is wired to keep that **out of the project directory entirely** so it
stays small regardless of how often you build:

- **End users** install via `brew install` or `cargo install --git` — both
  build in temp dirs and leave nothing behind in your filesystem.
- **Developers using `make`**: every `make build` / `make run` /
  `make install` automatically sets
  `CARGO_TARGET_DIR=~/.cache/cargo-target/better-than-you`, so artifacts
  land in your home cache, not in the project. The project directory stays
  ~10 MB forever.
- **Developers using `cargo` directly**: run this one-time hook so plain
  `cargo build`/`cargo run` also redirects:
  ```bash
  make install-shell-hook   # appends CARGO_TARGET_DIR export to ~/.zshrc
  source ~/.zshrc            # apply now
  ```
- **Release binary** is shrunk via `Cargo.toml`'s `[profile.release]`
  (`lto = "thin"`, `strip = "symbols"`, `codegen-units = 1`,
  `incremental = false`). ~16 MB → ~10 MB on macOS arm64.
- **Reclaim disk anytime**:
  ```bash
  make clean-cache    # full reclaim: target/, node_modules/, old reports
  make clean          # just the build cache
  make size           # see what's eating space
  ```

## License

MIT.
