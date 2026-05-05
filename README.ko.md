# 🦖 BetterThanYou

[![Release](https://img.shields.io/github/v/release/NomaDamas/BetterThanYou?style=flat-square&color=brightgreen)](https://github.com/NomaDamas/BetterThanYou/releases) [![Stars](https://img.shields.io/github/stars/NomaDamas/BetterThanYou?style=flat-square)](https://github.com/NomaDamas/BetterThanYou/stargazers) [![License](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](#-라이선스) [![Rust](https://img.shields.io/badge/rust-edition%202021-orange.svg?style=flat-square)](https://www.rust-lang.org/)

🌐 **다른 언어로 읽기:** [English](README.md) · [한국어](README.ko.md) · [中文](README.zh.md)

> ⚔️ CLI 우선의 **얼굴 배틀** 도구. 두 얼굴을 — 본인, 친구, 심지어 사람 vs 티라노사우루스 🦖 — 맞붙이고, 로컬 휴리스틱 또는 AI 비전 모델(OpenAI · Anthropic · Gemini)이 승자를 결정합니다.
>
> 🖥️ 터미널 UI · 🌐 다중 프로바이더 VLM 심사 · ☁️ Cloudflare 기반 공개 공유

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou && better-than-you
```

Rust 툴체인이 필요합니다(`brew install rust`). 첫 설치는 약 2분, 이후엔 그냥 `better-than-you` 만 실행하세요.

---

## 📑 목차

- [✨ BetterThanYou 란?](#-betterthanyou-란)
- [⚡ 빠른 시작](#-빠른-시작)
- [📦 설치](#-설치)
- [🎮 사용법](#-사용법)
- [🧰 서브커맨드](#-서브커맨드)
- [⚖️ 심사 모드](#️-심사-모드)
- [🧪 휴리스틱 심판 동작 원리](#-휴리스틱-심판-동작-원리)
- [🎯 채점 축](#-채점-축)
- [🌍 지원 언어](#-지원-언어)
- [🔗 공개 공유](#-공개-공유)
- [⌨️ TUI 키](#️-tui-키)
- [📁 출력 파일](#-출력-파일)
- [🛠️ 개발](#️-개발)
- [🧹 디스크 위생](#-디스크-위생)
- [📜 라이선스](#-라이선스)

---

## ✨ BetterThanYou 란?

BetterThanYou 는 CLI + TUI **얼굴 배틀 아레나**입니다. 두 이미지를 던져 넣으면 각 얼굴을 **10개 미적 축** — 대칭, 골격, 눈 표현력, 포토제닉 임팩트 등 — 으로 채점하고, 전체 HTML 리포트와 함께 승자를 발표합니다.

### ✅ 진지하게 쓰기

- 🤳 SNS 올리기 전 셀카 두 장 중 베스트 고르기.
- 🎨 AI 생성 초상화 A/B 테스트.
- 👯 "이 사진에서 누가 더 잘 나왔어?" 논쟁 종결.

### 🤡 그냥 재미로

- 🦖 **사람 vs 티라노사우루스 렉스.** 누가 턱선이 더 강할까? (스포일러: 공룡이 `BONE` 압살.)
- 🐕 우리 집 개 vs 고양이. 미루던 가족내전 시작.
- 🧙 생성형 마법사 vs 여권 사진.
- 🐸 개구리 vs 인플루언서. 우리는 판단 안 합니다 — 그건 도구 몫.

심판은 **결정론적 로컬 휴리스틱**(인터넷 / API 키 불필요, 1초 이내) 또는 **비전-언어 모델**(OpenAI · Anthropic · Gemini)을 선택할 수 있고, 결과는 독립 실행 HTML / JSON 으로 떨어집니다. 브라우저, LAN 으로 폰, 또는 Cloudflare 기반 공개 링크로 공유 가능합니다.

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## ⚡ 빠른 시작

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou
better-than-you                          # 🎛️ 인터랙티브 TUI 실행
better-than-you you.png trex.png         # ⚔️ 헤드리스 단발 배틀
```

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 📦 설치

### 🦀 Cargo

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou
```

### 🍺 Homebrew

```bash
brew install NomaDamas/better-than-you/better-than-you
```

탭 저장소: [`NomaDamas/homebrew-better-than-you`](https://github.com/NomaDamas/homebrew-better-than-you).

### 🧰 소스에서

```bash
git clone https://github.com/NomaDamas/BetterThanYou
cd BetterThanYou
make install        # = cargo install --path .  (프로젝트 디렉토리 오염 없음)
```

`cargo install` 과 `brew install` 둘 다 임시 디렉토리에서 빌드하므로 로컬 클론 안에 캐시를 남기지 않습니다. 코드 수정이 필요하면 `cargo build` 를 사용하세요([🧹 디스크 위생](#-디스크-위생) 참고).

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🎮 사용법

```bash
better-than-you                                    # 🎛️ 인터랙티브 TUI
better-than-you human.png trex.png --judge auto    # ⚔️ 단발 배틀
better-than-you battle left.png right.png --judge anthropic
better-than-you open                               # 🖼️ 최근 리포트를 브라우저로 열기
better-than-you publish --copy                     # 🔗 공개 + URL 클립보드 복사
better-than-you serve --port 8080                  # 📱 LAN 으로 폰에 리포트 서빙
```

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🧰 서브커맨드

| 명령 | 동작 |
| --- | --- |
| `battle` | ⚔️ 단발 얼굴 배틀 실행 후 리포트 작성. |
| `report` | 🔄 저장된 배틀 JSON 으로 HTML 리포트 재렌더링. |
| `open` | 🖼️ 최신/지정 리포트를 브라우저로 열기. |
| `publish` | 🔗 최신/지정 리포트 업로드 후 공개 URL 출력. |
| `serve` | 📱 reports 디렉토리를 LAN HTTP 로 서빙. |

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## ⚖️ 심사 모드

| 모드 | 동작 |
| --- | --- |
| 🤖 `auto` | 설정된 첫 VLM 프로바이더 사용, 실패 시 `heuristic` 폴백. |
| 🧮 `heuristic` | 결정론적 로컬 이미지 채점. 네트워크/API 키 불필요. |
| 🟢 `openai` | OpenAI 비전 심사. |
| 🟣 `anthropic` | Anthropic Claude 비전 심사. |
| 🔵 `gemini` | Google Gemini 비전 심사. |

`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY` 로 프로바이더 키를 설정하세요. 기본 모델은 `gpt-5.4-mini` 이며, 지원 모델 목록은 [`src/lib.rs`](src/lib.rs) 의 `OPENAI_VLM_MODELS`, `ANTHROPIC_VLM_MODELS`, `GEMINI_VLM_MODELS` 에 있습니다.

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🧪 휴리스틱 심판 동작 원리

`--judge heuristic` 은 완전히 **로컬, 결정론적인** 이미지 통계 파이프라인입니다. 같은 이미지 쌍은 항상 같은 점수가 나옵니다 — 네트워크/모델/API 키 모두 없습니다. 각 얼굴을 **48×60 격자** 픽셀 샘플(R / G / B + 휘도 + 채도 + 중앙 가중치)로 추출한 뒤, 영역별 지표로부터 10개 축 점수를 계산합니다:

| 축 | 주요 신호 (휴리스틱 전용) |
| --- | --- |
| ⚜️ **얼굴 대칭** | 좌우 반전 휘도 차이. 차이가 작을수록 점수 높음. |
| ◆ **얼굴 비율** | 상반/하반 거울 균형 + 가장 밝은 영역의 중심 정렬도. |
| ✨ **피부 상태** | 볼/이마 텍스처 분산(매끄러울수록 ↑) + 채도 균일성. |
| 👁️ **눈 표현력** | 눈 영역(상단 28–48%) 명암 + 엣지 밀도. |
| ✂️ **헤어 & 그루밍** | 헤어 영역(상단 30%) 엣지 밀도 + 채도 일관성. |
| 🦴 **골격 구조** | 턱선 영역(하단 60–90%) 엣지 밀도 + 로컬 명암. |
| 🔥 **표정 & 카리스마** | 중심 가중치 + 얼굴 따뜻함(R−B) + 채도 + 다이내믹 레인지. |
| 💡 **조명 & 색감** | 전체 다이내믹 레인지 + 휘도/채도 편차 + 색 분산. |
| 🖼️ **배경 & 구도** | 중심 질량 + 배경 차분함(외곽 분산↓) + 엣지 강도. |
| 💥 **포토제닉 임팩트** | 중심 존재감 + 팔레트 분위기 + 다이내믹 레인지 + 대칭의 합성치. |

축별 작은 해시 신호(이미지 내용으로부터 결정론적)가 ~0–4 점의 변동을 더해, 영역 통계가 비슷한 두 이미지가 동점이 되지 않게 합니다. 결과는 안정적이고 빠른(서브초) 베이스라인으로, 인터넷 없이도 동작합니다. 축별 산문 설명이나 인물 특화 코멘트가 필요하면 `--judge openai` / `--judge anthropic` / `--judge gemini` 를 해당 API 키와 함께 사용하세요.

전체 소스는 [`src/lib.rs`](src/lib.rs) 의 `score_portrait`, `compute_mirror_difference`, `region_*` 헬퍼에 있습니다.

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🎯 채점 축

| 축 키 | 축 | 짧은 라벨 | 가중치 |
| --- | --- | --- | --- |
| `facial_symmetry` | ⚜️ 얼굴 대칭 | SYM | 1.0 |
| `facial_proportions` | ◆ 얼굴 비율 | RATIO | 1.0 |
| `skin_quality` | ✨ 피부 상태 | SKIN | 1.0 |
| `eye_expression` | 👁️ 눈 표현력 | EYES | 1.1 |
| `hair_grooming` | ✂️ 헤어 & 그루밍 | HAIR | 0.8 |
| `bone_structure` | 🦴 골격 구조 | BONE | 0.9 |
| `expression_charisma` | 🔥 표정 & 카리스마 | AURA | 1.2 |
| `lighting_color` | 💡 조명 & 색감 | LIGHT | 1.0 |
| `background_framing` | 🖼️ 배경 & 구도 | FRAME | 0.8 |
| `photogenic_impact` | 💥 포토제닉 임팩트 | IMPACT | 1.3 |

🎚️ 실행마다 `--axis-weight KEY=WEIGHT` 로 가중치를 덮어쓸 수 있습니다. 티라노한테 공정한 매치를 만들어주려면 강점 축을 키우세요:

```bash
better-than-you human.png trex.png \
  --axis-weight bone_structure=2.0 \
  --axis-weight photogenic_impact=1.5 \
  --judge heuristic
```

**Settings → Aesthetic tuning** 에서 인터랙티브로 조정도 가능합니다.

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🌍 지원 언어

🇺🇸 English, 🇰🇷 한국어, 🇯🇵 日本語를 지원합니다. **Settings** 에서 언어를 변경하세요.

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🔗 공개 공유

`better-than-you publish` 는 기본적으로 공개 무료 호스트로 리포트와 공유 자산을 업로드합니다. `BTYU_PUBLISH_URL` 과 `BTYU_PUBLISH_TOKEN` 이 설정되어 있으면(또는 Settings 에서 구성하면), 본인 Cloudflare Worker 의 전용 서브도메인으로 먼저 업로드되어 `https://better-than-you.nomadamas.org/s/<id>.html` 형태의 URL 을 반환합니다.

```text
CLI ─POST /share (Bearer)─▶ better-than-you.nomadamas.org (Worker) ─▶ KV
                                     │
브라우저/SNS ◀── GET /s/<id>.html ────┘
```

직접 배포하고 싶다면 [`infra/cloudflare/README.md`](infra/cloudflare/README.md) 를 참고하세요.

- ☁️ Cloudflare 무료 티어로 개인 사용은 충분히 커버됩니다.
- 🚀 Workers 는 일 100k 요청, R2 는 10 GB 스토리지 + 무료 송신을 포함합니다.

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## ⌨️ TUI 키

| 키 | 동작 |
| --- | --- |
| `o` | 🖼️ 리포트 열기. |
| `q` | 🚪 종료. |

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 📁 출력 파일

배틀마다 reports 디렉토리에 다음이 떨어집니다:

- 📄 `battle-<ts>.html` — 독립 실행 전체 리포트
- 🧾 `battle-<ts>.json` — 재렌더링용 원본 점수 & 서사
- 🆕 `latest-battle.html` / `latest-battle.json` — 최신 배틀 포인터
- 🖼️ Share PNG — SNS 즉시 사용 가능

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🛠️ 개발

```bash
make check          # 🧐 cargo check
make build          # 🔨 cargo build --release
make run            # ▶️ cargo run --release
make clean-cache    # 🧹 디스크 회수 (target/, node_modules/, 오래된 리포트)
make size           # 📏 프로젝트 디스크 사용량 표시
```

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 🧹 디스크 위생

Rust 프로젝트는 `target/` 에 빌드 산출물(보통 1+ GB)을 쌓습니다. 본 저장소는 그 결과물을 **프로젝트 디렉토리 바깥**에 두도록 와이어링되어 있어, 빌드를 아무리 자주 해도 디렉토리가 작게 유지됩니다:

- 👤 **일반 사용자**는 `brew install` 또는 `cargo install --git` 으로 설치 — 둘 다 임시 디렉토리에서 빌드되어 파일 시스템에 흔적을 남기지 않습니다.
- 🧑‍💻 **`make` 사용 개발자**: `make build` / `make run` / `make install` 마다 `CARGO_TARGET_DIR=~/.cache/cargo-target/better-than-you` 가 자동 설정되어, 산출물이 홈 캐시로 가고 프로젝트 디렉토리는 영원히 ~10 MB 로 유지됩니다.
- 🪝 **`cargo` 직접 사용 개발자**: 일회성 훅을 등록하면 그냥 `cargo build` / `cargo run` 도 동일하게 리다이렉트됩니다:
  ```bash
  make install-shell-hook   # ~/.zshrc 에 CARGO_TARGET_DIR export 추가
  source ~/.zshrc           # 즉시 적용
  ```
- 🗜️ **릴리스 바이너리**는 `Cargo.toml` 의 `[profile.release]` (`lto = "thin"`, `strip = "symbols"`, `codegen-units = 1`, `incremental = false`) 로 압축되어 macOS arm64 에서 ~16 MB → ~10 MB 로 줄어듭니다.
- 🚮 **언제든 디스크 회수**:
  ```bash
  make clean-cache    # 전체 회수: target/, node_modules/, 오래된 리포트
  make clean          # 빌드 캐시만
  make size           # 어디가 차지하는지 확인
  ```

<div align="right"><a href="#-목차">⬆ 맨 위로</a></div>

---

## 📜 라이선스

MIT.
