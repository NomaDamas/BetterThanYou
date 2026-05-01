# 🦖 BetterThanYou

🌐 **切换语言：** [English](README.md) · [한국어](README.ko.md) · [中文](README.zh.md)

> ⚔️ 一个 CLI 优先的**人脸对决**工具。让任意两张脸 —— 你自己、你的朋友，甚至是人类 vs 霸王龙 🦖 —— 同台竞技，由本地启发式算法或 AI 视觉模型（OpenAI · Anthropic · Gemini）裁决胜负。
>
> 🖥️ 终端 UI · 🌐 多供应商 VLM 评判 · ☁️ Cloudflare 公共分享

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou && better-than-you
```

需要 Rust 工具链（`brew install rust`）。首次安装约 2 分钟；之后只需运行 `better-than-you`。

---

## 📑 目录

- [✨ BetterThanYou 是什么？](#-betterthanyou-是什么)
- [⚡ 快速开始](#-快速开始)
- [📦 安装](#-安装)
- [🎮 使用](#-使用)
- [🧰 子命令](#-子命令)
- [⚖️ 评判模式](#️-评判模式)
- [🧪 启发式裁判工作原理](#-启发式裁判工作原理)
- [🎯 评分维度](#-评分维度)
- [🌍 支持语言](#-支持语言)
- [🔗 公开分享](#-公开分享)
- [⌨️ TUI 快捷键](#️-tui-快捷键)
- [📁 输出文件](#-输出文件)
- [🛠️ 开发](#️-开发)
- [🧹 磁盘卫生](#-磁盘卫生)
- [📜 许可](#-许可)

---

## ✨ BetterThanYou 是什么？

BetterThanYou 是一个 CLI + TUI **人脸对决竞技场**。把两张图片丢给它，它会从 **10 个美学维度** —— 对称、骨骼、眼神、镜头表现力等 —— 为每张脸打分，然后宣布胜者并附上完整 HTML 报告。

正经用法：
- 🤳 发朋友圈前从两张自拍中选最佳。
- 🎨 AI 生成肖像的 A/B 测试。
- 👯 终结"这张照片谁更上镜"的争论。

不正经用法：
- 🦖 **人类 vs 霸王龙。** 谁的下颌线更狠？（剧透：恐龙 `BONE` 碾压。）
- 🐕 你家狗 vs 你家猫。一直没敢开打的家庭内战。
- 🧙 生成式巫师 vs 你的护照照片。
- 🐸 青蛙 vs 网红。我们不评判 —— 那是工具的活。

裁判可选**确定性本地启发式**（无需联网、无需 API 密钥、亚秒级）或**视觉-语言模型**（OpenAI · Anthropic · Gemini）以获得细致的文字评语。结果以独立 HTML 与 JSON 形式输出，可在浏览器、局域网内手机上查看，或通过 Cloudflare 支撑的链接公开分享。

---

## ⚡ 快速开始

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou
better-than-you                          # 🎛️ 启动交互式 TUI
better-than-you you.png trex.png         # ⚔️ 无界面单次对决
```

---

## 📦 安装

### 🦀 Cargo

```bash
cargo install --git https://github.com/NomaDamas/BetterThanYou
```

### 🍺 Homebrew

```bash
brew install NomaDamas/better-than-you/better-than-you
```

Tap 仓库：[`NomaDamas/homebrew-better-than-you`](https://github.com/NomaDamas/homebrew-better-than-you)。

### 🧰 从源码

```bash
git clone https://github.com/NomaDamas/BetterThanYou
cd BetterThanYou
make install        # = cargo install --path .  （不污染项目目录）
```

`cargo install` 和 `brew install` 都在临时目录中构建，不会在本地克隆里留下缓存。仅当你要修改代码时使用 `cargo build`（详见 [🧹 磁盘卫生](#-磁盘卫生)）。

---

## 🎮 使用

```bash
better-than-you                                    # 🎛️ 交互式 TUI
better-than-you human.png trex.png --judge auto    # ⚔️ 单次对决
better-than-you battle left.png right.png --judge anthropic
better-than-you open                               # 🖼️ 在浏览器中打开最新报告
better-than-you publish --copy                     # 🔗 发布并复制公开 URL
better-than-you serve --port 8080                  # 📱 在局域网内向手机提供报告
```

---

## 🧰 子命令

| 命令 | 作用 |
| --- | --- |
| `battle` | ⚔️ 运行一次人脸对决并写入报告。 |
| `report` | 🔄 用保存的对决 JSON 重新生成 HTML 报告。 |
| `open` | 🖼️ 在浏览器中打开最新或指定的报告。 |
| `publish` | 🔗 上传最新或指定的报告并打印公开 URL。 |
| `serve` | 📱 通过 HTTP 在局域网内提供 reports 目录。 |

---

## ⚖️ 评判模式

| 模式 | 行为 |
| --- | --- |
| 🤖 `auto` | 使用首个已配置的 VLM 供应商，失败则回退至 `heuristic`。 |
| 🧮 `heuristic` | 本地确定性图像评分。无需网络、无需 API 密钥。 |
| 🟢 `openai` | OpenAI 视觉评判。 |
| 🟣 `anthropic` | Anthropic Claude 视觉评判。 |
| 🔵 `gemini` | Google Gemini 视觉评判。 |

通过 `OPENAI_API_KEY`、`ANTHROPIC_API_KEY` 或 `GEMINI_API_KEY` 设置供应商密钥。默认模型为 `gpt-5.4-mini`；支持的模型列表位于 [`src/lib.rs`](src/lib.rs) 中的 `OPENAI_VLM_MODELS`、`ANTHROPIC_VLM_MODELS` 和 `GEMINI_VLM_MODELS`。

---

## 🧪 启发式裁判工作原理

`--judge heuristic` 运行一个完全**本地、确定性**的图像统计流水线。同一对图片永远产出同样的分数 —— 无网络、无模型、无 API 密钥。它将每张脸采样为 **48×60 网格**像素样本（R / G / B + 亮度 + 饱和度 + 中心权重），然后从区域指标推导 10 个维度分数：

| 维度 | 主要信号（仅启发式） |
| --- | --- |
| ⚜️ **面部对称** | 整帧左右镜像亮度差。差越小 → 分越高。 |
| ◆ **面部比例** | 上半 vs 下半镜像平衡 + 最亮区域的居中度。 |
| ✨ **皮肤质感** | 脸颊/前额纹理方差（越平滑越高）+ 饱和度均匀性。 |
| 👁️ **眼神表现** | 眼部区域（28–48% 高度）对比 + 边缘密度。 |
| ✂️ **发型与修饰** | 头发区域（顶部 30%）边缘密度 + 饱和度一致性。 |
| 🦴 **骨骼结构** | 下颌区域（60–90% 高度）边缘密度 + 局部对比。 |
| 🔥 **表情与气场** | 中心权重 + 面部暖色（R−B 色偏）+ 饱和度 + 动态范围。 |
| 💡 **光影与色彩** | 整帧动态范围 + 亮度/饱和度偏差 + 色彩分布。 |
| 🖼️ **背景与构图** | 中心质量 + 背景安静度（外缘方差低）+ 边缘强度。 |
| 💥 **镜头表现力** | 中心存在感 + 调色情绪 + 动态范围 + 对称的综合。 |

每个维度有一个小型哈希信号（由图像内容确定），增加约 0–4 分浮动，避免两张区域统计相似的图像打成平手。结果稳定、快速（亚秒级），即使没有网络也能跑。需要细致评判（每维度文字解释、个体化点评）时，使用 `--judge openai`、`--judge anthropic` 或 `--judge gemini` 配合相应 API 密钥。

完整源码位于 [`src/lib.rs`](src/lib.rs) 的 `score_portrait`、`compute_mirror_difference`、`region_*` 辅助函数。

---

## 🎯 评分维度

| 维度键 | 维度 | 简称 | 权重 |
| --- | --- | --- | --- |
| `facial_symmetry` | ⚜️ 面部对称 | SYM | 1.0 |
| `facial_proportions` | ◆ 面部比例 | RATIO | 1.0 |
| `skin_quality` | ✨ 皮肤质感 | SKIN | 1.0 |
| `eye_expression` | 👁️ 眼神表现 | EYES | 1.1 |
| `hair_grooming` | ✂️ 发型与修饰 | HAIR | 0.8 |
| `bone_structure` | 🦴 骨骼结构 | BONE | 0.9 |
| `expression_charisma` | 🔥 表情与气场 | AURA | 1.2 |
| `lighting_color` | 💡 光影与色彩 | LIGHT | 1.0 |
| `background_framing` | 🖼️ 背景与构图 | FRAME | 0.8 |
| `photogenic_impact` | 💥 镜头表现力 | IMPACT | 1.3 |

🎚️ 用 `--axis-weight KEY=WEIGHT` 在每次运行中覆盖权重。给霸王龙一个公平机会，就把它的强项加权：

```bash
better-than-you human.png trex.png \
  --axis-weight bone_structure=2.0 \
  --axis-weight photogenic_impact=1.5 \
  --judge heuristic
```

也可在 **Settings → Aesthetic tuning** 中交互式调整权重。

---

## 🌍 支持语言

🇺🇸 English、🇰🇷 한국어 与 🇯🇵 日本語 受支持。在 **Settings** 中切换语言。

---

## 🔗 公开分享

`better-than-you publish` 默认将报告与分享资源上传到公共免费托管。当 `BTYU_PUBLISH_URL` 和 `BTYU_PUBLISH_TOKEN` 已设置（或在 Settings 中配置），上传会先发到你自己的 Cloudflare Worker（一个专属子域），返回类似 `https://better-than-you.nomadamas.org/s/<id>.html` 的 URL。

```text
CLI ─POST /share (Bearer)─▶ better-than-you.nomadamas.org (Worker) ─▶ KV
                                     │
浏览器/SNS ◀── GET /s/<id>.html ─────┘
```

想要自部署？参见 [`infra/cloudflare/README.md`](infra/cloudflare/README.md)。

- ☁️ Cloudflare 免费层覆盖个人使用绰绰有余。
- 🚀 Workers 含每日 10 万请求；R2 含 10 GB 存储与免费出站。

---

## ⌨️ TUI 快捷键

| 键 | 动作 |
| --- | --- |
| `o` | 🖼️ 打开报告。 |
| `q` | 🚪 退出。 |

---

## 📁 输出文件

每次对决会向 reports 目录写入：

- 📄 `battle-<ts>.html` —— 完整独立报告
- 🧾 `battle-<ts>.json` —— 用于再渲染的原始分数 & 文案
- 🆕 `latest-battle.html` / `latest-battle.json` —— 指向最新对决的指针
- 🖼️ Share PNG —— 直接可用于社交媒体

---

## 🛠️ 开发

```bash
make check          # 🧐 cargo check
make build          # 🔨 cargo build --release
make run            # ▶️ cargo run --release
make clean-cache    # 🧹 回收磁盘（target/、node_modules/、旧报告）
make size           # 📏 显示项目磁盘占用
```

---

## 🧹 磁盘卫生

Rust 项目会在 `target/` 中累积构建产物（通常 1+ GB）。本仓库已配置为将其完全**置于项目目录之外**，无论你构建多少次都不会让目录膨胀：

- 👤 **终端用户**通过 `brew install` 或 `cargo install --git` 安装 —— 都在临时目录构建，不在你的文件系统中留下任何痕迹。
- 🧑‍💻 **使用 `make` 的开发者**：每次 `make build` / `make run` / `make install` 都会自动设置 `CARGO_TARGET_DIR=~/.cache/cargo-target/better-than-you`，产物落到主目录缓存而非项目里。项目目录永远保持在 ~10 MB。
- 🪝 **直接使用 `cargo` 的开发者**：执行一次性钩子，让裸 `cargo build` / `cargo run` 也重定向：
  ```bash
  make install-shell-hook   # 向 ~/.zshrc 追加 CARGO_TARGET_DIR 导出
  source ~/.zshrc           # 立即生效
  ```
- 🗜️ **发布二进制**通过 `Cargo.toml` 的 `[profile.release]` 收缩（`lto = "thin"`、`strip = "symbols"`、`codegen-units = 1`、`incremental = false`），在 macOS arm64 上从 ~16 MB → ~10 MB。
- 🚮 **随时回收磁盘**：
  ```bash
  make clean-cache    # 完全回收：target/、node_modules/、旧报告
  make clean          # 仅清构建缓存
  make size           # 看是什么吃掉了空间
  ```

---

## 📜 许可

MIT.
