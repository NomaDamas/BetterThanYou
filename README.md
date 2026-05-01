# 🦖 BetterThanYou

> ⚔️ A CLI-first **face battle** tool. Pit any two faces — yourself, your friends, or even a human vs a tyrannosaurus 🦖 — and let local heuristics or AI vision models (OpenAI · Anthropic · Gemini) crown the winner.
>
> 🖥️ Terminal UI · 🌐 multi-provider VLM judging · ☁️ Cloudflare-backed public sharing

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou && better-than-you
```

Requires Rust toolchain (`brew install rust`). First install takes ~2 minutes; afterwards just run `better-than-you`.

---

## 📑 Table of Contents

- [✨ What is BetterThanYou?](#-what-is-betterthanyou)
- [⚡ Quick Start](#-quick-start)
- [📦 Install](#-install)
- [🎮 Usage](#-usage)
- [🧰 Subcommands](#-subcommands)
- [⚖️ Judge Modes](#-judge-modes)
- [🧪 How the Heuristic Judge Works](#-how-the-heuristic-judge-works)
- [🎯 Scoring Axes](#-scoring-axes)
- [🌍 Languages](#-languages)
- [🔗 Public Sharing](#-public-sharing)
- [⌨️ TUI Keys](#-tui-keys)
- [📁 Outputs](#-outputs)
- [🛠️ Development](#-development)
- [🧹 Disk Hygiene](#-disk-hygiene)
- [📜 License](#-license)

---

## ✨ What is BetterThanYou?

BetterThanYou is a CLI + TUI **face-battle arena**. Drop two images into it, and it scores each face across **10 aesthetic axes** — symmetry, bone structure, eye expression, photogenic impact, and friends — before declaring a winner with a full HTML report.

Use it serious:
- 🤳 Pick the better of two selfies before posting.
- 🎨 A/B-test AI-generated portraits.
- 👯 Settle "who's more photogenic in this photo" arguments.

Use it ridiculous:
- 🦖 **Human vs Tyrannosaurus rex.** Whose jawline wins? (Spoiler: the dino crushes `BONE`.)
- 🐕 Your dog vs your cat. The vendetta you've been avoiding.
- 🧙 Generated wizard vs your passport photo.
- 🐸 Frog vs influencer. We don't judge — that's the tool's job.

The judge can be a **deterministic local heuristic** (no internet, no API key, sub-second) or a **vision-language model** (OpenAI · Anthropic · Gemini) for nuanced prose verdicts. Reports come out as standalone HTML and JSON, openable in your browser, your phone over LAN, or shared publicly via a Cloudflare-backed link.

---

## ⚡ Quick Start

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou
better-than-you                          # 🎛️ launch the interactive TUI
better-than-you you.png trex.png         # ⚔️ headless one-shot battle
```

---

## 📦 Install

### 🦀 Cargo

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou
```

### 🍺 Homebrew

```bash
brew install NomaDamas/better-than-you/better-than-you
```

Tap repo: [`NomaDamas/homebrew-better-than-you`](https://github.com/NomaDamas/homebrew-better-than-you).

### 🧰 From source

```bash
git clone https://github.com/NomaDamas/BetterThanYou
cd BetterThanYou
make install        # = cargo install --path .  (no project-dir pollution)
```

Both `cargo install` and `brew install` build in temp directories and leave no caches inside your local clone. Use `cargo build` only when you intend to iterate on the code (see [🧹 Disk Hygiene](#-disk-hygiene)).

---

## 🎮 Usage

```bash
better-than-you                                    # 🎛️ interactive TUI
better-than-you human.png trex.png --judge auto    # ⚔️ one-shot battle
better-than-you battle left.png right.png --judge anthropic
better-than-you open                               # 🖼️ open latest report in your browser
better-than-you publish --copy                     # 🔗 publish + copy public URL
better-than-you serve --port 8080                  # 📱 serve reports to your phone over LAN
```

---

## 🧰 Subcommands

| Command | What it does |
| --- | --- |
| `battle` | ⚔️ Run a single face battle and write reports. |
| `report` | 🔄 Re-render an HTML report from saved battle JSON. |
| `open` | 🖼️ Open the latest or specified report in your browser. |
| `publish` | 🔗 Upload the latest or specified report and print a public URL. |
| `serve` | 📱 Serve the reports directory over HTTP on your LAN. |

---

## ⚖️ Judge Modes

| Mode | Behavior |
| --- | --- |
| 🤖 `auto` | Uses the first configured VLM provider, then falls back to `heuristic`. |
| 🧮 `heuristic` | Local deterministic image scoring. No network, no API key. |
| 🟢 `openai` | OpenAI vision judging. |
| 🟣 `anthropic` | Anthropic Claude vision judging. |
| 🔵 `gemini` | Google Gemini vision judging. |

Set provider keys with `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `GEMINI_API_KEY`. The default model is `gpt-5.4-mini`; supported model lists live in [`src/lib.rs`](src/lib.rs) as `OPENAI_VLM_MODELS`, `ANTHROPIC_VLM_MODELS`, and `GEMINI_VLM_MODELS`.

---

## 🧪 How the Heuristic Judge Works

`--judge heuristic` runs a fully **local, deterministic** image-statistics pipeline. The same pair of images always produces the same scores — no network, no model, no API key. It samples each face into a **48×60 grid** of pixel samples (R / G / B + luminance + saturation + center weight) and derives the 10 axis scores from regional metrics:

| Axis | Primary signals (heuristic only) |
| --- | --- |
| ⚜️ **Facial Symmetry** | Left ↔ right luminance mirror difference across the whole frame. Lower diff → higher score. |
| ◆ **Facial Proportions** | Upper-half vs lower-half mirror balance + how centered the brightest mass is. |
| ✨ **Skin Quality** | Cheek/forehead texture variance (smoother → higher) + saturation uniformity. |
| 👁️ **Eye Expression** | Eye-region (upper 28–48% of frame) contrast + edge density. |
| ✂️ **Hair & Grooming** | Hair-region (top 30%) edge density + saturation consistency. |
| 🦴 **Bone Structure** | Jawline-region (lower 60–90%) edge density + local contrast. |
| 🔥 **Expression & Charisma** | Center weight + face warmth (R−B color tilt) + face saturation + dynamic range. |
| 💡 **Lighting & Color** | Whole-frame dynamic range + luminance/saturation deviation + color spread. |
| 🖼️ **Background & Framing** | Center mass strength + background calmness (low outer variance) + edge strength. |
| 💥 **Photogenic Impact** | Composite of center presence + palette mood + dynamic range + symmetry. |

A small per-axis hash signal (deterministic from the image content) adds ~0–4 points of variation so two images with similar regional statistics don't tie. The result is a stable, fast (sub-second) baseline that runs even without internet access. For nuanced judgement (per-axis prose explanations, identity-specific commentary), use `--judge openai`, `--judge anthropic`, or `--judge gemini` with the corresponding API key.

The full source lives in [`src/lib.rs`](src/lib.rs) under `score_portrait`, `compute_mirror_difference`, `region_*` helpers.

---

## 🎯 Scoring Axes

| Axis key | Axis | Short | Weight |
| --- | --- | --- | --- |
| `facial_symmetry` | ⚜️ Facial Symmetry | SYM | 1.0 |
| `facial_proportions` | ◆ Facial Proportions | RATIO | 1.0 |
| `skin_quality` | ✨ Skin Quality | SKIN | 1.0 |
| `eye_expression` | 👁️ Eye Expression | EYES | 1.1 |
| `hair_grooming` | ✂️ Hair & Grooming | HAIR | 0.8 |
| `bone_structure` | 🦴 Bone Structure | BONE | 0.9 |
| `expression_charisma` | 🔥 Expression & Charisma | AURA | 1.2 |
| `lighting_color` | 💡 Lighting & Color | LIGHT | 1.0 |
| `background_framing` | 🖼️ Background & Framing | FRAME | 0.8 |
| `photogenic_impact` | 💥 Photogenic Impact | IMPACT | 1.3 |

🎚️ Override weights per run with `--axis-weight KEY=WEIGHT`. Stack a T-Rex's strengths to give it a fair chance:

```bash
better-than-you human.png trex.png \
  --axis-weight bone_structure=2.0 \
  --axis-weight photogenic_impact=1.5 \
  --judge heuristic
```

You can also tune weights interactively under **Settings → Aesthetic tuning**.

---

## 🌍 Languages

🇺🇸 English, 🇰🇷 한국어, and 🇯🇵 日本語 are supported. Switch language under **Settings**.

---

## 🔗 Public Sharing

`better-than-you publish` uploads reports and share assets to public free-host providers by default. When `BTYU_PUBLISH_URL` and `BTYU_PUBLISH_TOKEN` are set (or configured through Settings), uploads first go to your own Cloudflare Worker on its dedicated subdomain and return a URL such as `https://better-than-you.nomadamas.org/s/<id>.html`.

```text
CLI ─POST /share (Bearer)─▶ better-than-you.nomadamas.org (Worker) ─▶ KV
                                     │
Browser/SNS ◀── GET /s/<id>.html ────┘
```

Want your own deploy? See [`infra/cloudflare/README.md`](infra/cloudflare/README.md).

- ☁️ Cloudflare free tier covers personal use comfortably.
- 🚀 Workers includes 100k requests/day; R2 includes 10 GB storage plus free egress.

---

## ⌨️ TUI Keys

| Key | Action |
| --- | --- |
| `o` | 🖼️ Open report. |
| `q` | 🚪 Quit. |

---

## 📁 Outputs

Each battle drops the following into your reports directory:

- 📄 `battle-<ts>.html` — full standalone report
- 🧾 `battle-<ts>.json` — raw scores & narrative for re-rendering
- 🆕 `latest-battle.html` / `latest-battle.json` — pointers to the most recent battle
- 🖼️ Share PNG — ready for SNS

---

## 🛠️ Development

```bash
make check          # 🧐 cargo check
make build          # 🔨 cargo build --release
make run            # ▶️ cargo run --release
make clean-cache    # 🧹 reclaim disk (target/, node_modules/, old reports)
make size           # 📏 show project disk usage
```

---

## 🧹 Disk Hygiene

Rust projects accumulate build artifacts in `target/` (often 1+ GB). This repo is wired to keep that **out of the project directory entirely** so it stays small regardless of how often you build:

- 👤 **End users** install via `brew install` or `cargo install --git` — both build in temp dirs and leave nothing behind in your filesystem.
- 🧑‍💻 **Developers using `make`**: every `make build` / `make run` / `make install` automatically sets `CARGO_TARGET_DIR=~/.cache/cargo-target/better-than-you`, so artifacts land in your home cache, not in the project. The project directory stays ~10 MB forever.
- 🪝 **Developers using `cargo` directly**: run this one-time hook so plain `cargo build` / `cargo run` also redirects:
  ```bash
  make install-shell-hook   # appends CARGO_TARGET_DIR export to ~/.zshrc
  source ~/.zshrc           # apply now
  ```
- 🗜️ **Release binary** is shrunk via `Cargo.toml`'s `[profile.release]` (`lto = "thin"`, `strip = "symbols"`, `codegen-units = 1`, `incremental = false`). ~16 MB → ~10 MB on macOS arm64.
- 🚮 **Reclaim disk anytime**:
  ```bash
  make clean-cache    # full reclaim: target/, node_modules/, old reports
  make clean          # just the build cache
  make size           # see what's eating space
  ```

---

## 📜 License

MIT.
