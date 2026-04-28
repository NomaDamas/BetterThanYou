use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use image::{imageops, DynamicImage, ImageDecoder, Rgba, RgbaImage};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const PRODUCT_NAME: &str = "BetterThanYou";
pub const ENGINE_VERSION: &str = "deterministic-heuristic-v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-5.4-mini";

/// How many recent battles' artifacts to keep when auto-pruning `reports/`.
/// Each battle has up to 3 entries (.html, .json, -share/). Default keeps
/// reports/ around the 10–30 MB range for typical use.
pub const REPORTS_KEEP_RECENT: usize = 5;

/// Filenames that must NEVER be deleted by `prune_old_reports`.
const REPORTS_PROTECTED: &[&str] = &[
    "latest-battle.html",
    "latest-battle.json",
    "latest-published.json",
    ".gitkeep",
    ".gitignore",
    ".DS_Store",
];

/// Extract the battle-id prefix from a filename (e.g.
/// "2026-04-27t18-45-02-338z-img-0674-trax.html" → "2026-04-27t18-45-02-338z").
/// Used to group .html, .json, and -share/ entries that belong to the same battle.
fn extract_battle_prefix(name: &str) -> Option<&str> {
    let bytes = name.as_bytes();
    if bytes.len() < 24 {
        return None;
    }
    if !bytes[..4].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if bytes[4] != b'-' {
        return None;
    }
    bytes.iter().position(|&b| b == b'z').map(|i| &name[..=i])
}

/// Wipe every battle artifact from `out_dir`. Protected files
/// (`latest-battle.*`, `latest-published.json`, `.gitkeep`, `.gitignore`,
/// `.DS_Store`) are kept so dotfiles and the "current" pointers survive.
/// Returns the number of entries deleted.
pub fn clear_all_reports(out_dir: &Path) -> usize {
    if !out_dir.exists() {
        return 0;
    }
    let read_dir = match fs::read_dir(out_dir) {
        Ok(r) => r,
        Err(_) => return 0,
    };
    let mut deleted = 0usize;
    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if REPORTS_PROTECTED.iter().any(|p| *p == name.as_str()) {
            continue;
        }
        if extract_battle_prefix(&name).is_none() {
            continue;
        }
        let removed = if path.is_dir() {
            fs::remove_dir_all(&path).is_ok()
        } else {
            fs::remove_file(&path).is_ok()
        };
        if removed {
            deleted += 1;
        }
    }
    // Also wipe latest-* once the user explicitly clears, since they're
    // pointers to artifacts we just deleted.
    for ptr in &["latest-battle.html", "latest-battle.json", "latest-published.json"] {
        let p = out_dir.join(ptr);
        if p.exists() && fs::remove_file(&p).is_ok() {
            deleted += 1;
        }
    }
    deleted
}

/// Trim `out_dir` to the most recent `keep_recent` battles. Idempotent and
/// silent when nothing needs to be pruned. Errors are swallowed per-entry so a
/// stuck file (e.g. open in Preview) never aborts the cleanup.
///
/// Returns the number of entries deleted.
pub fn prune_old_reports(out_dir: &Path, keep_recent: usize) -> usize {
    if !out_dir.exists() {
        return 0;
    }
    let read_dir = match fs::read_dir(out_dir) {
        Ok(r) => r,
        Err(_) => return 0,
    };

    use std::collections::HashMap;
    use std::time::SystemTime;

    let mut groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut group_mtime: HashMap<String, SystemTime> = HashMap::new();

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if REPORTS_PROTECTED.iter().any(|p| *p == name.as_str()) {
            continue;
        }
        let prefix = match extract_battle_prefix(&name) {
            Some(p) => p.to_string(),
            None => continue,
        };
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        groups.entry(prefix.clone()).or_default().push(path);
        group_mtime
            .entry(prefix)
            .and_modify(|cur| {
                if mtime > *cur {
                    *cur = mtime;
                }
            })
            .or_insert(mtime);
    }

    if groups.len() <= keep_recent {
        return 0;
    }

    let mut sorted: Vec<(String, SystemTime)> = group_mtime.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let mut deleted = 0usize;
    for (prefix, _) in sorted.into_iter().skip(keep_recent) {
        if let Some(paths) = groups.remove(&prefix) {
            for path in paths {
                let removed = if path.is_dir() {
                    fs::remove_dir_all(&path).is_ok()
                } else {
                    fs::remove_file(&path).is_ok()
                };
                if removed {
                    deleted += 1;
                }
            }
        }
    }
    deleted
}

// Curated to vision-capable, non-deprecated OpenAI models as of 2026-04-28.
// Source: https://developers.openai.com/api/docs/models/all
// Removed: gpt-4o, gpt-4o-mini, gpt-5.4-nano, o4-mini (deprecated).
// Removed: gpt-realtime/gpt-audio/gpt-*-codex/omni-moderation (not VLM).
// Added:   gpt-5.5, gpt-5.5-pro, gpt-5-mini/nano/pro, gpt-5.2-pro,
//          gpt-5.1, o3-pro.
pub const OPENAI_VLM_MODELS: &[&str] = &[
    "gpt-5.5",
    "gpt-5.5-pro",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.4-pro",
    "gpt-5.2",
    "gpt-5.2-pro",
    "gpt-5.1",
    "gpt-5",
    "gpt-5-mini",
    "gpt-5-nano",
    "gpt-5-pro",
    "gpt-4.1",
    "gpt-4.1-mini",
    "o3",
    "o3-pro",
];

// Models verified active for vision/multimodal use as of 2026-04-28.
// Source: https://platform.claude.com/docs/en/about-claude/models/overview
// Removed: claude-{sonnet,opus}-4-20250514 (deprecated, retire 2026-06-15).
pub const ANTHROPIC_VLM_MODELS: &[&str] = &[
    "claude-opus-4-7",
    "claude-sonnet-4-6",
    "claude-haiku-4-5-20251001",
    "claude-opus-4-6",
    "claude-sonnet-4-5-20250929",
    "claude-opus-4-5-20251101",
    "claude-opus-4-1-20250805",
];

// Source: https://ai.google.dev/gemini-api/docs/models
// Removed: gemini-2.0-flash (deprecated, shuts down 2026-06-01).
pub const GEMINI_VLM_MODELS: &[&str] = &[
    "gemini-3.1-pro-preview",
    "gemini-3-flash-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JudgeMode {
    Auto,
    Heuristic,
    Openai,
    Anthropic,
    Gemini,
}

impl JudgeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Heuristic => "heuristic",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    English,
    Korean,
    Japanese,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Korean => "한국어",
            Self::Japanese => "日本語",
        }
    }
}

pub fn t(lang: Language, key: &str) -> &'static str {
    match (lang, key) {
        // Menu items
        (Language::Korean, "start_battle") => "배틀 시작",
        (Language::Korean, "open_report") => "최근 리포트 열기",
        (Language::Korean, "share_result") => "SNS 공유",
        (Language::Korean, "publish_web") => "공개 웹 공유 (폰 + SNS)",
        (Language::Korean, "serve_lan") => "폰에서 보기 (같은 Wi-Fi/LAN)",
        (Language::Korean, "settings") => "설정",
        (Language::Korean, "quit") => "종료",
        (Language::Korean, "star_github") => "GitHub 스타 주기",
        (Language::Korean, "back") => "뒤로",
        (Language::Korean, "rematch") => "같은 상대로 재대결",
        (Language::Korean, "new_portraits") => "새 사진 선택",
        (Language::Korean, "battle_setup") => "배틀 준비",
        (Language::Korean, "left_portrait") => "왼쪽 초상화",
        (Language::Korean, "right_portrait") => "오른쪽 초상화",
        (Language::Korean, "waiting") => "대기 중...",
        (Language::Korean, "ready") => "준비 완료",
        (Language::Korean, "switch_side") => "패널 전환",
        (Language::Korean, "fill_both") => "양쪽 다 입력",
        (Language::Korean, "cancel") => "취소",
        (Language::Korean, "press_start") => "아무 키를 눌러 시작",
        (Language::Korean, "analyzing") => "분석 중...",
        (Language::Korean, "battle_result") => "배틀 결과",
        (Language::Korean, "winner") => "승자",
        (Language::Korean, "judge_mode") => "심판 모드",
        (Language::Korean, "model") => "모델",
        (Language::Korean, "language") => "언어",
        (Language::Korean, "api_keys") => "API 키 관리",
        (Language::Korean, "labels") => "라벨",
        (Language::Korean, "output_dir") => "출력 디렉토리",
        (Language::Korean, "aesthetic_tuning") => "미적 조정",
        // Report
        (Language::Korean, "report_title") => "배틀 리포트",
        (Language::Korean, "report_winner") => "승자",
        (Language::Korean, "report_overall_take") => "총평",
        (Language::Korean, "report_why_won") => "승리 이유",
        (Language::Korean, "report_jury_notes") => "심사위원 노트",
        (Language::Korean, "report_heuristic") => "휴리스틱 분석",
        (Language::Korean, "report_vlm") => "AI 심사관",
        (Language::Korean, "report_combined") => "종합 점수",
        (Language::Korean, "report_score") => "점수",
        (Language::Korean, "report_strengths") => "강점",
        (Language::Korean, "report_weaknesses") => "약점",
        (Language::Korean, "report_ability_comparison") => "능력치 비교",
        (Language::Korean, "report_portrait_analysis") => "사진별 분석",
        (Language::Korean, "report_margin") => "격차",
        (Language::Korean, "report_decisive") => "압도적!",
        (Language::Korean, "report_left") => "왼쪽",
        (Language::Korean, "report_right") => "오른쪽",
        (Language::Korean, "report_vs") => "VS",
        (Language::Korean, "report_generated") => "생성 시각",
        (Language::Korean, "axis_facial_symmetry") => "얼굴 대칭",
        (Language::Korean, "axis_facial_proportions") => "얼굴 비율",
        (Language::Korean, "axis_skin_quality") => "피부 상태",
        (Language::Korean, "axis_eye_expression") => "눈 표현력",
        (Language::Korean, "axis_hair_grooming") => "헤어 & 그루밍",
        (Language::Korean, "axis_bone_structure") => "골격 구조",
        (Language::Korean, "axis_expression_charisma") => "표정 & 카리스마",
        (Language::Korean, "axis_lighting_color") => "조명 & 색감",
        (Language::Korean, "axis_background_framing") => "배경 & 구도",
        (Language::Korean, "axis_photogenic_impact") => "포토제닉 임팩트",
        // Short axis labels (for tight UI)
        (Language::Korean, "short_facial_symmetry") => "대칭",
        (Language::Korean, "short_facial_proportions") => "비율",
        (Language::Korean, "short_skin_quality") => "피부",
        (Language::Korean, "short_eye_expression") => "눈",
        (Language::Korean, "short_hair_grooming") => "헤어",
        (Language::Korean, "short_bone_structure") => "골격",
        (Language::Korean, "short_expression_charisma") => "아우라",
        (Language::Korean, "short_lighting_color") => "조명",
        (Language::Korean, "short_background_framing") => "배경",
        (Language::Korean, "short_photogenic_impact") => "임팩트",
        // Axis descriptions (for "what does this mean")
        (Language::Korean, "desc_facial_symmetry") => "좌우 얼굴 균형",
        (Language::Korean, "desc_facial_proportions") => "황금비율, 이목구비 배치",
        (Language::Korean, "desc_skin_quality") => "매끄러움, 톤 균일성",
        (Language::Korean, "desc_eye_expression") => "눈의 생동감과 표현력",
        (Language::Korean, "desc_hair_grooming") => "스타일과 프레이밍",
        (Language::Korean, "desc_bone_structure") => "턱선과 골격 정의",
        (Language::Korean, "desc_expression_charisma") => "표정과 분위기",
        (Language::Korean, "desc_lighting_color") => "조명 품질과 색감",
        (Language::Korean, "desc_background_framing") => "배경과 구도",
        (Language::Korean, "desc_photogenic_impact") => "첫인상 임팩트",

        (Language::Japanese, "start_battle") => "バトル開始",
        (Language::Japanese, "open_report") => "最新レポートを開く",
        (Language::Japanese, "share_result") => "SNSシェア",
        (Language::Japanese, "publish_web") => "公開ウェブ共有（スマホ + SNS）",
        (Language::Japanese, "serve_lan") => "スマホで見る（同じ Wi-Fi / LAN）",
        (Language::Japanese, "settings") => "設定",
        (Language::Japanese, "quit") => "終了",
        (Language::Japanese, "star_github") => "GitHubスター",
        (Language::Japanese, "back") => "戻る",
        (Language::Japanese, "rematch") => "再戦",
        (Language::Japanese, "new_portraits") => "新しい写真を選択",
        (Language::Japanese, "battle_setup") => "バトル準備",
        (Language::Japanese, "left_portrait") => "左の写真",
        (Language::Japanese, "right_portrait") => "右の写真",
        (Language::Japanese, "waiting") => "待機中...",
        (Language::Japanese, "ready") => "準備完了",
        (Language::Japanese, "switch_side") => "パネル切替",
        (Language::Japanese, "fill_both") => "両方入力",
        (Language::Japanese, "cancel") => "キャンセル",
        (Language::Japanese, "press_start") => "キーを押してスタート",
        (Language::Japanese, "analyzing") => "分析中...",
        (Language::Japanese, "battle_result") => "バトル結果",
        (Language::Japanese, "winner") => "勝者",
        (Language::Japanese, "judge_mode") => "審査モード",
        (Language::Japanese, "model") => "モデル",
        (Language::Japanese, "language") => "言語",
        (Language::Japanese, "api_keys") => "APIキー管理",
        (Language::Japanese, "labels") => "ラベル",
        (Language::Japanese, "output_dir") => "出力ディレクトリ",
        (Language::Japanese, "aesthetic_tuning") => "美的調整",
        // Report
        (Language::Japanese, "report_title") => "バトルレポート",
        (Language::Japanese, "report_winner") => "勝者",
        (Language::Japanese, "report_overall_take") => "総評",
        (Language::Japanese, "report_why_won") => "勝利の理由",
        (Language::Japanese, "report_jury_notes") => "ジュリーノート",
        (Language::Japanese, "report_heuristic") => "ヒューリスティック分析",
        (Language::Japanese, "report_vlm") => "AI審査官",
        (Language::Japanese, "report_combined") => "総合スコア",
        (Language::Japanese, "report_score") => "スコア",
        (Language::Japanese, "report_strengths") => "強み",
        (Language::Japanese, "report_weaknesses") => "弱み",
        (Language::Japanese, "report_ability_comparison") => "能力値比較",
        (Language::Japanese, "report_portrait_analysis") => "写真別分析",
        (Language::Japanese, "report_margin") => "差",
        (Language::Japanese, "report_decisive") => "圧倒的！",
        (Language::Japanese, "report_left") => "左",
        (Language::Japanese, "report_right") => "右",
        (Language::Japanese, "report_vs") => "VS",
        (Language::Japanese, "report_generated") => "生成時刻",
        (Language::Japanese, "axis_facial_symmetry") => "顔の対称性",
        (Language::Japanese, "axis_facial_proportions") => "顔のプロポーション",
        (Language::Japanese, "axis_skin_quality") => "肌の質",
        (Language::Japanese, "axis_eye_expression") => "目の表現力",
        (Language::Japanese, "axis_hair_grooming") => "ヘア & グルーミング",
        (Language::Japanese, "axis_bone_structure") => "骨格構造",
        (Language::Japanese, "axis_expression_charisma") => "表情 & カリスマ",
        (Language::Japanese, "axis_lighting_color") => "照明 & 色",
        (Language::Japanese, "axis_background_framing") => "背景 & 構図",
        (Language::Japanese, "axis_photogenic_impact") => "フォトジェニックインパクト",
        // Short axis labels
        (Language::Japanese, "short_facial_symmetry") => "対称",
        (Language::Japanese, "short_facial_proportions") => "比率",
        (Language::Japanese, "short_skin_quality") => "肌",
        (Language::Japanese, "short_eye_expression") => "目",
        (Language::Japanese, "short_hair_grooming") => "髪",
        (Language::Japanese, "short_bone_structure") => "骨格",
        (Language::Japanese, "short_expression_charisma") => "オーラ",
        (Language::Japanese, "short_lighting_color") => "照明",
        (Language::Japanese, "short_background_framing") => "背景",
        (Language::Japanese, "short_photogenic_impact") => "インパクト",
        // Descriptions
        (Language::Japanese, "desc_facial_symmetry") => "左右のバランス",
        (Language::Japanese, "desc_facial_proportions") => "黄金比と配置",
        (Language::Japanese, "desc_skin_quality") => "滑らかさと均一性",
        (Language::Japanese, "desc_eye_expression") => "目の生命力と表現",
        (Language::Japanese, "desc_hair_grooming") => "スタイルとフレーミング",
        (Language::Japanese, "desc_bone_structure") => "顎のラインと骨格",
        (Language::Japanese, "desc_expression_charisma") => "表情と雰囲気",
        (Language::Japanese, "desc_lighting_color") => "照明品質と色彩",
        (Language::Japanese, "desc_background_framing") => "背景と構図",
        (Language::Japanese, "desc_photogenic_impact") => "第一印象のインパクト",

        // English defaults
        (_, "start_battle") => "Start Battle",
        (_, "open_report") => "Open Latest Report",
        (_, "share_result") => "Share to SNS",
        (_, "publish_web") => "Public Web Share (Phone + SNS)",
        (_, "serve_lan") => "View on Phone (same Wi-Fi / LAN)",
        (_, "settings") => "Settings",
        (_, "quit") => "Quit",
        (_, "star_github") => "Star BetterThanYou on GitHub",
        (_, "back") => "Back",
        (_, "rematch") => "Rematch Same Pair",
        (_, "new_portraits") => "Choose New Portraits",
        (_, "battle_setup") => "Battle Setup",
        (_, "left_portrait") => "LEFT PORTRAIT",
        (_, "right_portrait") => "RIGHT PORTRAIT",
        (_, "waiting") => "waiting...",
        (_, "ready") => "ready",
        (_, "switch_side") => "Switch side",
        (_, "fill_both") => "fill both",
        (_, "cancel") => "Cancel",
        (_, "press_start") => "PRESS ANY KEY TO START",
        (_, "analyzing") => "Analyzing...",
        (_, "battle_result") => "BATTLE RESULT",
        (_, "winner") => "WINNER",
        (_, "judge_mode") => "Judge mode",
        (_, "model") => "Model",
        (_, "language") => "Language",
        (_, "api_keys") => "API keys",
        (_, "labels") => "Labels",
        (_, "output_dir") => "Output directory",
        (_, "aesthetic_tuning") => "Aesthetic tuning",
        // Report defaults
        (_, "report_title") => "Battle Report",
        (_, "report_winner") => "Winner",
        (_, "report_overall_take") => "Overall Take",
        (_, "report_why_won") => "Why This Won",
        (_, "report_jury_notes") => "Jury Notes",
        (_, "report_heuristic") => "Heuristic Analysis",
        (_, "report_vlm") => "AI Judge",
        (_, "report_combined") => "Combined Score",
        (_, "report_score") => "Score",
        (_, "report_strengths") => "Strengths",
        (_, "report_weaknesses") => "Weaknesses",
        (_, "report_ability_comparison") => "Ability Comparison",
        (_, "report_portrait_analysis") => "Per-Portrait Analysis",
        (_, "report_margin") => "Margin",
        (_, "report_decisive") => "DECISIVE!",
        (_, "report_left") => "LEFT",
        (_, "report_right") => "RIGHT",
        (_, "report_vs") => "VS",
        (_, "report_generated") => "Generated",
        (_, "axis_facial_symmetry") => "Facial Symmetry",
        (_, "axis_facial_proportions") => "Facial Proportions",
        (_, "axis_skin_quality") => "Skin Quality",
        (_, "axis_eye_expression") => "Eye Expression",
        (_, "axis_hair_grooming") => "Hair & Grooming",
        (_, "axis_bone_structure") => "Bone Structure",
        (_, "axis_expression_charisma") => "Expression & Charisma",
        (_, "axis_lighting_color") => "Lighting & Color",
        (_, "axis_background_framing") => "Background & Framing",
        (_, "axis_photogenic_impact") => "Photogenic Impact",
        // Short axis labels
        (_, "short_facial_symmetry") => "SYM",
        (_, "short_facial_proportions") => "RATIO",
        (_, "short_skin_quality") => "SKIN",
        (_, "short_eye_expression") => "EYES",
        (_, "short_hair_grooming") => "HAIR",
        (_, "short_bone_structure") => "BONE",
        (_, "short_expression_charisma") => "AURA",
        (_, "short_lighting_color") => "LIGHT",
        (_, "short_background_framing") => "FRAME",
        (_, "short_photogenic_impact") => "IMPACT",
        // Descriptions
        (_, "desc_facial_symmetry") => "Left-right face balance",
        (_, "desc_facial_proportions") => "Golden ratio & feature placement",
        (_, "desc_skin_quality") => "Smoothness & evenness",
        (_, "desc_eye_expression") => "Eye vibrance & emotion",
        (_, "desc_hair_grooming") => "Style & framing",
        (_, "desc_bone_structure") => "Jawline & structure",
        (_, "desc_expression_charisma") => "Expression & mood",
        (_, "desc_lighting_color") => "Lighting quality & color",
        (_, "desc_background_framing") => "Background & composition",
        (_, "desc_photogenic_impact") => "First-impression impact",
        _ => "",
    }
}

/// Short axis label (2-6 chars) suitable for cramped UI.
pub fn localized_axis_short(lang: Language, key: &str) -> String {
    let mapped = match key {
        "facial_symmetry" => "short_facial_symmetry",
        "facial_proportions" => "short_facial_proportions",
        "skin_quality" => "short_skin_quality",
        "eye_expression" => "short_eye_expression",
        "hair_grooming" => "short_hair_grooming",
        "bone_structure" => "short_bone_structure",
        "expression_charisma" => "short_expression_charisma",
        "lighting_color" => "short_lighting_color",
        "background_framing" => "short_background_framing",
        "photogenic_impact" => "short_photogenic_impact",
        _ => return key.to_string(),
    };
    t(lang, mapped).to_string()
}

/// Brief description of what an axis measures.
pub fn localized_axis_desc(lang: Language, key: &str) -> String {
    let mapped = match key {
        "facial_symmetry" => "desc_facial_symmetry",
        "facial_proportions" => "desc_facial_proportions",
        "skin_quality" => "desc_skin_quality",
        "eye_expression" => "desc_eye_expression",
        "hair_grooming" => "desc_hair_grooming",
        "bone_structure" => "desc_bone_structure",
        "expression_charisma" => "desc_expression_charisma",
        "lighting_color" => "desc_lighting_color",
        "background_framing" => "desc_background_framing",
        "photogenic_impact" => "desc_photogenic_impact",
        _ => return String::new(),
    };
    t(lang, mapped).to_string()
}

/// Icon for an axis (used in UI).
pub fn axis_icon(key: &str) -> &'static str {
    AXIS_DEFINITIONS.iter().find(|a| a.key == key).map(|a| a.icon).unwrap_or("")
}

/// Look up localized axis label by axis key.
pub fn localized_axis_label(lang: Language, key: &str) -> String {
    let mapped = match key {
        "facial_symmetry" => "axis_facial_symmetry",
        "facial_proportions" => "axis_facial_proportions",
        "skin_quality" => "axis_skin_quality",
        "eye_expression" => "axis_eye_expression",
        "hair_grooming" => "axis_hair_grooming",
        "bone_structure" => "axis_bone_structure",
        "expression_charisma" => "axis_expression_charisma",
        "lighting_color" => "axis_lighting_color",
        "background_framing" => "axis_background_framing",
        "photogenic_impact" => "axis_photogenic_impact",
        _ => return key.to_string(),
    };
    t(lang, mapped).to_string()
}

#[derive(Debug, Clone, Copy)]
pub struct AxisDefinition {
    pub key: &'static str,
    pub label: &'static str,
    pub short: &'static str,
    pub icon: &'static str,
    pub weight: f32,
}

pub const AXIS_DEFINITIONS: [AxisDefinition; 10] = [
    AxisDefinition { key: "facial_symmetry",     label: "Facial Symmetry",     short: "SYM",   icon: "\u{269C}",   weight: 1.0 }, // ⚜
    AxisDefinition { key: "facial_proportions",  label: "Facial Proportions",  short: "RATIO", icon: "\u{25C6}",   weight: 1.0 }, // ◆
    AxisDefinition { key: "skin_quality",        label: "Skin Quality",        short: "SKIN",  icon: "\u{2728}",   weight: 1.0 }, // ✨
    AxisDefinition { key: "eye_expression",      label: "Eye Expression",      short: "EYES",  icon: "\u{1F441}",  weight: 1.1 }, // 👁
    AxisDefinition { key: "hair_grooming",       label: "Hair & Grooming",     short: "HAIR",  icon: "\u{2702}",   weight: 0.8 }, // ✂
    AxisDefinition { key: "bone_structure",      label: "Bone Structure",      short: "BONE",  icon: "\u{1F9B4}",  weight: 0.9 }, // 🦴
    AxisDefinition { key: "expression_charisma", label: "Expression & Charisma", short: "AURA",  icon: "\u{1F525}",  weight: 1.2 }, // 🔥
    AxisDefinition { key: "lighting_color",      label: "Lighting & Color",    short: "LIGHT", icon: "\u{1F4A1}",  weight: 1.0 }, // 💡
    AxisDefinition { key: "background_framing",  label: "Background & Framing", short: "FRAME", icon: "\u{1F5BC}",  weight: 0.8 }, // 🖼
    AxisDefinition { key: "photogenic_impact",   label: "Photogenic Impact",   short: "IMPACT", icon: "\u{1F4A5}",  weight: 1.3 }, // 💥
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AxisScores {
    #[serde(default)]
    pub facial_symmetry: f32,
    #[serde(default)]
    pub facial_proportions: f32,
    #[serde(default)]
    pub skin_quality: f32,
    #[serde(default)]
    pub eye_expression: f32,
    #[serde(default)]
    pub hair_grooming: f32,
    #[serde(default)]
    pub bone_structure: f32,
    #[serde(default)]
    pub expression_charisma: f32,
    #[serde(default)]
    pub lighting_color: f32,
    #[serde(default)]
    pub background_framing: f32,
    #[serde(default)]
    pub photogenic_impact: f32,
}

impl AxisScores {
    pub fn get(&self, key: &str) -> f32 {
        match key {
            "facial_symmetry" => self.facial_symmetry,
            "facial_proportions" => self.facial_proportions,
            "skin_quality" => self.skin_quality,
            "eye_expression" => self.eye_expression,
            "hair_grooming" => self.hair_grooming,
            "bone_structure" => self.bone_structure,
            "expression_charisma" => self.expression_charisma,
            "lighting_color" => self.lighting_color,
            "background_framing" => self.background_framing,
            "photogenic_impact" => self.photogenic_impact,
            _ => 0.0,
        }
    }

    pub fn set(&mut self, key: &str, value: f32) {
        match key {
            "facial_symmetry" => self.facial_symmetry = value,
            "facial_proportions" => self.facial_proportions = value,
            "skin_quality" => self.skin_quality = value,
            "eye_expression" => self.eye_expression = value,
            "hair_grooming" => self.hair_grooming = value,
            "bone_structure" => self.bone_structure = value,
            "expression_charisma" => self.expression_charisma = value,
            "lighting_color" => self.lighting_color = value,
            "background_framing" => self.background_framing = value,
            "photogenic_impact" => self.photogenic_impact = value,
            _ => {}
        }
    }

    /// Weighted blend: self * a + other * b (weights should sum to 1.0)
    pub fn blend(&self, other: &Self, self_weight: f32, other_weight: f32) -> Self {
        let mut out = Self::default();
        for axis in AXIS_DEFINITIONS.iter() {
            let v = self.get(axis.key) * self_weight + other.get(axis.key) * other_weight;
            out.set(axis.key, round(v));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreTelemetry {
    pub mirror_difference: f32,
    pub edge_strength: f32,
    pub center_presence: f32,
    pub dynamic_range: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBundle {
    pub axes: AxisScores,
    pub total: f32,
    pub telemetry: Option<ScoreTelemetry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualScores {
    pub heuristic: SideScores,
    #[serde(default)]
    pub vlm: Option<SideScores>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortraitRef {
    pub id: String,
    pub label: String,
    pub source_type: String,
    pub width: u32,
    pub height: u32,
    pub hash: String,
    pub image_data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleInputs {
    pub left: PortraitRef,
    pub right: PortraitRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Winner {
    pub id: String,
    pub label: String,
    pub total_score: f32,
    pub opponent_score: f32,
    pub margin: f32,
    pub decisive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisCard {
    pub key: String,
    pub label: String,
    pub left: f32,
    pub right: f32,
    pub diff: f32,
    pub leader: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleSections {
    pub overall_take: String,
    pub strengths: SideTexts,
    pub weaknesses: SideTexts,
    pub why_this_won: String,
    pub model_jury_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideTexts {
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineMeta {
    pub version: String,
    pub qualitative_sections: Vec<String>,
    pub judge_mode: String,
    pub provider: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleResult {
    pub battle_id: String,
    pub product_name: String,
    pub created_at: String,
    pub engine: EngineMeta,
    pub winner_first: bool,
    pub quantitative_axes: Vec<String>,
    pub qualitative_sections: Vec<String>,
    pub inputs: BattleInputs,
    pub scores: SideScores,
    #[serde(default)]
    pub dual_scores: Option<DualScores>,
    pub axis_cards: Vec<AxisCard>,
    pub winner: Winner,
    pub sections: BattleSections,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideScores {
    pub left: ScoreBundle,
    pub right: ScoreBundle,
}

#[derive(Debug, Clone)]
struct LoadedPortrait {
    id: String,
    label: String,
    source_type: String,
    width: u32,
    height: u32,
    hash: String,
    image_data_url: String,
    image: DynamicImage,
}

#[derive(Debug, Clone)]
pub struct OpenAiJudgeOutput {
    pub winner_id: String,
    pub left_scores: AxisScores,
    pub right_scores: AxisScores,
    pub sections: BattleSections,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct OpenAiConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnalyzeOptions {
    pub left_source: String,
    pub right_source: String,
    pub left_label: Option<String>,
    pub right_label: Option<String>,
    pub judge_mode: JudgeMode,
    pub openai_model: String,
    pub openai_config: OpenAiConfig,
    pub axis_weights: Vec<(String, f32)>,
    pub language: Language,
}

impl AnalyzeOptions {
    pub fn new(left_source: impl Into<String>, right_source: impl Into<String>) -> Self {
        Self {
            left_source: left_source.into(),
            right_source: right_source.into(),
            left_label: None,
            right_label: None,
            judge_mode: JudgeMode::Auto,
            openai_model: DEFAULT_OPENAI_MODEL.to_string(),
            openai_config: OpenAiConfig::default(),
            axis_weights: Vec::new(),
            language: Language::English,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedArtifacts {
    pub html_path: String,
    pub json_path: String,
    pub latest_html_path: String,
    pub latest_json_path: String,
}

fn round(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.min(max).max(min)
}

fn average(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f32>() / values.len() as f32
}

fn stddev(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = average(values);
    let variance = average(&values.iter().map(|v| (v - mean).powi(2)).collect::<Vec<_>>());
    variance.sqrt()
}

fn percentile(values: &[f32], ratio: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let index = ((sorted.len() - 1) as f32 * ratio).floor() as usize;
    sorted[index]
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in input.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches('-').chars().take(48).collect::<String>()
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn hash_signal(hash: &str, index: usize, scale: f32) -> f32 {
    let offset = (index * 2) % hash.len();
    let end = (offset + 2).min(hash.len());
    let slice = &hash[offset..end];
    let value = u8::from_str_radix(slice, 16).unwrap_or(0) as f32;
    (value / 255.0) * scale
}

fn infer_mime_type(source: &str) -> &'static str {
    let lower = source.to_lowercase();
    if lower.starts_with("data:image/jpeg") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    }
}

fn normalize_source_input(input: &str) -> String {
    let mut value = input.trim().to_string();
    if (value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')) {
        value = value[1..value.len() - 1].to_string();
    }
    value = value.replace("\\ ", " ");
    if value.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            value = PathBuf::from(home).join(&value[2..]).display().to_string();
        }
    }
    value
}

/// Extract a human-friendly label from a source (filename stem without extension).
/// Returns None for data URLs or if no meaningful name can be extracted.
fn label_from_source(source: &str) -> Option<String> {
    if source.starts_with("data:") {
        return None;
    }
    let normalized = normalize_source_input(source);
    // Strip query/fragment for URLs
    let clean = normalized.split(['?', '#']).next().unwrap_or(&normalized);
    let path = Path::new(clean);
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let trimmed = stem.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn looks_like_base64(value: &str) -> bool {
    value.len() > 96 && value.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '\n' | '\r'))
}

async fn fetch_url_bytes(url: &str) -> Result<Vec<u8>> {
    let response = Client::new().get(url).send().await.with_context(|| format!("failed to fetch {url}"))?;
    if !response.status().is_success() {
        bail!("failed to fetch {url}: HTTP {}", response.status());
    }
    Ok(response.bytes().await?.to_vec())
}

async fn load_portrait(source: &str, label: Option<&str>, side: &str) -> Result<LoadedPortrait> {
    let normalized = normalize_source_input(source);
    let bytes = if normalized.starts_with("data:image/") {
        let (_, encoded) = normalized.split_once(',').ok_or_else(|| anyhow!("invalid data URL"))?;
        base64::engine::general_purpose::STANDARD.decode(encoded)?
    } else if normalized.starts_with("http://") || normalized.starts_with("https://") {
        fetch_url_bytes(&normalized).await?
    } else if Path::new(&normalized).exists() {
        fs::read(&normalized)?
    } else if looks_like_base64(&normalized) {
        base64::engine::general_purpose::STANDARD.decode(normalized.replace(['\r', '\n'], ""))?
    } else {
        bail!("unsupported portrait input: {normalized}");
    };

    // Apply EXIF orientation so the analysis pixels match what the user's
    // camera reported. Without this, photos with non-default EXIF orientation
    // (especially front-camera selfies with the mirror flag) render flipped
    // in the browser HTML report but un-flipped in the share PNG, producing
    // a confusing left/right-reversed image. After this fix the data URL is
    // a re-encoded PNG with no EXIF metadata, so analysis, browser, and
    // share PNG all agree on the same orientation.
    use std::io::Cursor;
    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .context("failed to detect image format")?;
    let mut decoder = reader
        .into_decoder()
        .context("failed to create image decoder")?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = image::DynamicImage::from_decoder(decoder)
        .context("failed to decode image")?;
    image.apply_orientation(orientation);

    // Re-encode to PNG so the data URL has no EXIF, guaranteeing browsers
    // render the same pixels we just analyzed.
    let mut png_bytes: Vec<u8> = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .context("failed to re-encode portrait as PNG")?;

    let hash = hash_bytes(&png_bytes);
    let final_label = label
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| label_from_source(source))
        .unwrap_or_else(|| side.to_string());

    Ok(LoadedPortrait {
        id: side.to_string(),
        label: final_label,
        source_type: if normalized.starts_with("http") { "url".into() } else if normalized.starts_with("data:image/") { "data-url".into() } else if Path::new(&normalized).exists() { "path".into() } else { "base64".into() },
        width: image.width(),
        height: image.height(),
        hash,
        image_data_url: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&png_bytes)
        ),
        image,
    })
}

fn compute_luminance(r: f32, g: f32, b: f32) -> f32 {
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
}

fn compute_saturation(r: f32, g: f32, b: f32) -> f32 {
    let max = r.max(g).max(b) / 255.0;
    let min = r.min(g).min(b) / 255.0;
    if max == 0.0 { 0.0 } else { (max - min) / max }
}

#[derive(Clone, Copy)]
struct Sample {
    luminance: f32,
    saturation: f32,
    r: f32,
    g: f32,
    b: f32,
    center_weight: f32,
}

fn sample_grid(image: &DynamicImage, grid_width: u32, grid_height: u32) -> Vec<Vec<Sample>> {
    let rgba = image.to_rgba8();
    let mut rows = Vec::new();
    for row in 0..grid_height {
        let y = ((row as f32 / (grid_height.saturating_sub(1).max(1)) as f32) * (rgba.height() - 1) as f32).round() as u32;
        let mut cols = Vec::new();
        for col in 0..grid_width {
            let x = ((col as f32 / (grid_width.saturating_sub(1).max(1)) as f32) * (rgba.width() - 1) as f32).round() as u32;
            let pixel = rgba.get_pixel(x, y).0;
            let r = pixel[0] as f32;
            let g = pixel[1] as f32;
            let b = pixel[2] as f32;
            let nx = col as f32 / grid_width.saturating_sub(1).max(1) as f32;
            let ny = row as f32 / grid_height.saturating_sub(1).max(1) as f32;
            let dx = nx - 0.5;
            let dy = ny - 0.45;
            let distance = (dx * dx + dy * dy).sqrt() / 0.72;
            cols.push(Sample {
                luminance: compute_luminance(r, g, b),
                saturation: compute_saturation(r, g, b),
                r,
                g,
                b,
                center_weight: 1.0 - clamp(distance, 0.0, 1.0),
            });
        }
        rows.push(cols);
    }
    rows
}

fn flatten_grid(grid: &[Vec<Sample>]) -> Vec<Sample> {
    grid.iter().flat_map(|row| row.iter().copied()).collect()
}

fn compute_mirror_difference(grid: &[Vec<Sample>]) -> f32 {
    let mut diffs = Vec::new();
    for row in grid {
        let half = row.len() / 2;
        for idx in 0..half {
            let left = row[idx];
            let right = row[row.len() - 1 - idx];
            diffs.push((left.luminance - right.luminance).abs());
        }
    }
    average(&diffs)
}

fn compute_edge_strength(grid: &[Vec<Sample>]) -> f32 {
    let mut strengths = Vec::new();
    for row in 0..grid.len() {
        for col in 0..grid[row].len() {
            let current = grid[row][col];
            if let Some(right) = grid[row].get(col + 1) {
                strengths.push((current.luminance - right.luminance).abs());
            }
            if let Some(next_row) = grid.get(row + 1) {
                strengths.push((current.luminance - next_row[col].luminance).abs());
            }
        }
    }
    average(&strengths)
}

fn compute_center_presence(flat: &[Sample]) -> f32 {
    let center: Vec<f32> = flat.iter().filter(|s| s.center_weight >= 0.55).map(|s| s.saturation * 0.45 + s.luminance * 0.2 + s.center_weight * 0.35).collect();
    let outer: Vec<f32> = flat.iter().filter(|s| s.center_weight < 0.55).map(|s| s.saturation * 0.4 + s.luminance * 0.2).collect();
    clamp(average(&center) - average(&outer) + 0.55, 0.0, 1.0)
}

fn compute_palette_mood(flat: &[Sample]) -> f32 {
    let warmth: Vec<f32> = flat.iter().map(|s| (s.r - s.b) / 255.0).collect();
    let vibrance: Vec<f32> = flat.iter().map(|s| s.saturation).collect();
    clamp((average(&warmth) + 1.0) * 0.25 + average(&vibrance) * 0.65, 0.0, 1.0)
}

/// Sample a rectangular sub-region of the grid by normalized bounds (0..1).
/// y0/y1 are vertical bounds (top=0), x0/x1 are horizontal bounds.
fn region_samples(grid: &[Vec<Sample>], x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Sample> {
    let h = grid.len();
    if h == 0 { return Vec::new(); }
    let w = grid[0].len();
    if w == 0 { return Vec::new(); }
    let row_start = ((y0 * h as f32).max(0.0) as usize).min(h.saturating_sub(1));
    let row_end = ((y1 * h as f32).ceil() as usize).min(h);
    let col_start = ((x0 * w as f32).max(0.0) as usize).min(w.saturating_sub(1));
    let col_end = ((x1 * w as f32).ceil() as usize).min(w);
    let mut out = Vec::new();
    for r in row_start..row_end {
        for c in col_start..col_end {
            out.push(grid[r][c]);
        }
    }
    out
}

/// Texture variance over a region — used for skin smoothness (low variance = smooth skin).
fn region_texture_variance(grid: &[Vec<Sample>], x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let h = grid.len();
    if h == 0 { return 0.0; }
    let w = grid[0].len();
    if w == 0 { return 0.0; }
    let row_start = ((y0 * h as f32).max(0.0) as usize).min(h.saturating_sub(1));
    let row_end = ((y1 * h as f32).ceil() as usize).min(h);
    let col_start = ((x0 * w as f32).max(0.0) as usize).min(w.saturating_sub(1));
    let col_end = ((x1 * w as f32).ceil() as usize).min(w);
    let mut diffs = Vec::new();
    for r in row_start..row_end {
        for c in col_start..col_end {
            if c + 1 < col_end {
                diffs.push((grid[r][c].luminance - grid[r][c + 1].luminance).abs());
            }
            if r + 1 < row_end {
                diffs.push((grid[r][c].luminance - grid[r + 1][c].luminance).abs());
            }
        }
    }
    average(&diffs)
}

/// Edge density over a region — high value = sharp/well-defined features.
fn region_edge_density(grid: &[Vec<Sample>], x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    region_texture_variance(grid, x0, y0, x1, y1)
}

/// Symmetry of a horizontal slice (upper or lower half of face).
fn region_mirror_diff(grid: &[Vec<Sample>], y0: f32, y1: f32) -> f32 {
    let h = grid.len();
    if h == 0 { return 0.5; }
    let w = grid[0].len();
    if w == 0 { return 0.5; }
    let row_start = ((y0 * h as f32).max(0.0) as usize).min(h.saturating_sub(1));
    let row_end = ((y1 * h as f32).ceil() as usize).min(h);
    let mut diffs = Vec::new();
    for r in row_start..row_end {
        let half = w / 2;
        for idx in 0..half {
            let l = grid[r][idx];
            let rr = grid[r][w - 1 - idx];
            diffs.push((l.luminance - rr.luminance).abs());
        }
    }
    average(&diffs)
}

fn score_portrait(portrait: &LoadedPortrait, axis_definitions: &[AxisDefinition]) -> ScoreBundle {
    let grid = sample_grid(&portrait.image, 48, 60);
    let flat = flatten_grid(&grid);
    let luminances: Vec<f32> = flat.iter().map(|s| s.luminance).collect();
    let saturations: Vec<f32> = flat.iter().map(|s| s.saturation).collect();
    let color_spread = average(&flat.iter().map(|s| stddev(&[s.r / 255.0, s.g / 255.0, s.b / 255.0])).collect::<Vec<_>>());
    let mirror_difference = compute_mirror_difference(&grid);
    let edge_strength = compute_edge_strength(&grid);
    let center_presence = compute_center_presence(&flat);
    let dynamic_range = percentile(&luminances, 0.9) - percentile(&luminances, 0.1);
    let luminance_deviation = stddev(&luminances);
    let saturation_deviation = stddev(&saturations);
    let palette_mood = compute_palette_mood(&flat);

    // ── Region-based heuristics (approximated face regions) ──────────────
    // Face region: center 60% width, upper 15% → lower 85% height
    let face_samples = region_samples(&grid, 0.2, 0.15, 0.8, 0.85);
    let face_lums: Vec<f32> = face_samples.iter().map(|s| s.luminance).collect();
    let face_sats: Vec<f32> = face_samples.iter().map(|s| s.saturation).collect();

    // Eye region: center, upper portion (30-50% height)
    let eye_samples = region_samples(&grid, 0.25, 0.28, 0.75, 0.48);
    let eye_lums: Vec<f32> = eye_samples.iter().map(|s| s.luminance).collect();
    let eye_contrast = if eye_lums.is_empty() { 0.0 } else { percentile(&eye_lums, 0.9) - percentile(&eye_lums, 0.1) };
    let eye_edge = region_edge_density(&grid, 0.25, 0.28, 0.75, 0.48);

    // Skin region: cheeks and forehead (lower-middle of face)
    let skin_variance = region_texture_variance(&grid, 0.28, 0.38, 0.72, 0.72);
    let skin_color_uniformity = 1.0 - stddev(&face_sats).min(1.0);

    // Hair region: top 30% of frame
    let hair_edge = region_edge_density(&grid, 0.1, 0.0, 0.9, 0.3);
    let hair_samples = region_samples(&grid, 0.1, 0.0, 0.9, 0.25);
    let hair_sat_consistency = 1.0 - stddev(&hair_samples.iter().map(|s| s.saturation).collect::<Vec<_>>()).min(1.0);

    // Jawline/bone structure: lower face (60-90% height)
    let jaw_edge = region_edge_density(&grid, 0.2, 0.60, 0.8, 0.90);
    let jaw_contrast = {
        let samples = region_samples(&grid, 0.2, 0.60, 0.8, 0.90);
        let lums: Vec<f32> = samples.iter().map(|s| s.luminance).collect();
        if lums.is_empty() { 0.0 } else { percentile(&lums, 0.85) - percentile(&lums, 0.15) }
    };

    // Upper/lower face balance (for proportions)
    let upper_mirror = region_mirror_diff(&grid, 0.15, 0.55);
    let lower_mirror = region_mirror_diff(&grid, 0.55, 0.90);
    let proportion_harmony = 1.0 - (upper_mirror - lower_mirror).abs();

    // Face warmth (expression/charisma proxy)
    let face_warmth: f32 = if face_samples.is_empty() {
        0.0
    } else {
        face_samples.iter().map(|s| (s.r - s.b) / 255.0).sum::<f32>() / face_samples.len() as f32
    };
    let face_saturation_avg = average(&face_sats);
    let face_luminance_range = if face_lums.is_empty() { 0.0 } else { percentile(&face_lums, 0.9) - percentile(&face_lums, 0.1) };

    // Background (outer region)
    let bg_samples: Vec<Sample> = flat.iter().filter(|s| s.center_weight < 0.35).copied().collect();
    let bg_variance = stddev(&bg_samples.iter().map(|s| s.luminance).collect::<Vec<_>>());
    let bg_quality = clamp(1.0 - bg_variance * 1.5, 0.0, 1.0); // calmer bg = better framing

    // ── Compute axis scores ──────────────────────────────────────────────
    let h = &portrait.hash;

    let facial_symmetry = round(clamp(
        100.0 - mirror_difference * 140.0 + hash_signal(h, 0, 4.0),
        28.0, 99.0,
    ));

    let facial_proportions = round(clamp(
        proportion_harmony * 75.0 + center_presence * 20.0 + hash_signal(h, 1, 4.0) + 5.0,
        25.0, 99.0,
    ));

    let skin_quality = round(clamp(
        100.0 - skin_variance * 420.0 + skin_color_uniformity * 15.0 + hash_signal(h, 2, 4.0),
        22.0, 99.0,
    ));

    let eye_expression = round(clamp(
        eye_contrast * 110.0 + eye_edge * 220.0 + hash_signal(h, 3, 4.0) + 10.0,
        25.0, 99.0,
    ));

    let hair_grooming = round(clamp(
        hair_edge * 180.0 + hair_sat_consistency * 30.0 + hash_signal(h, 4, 4.0) + 15.0,
        22.0, 99.0,
    ));

    let bone_structure = round(clamp(
        jaw_edge * 200.0 + jaw_contrast * 85.0 + hash_signal(h, 5, 4.0) + 12.0,
        22.0, 99.0,
    ));

    let expression_charisma = round(clamp(
        center_presence * 65.0 + (face_warmth + 1.0) * 18.0 + face_saturation_avg * 35.0 + face_luminance_range * 28.0 + hash_signal(h, 6, 4.0),
        22.0, 99.0,
    ));

    let lighting_color = round(clamp(
        dynamic_range * 55.0 + luminance_deviation * 60.0 + average(&saturations) * 45.0 + saturation_deviation * 30.0 + color_spread * 25.0 + hash_signal(h, 7, 4.0),
        22.0, 99.0,
    ));

    let background_framing = round(clamp(
        center_presence * 70.0 + bg_quality * 35.0 + edge_strength * 14.0 + hash_signal(h, 8, 4.0),
        22.0, 99.0,
    ));

    let photogenic_impact = round(clamp(
        center_presence * 45.0 + palette_mood * 35.0 + dynamic_range * 25.0 + average(&saturations) * 22.0 + (1.0 - mirror_difference) * 18.0 + hash_signal(h, 9, 4.0),
        22.0, 99.0,
    ));

    let axes = AxisScores {
        facial_symmetry,
        facial_proportions,
        skin_quality,
        eye_expression,
        hair_grooming,
        bone_structure,
        expression_charisma,
        lighting_color,
        background_framing,
        photogenic_impact,
    };

    ScoreBundle {
        total: round(compute_total_from_axes(&axes, axis_definitions)),
        axes,
        telemetry: Some(ScoreTelemetry {
            mirror_difference: round(mirror_difference),
            edge_strength: round(edge_strength),
            center_presence: round(center_presence),
            dynamic_range: round(dynamic_range),
        }),
    }
}

fn compute_total_from_axes(axes: &AxisScores, axis_definitions: &[AxisDefinition]) -> f32 {
    let weighted: f32 = axis_definitions.iter().map(|axis| axes.get(axis.key) * axis.weight).sum();
    let weights: f32 = axis_definitions.iter().map(|axis| axis.weight).sum();
    if weights <= 0.0 {
        return 0.0;
    }
    weighted / weights
}

fn axis_definitions_with_overrides(overrides: &[(String, f32)]) -> Result<Vec<AxisDefinition>> {
    let mut definitions = AXIS_DEFINITIONS.to_vec();
    for (key, weight) in overrides {
        if !weight.is_finite() || *weight < 0.0 {
            bail!("Invalid axis weight: {key}={weight} (must be a finite non-negative number)");
        }
        let Some(axis) = definitions.iter_mut().find(|axis| axis.key == key) else {
            bail!("Unknown axis key: {key}");
        };
        axis.weight = *weight;
    }
    Ok(definitions)
}

fn build_axis_cards(left_scores: &ScoreBundle, right_scores: &ScoreBundle) -> Vec<AxisCard> {
    AXIS_DEFINITIONS.iter().map(|axis| {
        let left = left_scores.axes.get(axis.key);
        let right = right_scores.axes.get(axis.key);
        let leader = if (left - right).abs() < f32::EPSILON { "tie" } else if left > right { "left" } else { "right" };
        AxisCard {
            key: axis.key.to_string(),
            label: axis.label.to_string(),
            left,
            right,
            diff: round((left - right).abs()),
            leader: leader.to_string(),
        }
    }).collect()
}

fn rank_axes(scores: &AxisScores) -> Vec<AxisDefinition> {
    let mut axes = AXIS_DEFINITIONS.to_vec();
    axes.sort_by(|a, b| scores.get(b.key).partial_cmp(&scores.get(a.key)).unwrap());
    axes
}

fn build_battle_narrative(left: &LoadedPortrait, right: &LoadedPortrait, left_scores: &ScoreBundle, right_scores: &ScoreBundle, winner: &Winner, axis_cards: &[AxisCard]) -> BattleSections {
    let left_ranked = rank_axes(&left_scores.axes);
    let right_ranked = rank_axes(&right_scores.axes);
    let lead_axes: Vec<&AxisCard> = axis_cards.iter().filter(|card| card.leader == winner.id).collect();
    let decisive = lead_axes.iter().max_by(|a, b| a.diff.partial_cmp(&b.diff).unwrap()).copied().unwrap_or(&axis_cards[0]);
    let margin_word = if winner.margin >= 8.0 { "clear" } else if winner.margin >= 4.0 { "controlled" } else { "narrow" };

    BattleSections {
        overall_take: format!("{} takes the battle with a {} edge, landing at {:.1} to {:.1}. The biggest pressure points were {} and the overall style read.", winner.label, margin_word, winner.total_score, winner.opponent_score, decisive.label.to_lowercase()),
        strengths: SideTexts {
            left: format!("{} peaks in {} and {}, giving the portrait a confident first read in the side-by-side.", left.label, left_ranked[0].label.to_lowercase(), left_ranked[1].label.to_lowercase()),
            right: format!("{} shows its best form in {} and {}, which keeps the matchup competitive even when it loses.", right.label, right_ranked[0].label.to_lowercase(), right_ranked[1].label.to_lowercase()),
        },
        weaknesses: SideTexts {
            left: format!("{} leaves points on the table in {} and {}, so its lower-end moments feel less polished.", left.label, left_ranked[left_ranked.len() - 1].label.to_lowercase(), left_ranked[left_ranked.len() - 2].label.to_lowercase()),
            right: format!("{} loses ground most visibly in {} and {}, which softens the overall punch.", right.label, right_ranked[right_ranked.len() - 1].label.to_lowercase(), right_ranked[right_ranked.len() - 2].label.to_lowercase()),
        },
        why_this_won: format!("{} won because it led {} of {} axes and created its best separation in {} by {:.1} points.", winner.label, lead_axes.len(), AXIS_DEFINITIONS.len(), decisive.label.to_lowercase(), decisive.diff),
        model_jury_notes: "Jury notes are heuristic-only in v1. The engine is deterministic, favors centered portrait presence, and treats totals within 2.5 points as near toss-up territory.".to_string(),
    }
}

fn pick_winner(left: &LoadedPortrait, right: &LoadedPortrait, left_scores: &ScoreBundle, right_scores: &ScoreBundle, axis_cards: &[AxisCard], preferred: Option<&str>) -> String {
    if let Some(value) = preferred {
        if value == "left" || value == "right" {
            return value.to_string();
        }
    }
    if (left_scores.total - right_scores.total).abs() < f32::EPSILON {
        let left_leads = axis_cards.iter().filter(|card| card.leader == "left").count();
        let right_leads = axis_cards.iter().filter(|card| card.leader == "right").count();
        if left_leads == right_leads {
            return if left.hash > right.hash { "left" } else { "right" }.to_string();
        }
        return if left_leads > right_leads { "left" } else { "right" }.to_string();
    }
    if left_scores.total > right_scores.total { "left" } else { "right" }.to_string()
}

/// Make a VLM provider's error message safe to embed in user-facing prose:
/// strip HTML tags (e.g. Cloudflare's "<html><head><title>502 Bad Gateway"
/// pages), collapse whitespace, and cap length at 240 chars.
fn sanitize_fallback_reason(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let collapsed: String = out
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");
    if collapsed.len() > 240 {
        let mut truncated: String = collapsed.chars().take(237).collect();
        truncated.push('…');
        truncated
    } else {
        collapsed
    }
}

fn build_result(left: &LoadedPortrait, right: &LoadedPortrait, left_scores: ScoreBundle, right_scores: ScoreBundle, sections: BattleSections, judge_mode: JudgeMode, provider: &str, model: Option<String>, preferred_winner: Option<&str>, fallback: Option<String>) -> BattleResult {
    let axis_cards = build_axis_cards(&left_scores, &right_scores);
    let winner_id = pick_winner(left, right, &left_scores, &right_scores, &axis_cards, preferred_winner);
    let winner_left = winner_id == "left";
    let winner = Winner {
        id: winner_id.clone(),
        label: if winner_left { left.label.clone() } else { right.label.clone() },
        total_score: if winner_left { left_scores.total } else { right_scores.total },
        opponent_score: if winner_left { right_scores.total } else { left_scores.total },
        margin: round((left_scores.total - right_scores.total).abs()),
        decisive: (left_scores.total - right_scores.total).abs() >= 6.0,
    };
    let battle_id = format!("{}-{}", Utc::now().format("%Y-%m-%dt%H-%M-%S-%3fz"), slugify(&format!("{}-{}", left.label, right.label)));
    let mut final_sections = sections;
    if let Some(fallback_reason) = fallback {
        let cleaned = sanitize_fallback_reason(&fallback_reason);
        final_sections.model_jury_notes = format!(
            "{} VLM judge unavailable — fell back to heuristic. Reason: {}",
            final_sections.model_jury_notes, cleaned
        );
    }

    BattleResult {
        battle_id,
        product_name: PRODUCT_NAME.to_string(),
        created_at: Utc::now().to_rfc3339(),
        engine: EngineMeta {
            version: match judge_mode {
                JudgeMode::Heuristic => ENGINE_VERSION.to_string(),
                JudgeMode::Auto => if provider == "local" { ENGINE_VERSION.to_string() } else { format!("{}-{}", provider, model.clone().unwrap_or_default()) },
                JudgeMode::Openai => format!("openai-{}", model.clone().unwrap_or_default()),
                JudgeMode::Anthropic => format!("anthropic-{}", model.clone().unwrap_or_default()),
                JudgeMode::Gemini => format!("gemini-{}", model.clone().unwrap_or_default()),
            },
            qualitative_sections: vec!["overall_take", "strengths", "weaknesses", "why_this_won", "model_jury_notes"].into_iter().map(str::to_string).collect(),
            judge_mode: judge_mode.as_str().to_string(),
            provider: provider.to_string(),
            model,
        },
        winner_first: true,
        quantitative_axes: AXIS_DEFINITIONS.iter().map(|axis| axis.key.to_string()).collect(),
        qualitative_sections: vec!["overall_take", "strengths", "weaknesses", "why_this_won", "model_jury_notes"].into_iter().map(str::to_string).collect(),
        inputs: BattleInputs {
            left: PortraitRef {
                id: left.id.clone(),
                label: left.label.clone(),
                source_type: left.source_type.clone(),
                width: left.width,
                height: left.height,
                hash: left.hash.clone(),
                image_data_url: left.image_data_url.clone(),
            },
            right: PortraitRef {
                id: right.id.clone(),
                label: right.label.clone(),
                source_type: right.source_type.clone(),
                width: right.width,
                height: right.height,
                hash: right.hash.clone(),
                image_data_url: right.image_data_url.clone(),
            },
        },
        scores: SideScores { left: left_scores, right: right_scores },
        dual_scores: None,
        axis_cards,
        winner,
        sections: final_sections,
        language: None,
    }
}

async fn judge_with_openai(left: &LoadedPortrait, right: &LoadedPortrait, model: &str, config: &OpenAiConfig, lang: Language) -> Result<OpenAiJudgeOutput> {
    let api_key = config.api_key.clone().or_else(|| std::env::var("BTY_OPENAI_API_KEY").ok()).or_else(|| std::env::var("OPENAI_API_KEY").ok()).ok_or_else(|| anyhow!("OpenAI judging requires OPENAI_API_KEY or BTY_OPENAI_API_KEY"))?;
    let base_url = config.base_url.clone().or_else(|| std::env::var("OPENAI_BASE_URL").ok()).unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "winner_id": { "type": "string", "enum": ["left", "right"] },
            "left_scores": {
                "type": "object",
                "additionalProperties": false,
                "properties": AXIS_DEFINITIONS.iter().map(|axis| (axis.key.to_string(), json!({"type":"number","minimum":0,"maximum":100}))).collect::<serde_json::Map<_, _>>(),
                "required": AXIS_DEFINITIONS.iter().map(|axis| axis.key).collect::<Vec<_>>()
            },
            "right_scores": {
                "type": "object",
                "additionalProperties": false,
                "properties": AXIS_DEFINITIONS.iter().map(|axis| (axis.key.to_string(), json!({"type":"number","minimum":0,"maximum":100}))).collect::<serde_json::Map<_, _>>(),
                "required": AXIS_DEFINITIONS.iter().map(|axis| axis.key).collect::<Vec<_>>()
            },
            "sections": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "overall_take": {"type":"string"},
                    "strengths_left": {"type":"string"},
                    "strengths_right": {"type":"string"},
                    "weaknesses_left": {"type":"string"},
                    "weaknesses_right": {"type":"string"},
                    "why_this_won": {"type":"string"},
                    "model_jury_notes": {"type":"string"}
                },
                "required": ["overall_take","strengths_left","strengths_right","weaknesses_left","weaknesses_right","why_this_won","model_jury_notes"]
            }
        },
        "required": ["winner_id","left_scores","right_scores","sections"]
    });

    let prompt = vlm_json_prompt(lang);

    let body = json!({
        "model": model,
        "input": [{
            "role": "user",
            "content": [
                {"type": "input_text", "text": prompt},
                {"type": "input_image", "image_url": left.image_data_url, "detail": "high"},
                {"type": "input_image", "image_url": right.image_data_url, "detail": "high"}
            ]
        }],
        "text": {
            "format": {
                "type": "json_schema",
                "name": "better_than_you_battle",
                "strict": true,
                "schema": schema
            }
        }
    });

    let client = Client::new();
    let response = client
        .post(format!("{}/responses", base_url.trim_end_matches('/')))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("OpenAI judge failed: HTTP {} {}", response.status(), response.text().await.unwrap_or_default());
    }

    let payload: Value = response.json().await?;
    let output_text = payload.get("output_text").and_then(Value::as_str).map(str::to_string).or_else(|| {
        payload.get("output")?.as_array()?.iter().flat_map(|item| item.get("content").and_then(Value::as_array).into_iter().flatten()).find_map(|content| content.get("text").and_then(Value::as_str)).map(str::to_string)
    }).ok_or_else(|| anyhow!("OpenAI judge returned no output text"))?;

    let parsed: Value = serde_json::from_str(&output_text)?;
    let parse_axes = |key: &str| -> AxisScores {
        let scores = parsed.get(key).and_then(Value::as_object).cloned().unwrap_or_default();
        let mut out = AxisScores::default();
        for axis in AXIS_DEFINITIONS.iter() {
            let v = round(scores.get(axis.key).and_then(Value::as_f64).unwrap_or(0.0) as f32);
            out.set(axis.key, v);
        }
        out
    };

    let sections = parsed.get("sections").and_then(Value::as_object).ok_or_else(|| anyhow!("OpenAI judge missing sections object"))?;
    Ok(OpenAiJudgeOutput {
        winner_id: parsed.get("winner_id").and_then(Value::as_str).unwrap_or("left").to_string(),
        left_scores: parse_axes("left_scores"),
        right_scores: parse_axes("right_scores"),
        sections: BattleSections {
            overall_take: sections.get("overall_take").and_then(Value::as_str).unwrap_or_default().to_string(),
            strengths: SideTexts {
                left: sections.get("strengths_left").and_then(Value::as_str).unwrap_or_default().to_string(),
                right: sections.get("strengths_right").and_then(Value::as_str).unwrap_or_default().to_string(),
            },
            weaknesses: SideTexts {
                left: sections.get("weaknesses_left").and_then(Value::as_str).unwrap_or_default().to_string(),
                right: sections.get("weaknesses_right").and_then(Value::as_str).unwrap_or_default().to_string(),
            },
            why_this_won: sections.get("why_this_won").and_then(Value::as_str).unwrap_or_default().to_string(),
            model_jury_notes: sections.get("model_jury_notes").and_then(Value::as_str).unwrap_or_default().to_string(),
        },
        provider: "openai".to_string(),
        model: model.to_string(),
    })
}

fn parse_data_url(data_url: &str) -> (String, String) {
    if let Some(comma_pos) = data_url.find(',') {
        let header = &data_url[..comma_pos];
        let base64_data = &data_url[comma_pos + 1..];
        let media_type = header
            .strip_prefix("data:")
            .and_then(|s| s.split(';').next())
            .unwrap_or("image/jpeg")
            .to_string();
        (media_type, base64_data.to_string())
    } else {
        ("image/jpeg".to_string(), data_url.to_string())
    }
}

fn parse_vlm_axes(parsed: &Value, key: &str) -> AxisScores {
    let scores = parsed.get(key).and_then(Value::as_object).cloned().unwrap_or_default();
    let mut out = AxisScores::default();
    for axis in AXIS_DEFINITIONS.iter() {
        let v = round(scores.get(axis.key).and_then(Value::as_f64).unwrap_or(0.0) as f32);
        out.set(axis.key, v);
    }
    out
}

fn parse_vlm_sections(parsed: &Value) -> Result<BattleSections> {
    let sections = parsed.get("sections").and_then(Value::as_object).ok_or_else(|| anyhow!("VLM judge missing sections object"))?;
    Ok(BattleSections {
        overall_take: sections.get("overall_take").and_then(Value::as_str).unwrap_or_default().to_string(),
        strengths: SideTexts {
            left: sections.get("strengths_left").and_then(Value::as_str).unwrap_or_default().to_string(),
            right: sections.get("strengths_right").and_then(Value::as_str).unwrap_or_default().to_string(),
        },
        weaknesses: SideTexts {
            left: sections.get("weaknesses_left").and_then(Value::as_str).unwrap_or_default().to_string(),
            right: sections.get("weaknesses_right").and_then(Value::as_str).unwrap_or_default().to_string(),
        },
        why_this_won: sections.get("why_this_won").and_then(Value::as_str).unwrap_or_default().to_string(),
        model_jury_notes: sections.get("model_jury_notes").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

fn vlm_json_prompt(lang: Language) -> String {
    let axis_schema: serde_json::Map<String, Value> = AXIS_DEFINITIONS
        .iter()
        .map(|axis| (axis.key.to_string(), json!(0)))
        .collect();

    let schema_str = serde_json::to_string_pretty(&json!({
        "winner_id": "left or right",
        "left_scores": axis_schema,
        "right_scores": axis_schema,
        "sections": {
            "overall_take": "",
            "strengths_left": "",
            "strengths_right": "",
            "weaknesses_left": "",
            "weaknesses_right": "",
            "why_this_won": "",
            "model_jury_notes": ""
        }
    })).unwrap_or_default();

    let axis_descriptions = AXIS_DEFINITIONS
        .iter()
        .map(|axis| format!("{}: {}", axis.key, axis.label))
        .collect::<Vec<_>>()
        .join(", ");

    let rubric = "Scoring rubric (0-100 per axis):\n\
        - facial_symmetry: bilateral symmetry and harmony of facial features\n\
        - facial_proportions: golden-ratio alignment, balanced feature placement (eye-nose-mouth-chin)\n\
        - skin_quality: smoothness, evenness, absence of blemishes, healthy complexion\n\
        - eye_expression: brightness, clarity, engagement, emotional presence of the eyes\n\
        - hair_grooming: style quality, condition, how it frames the face\n\
        - bone_structure: jawline definition, cheekbones, overall structural elegance\n\
        - expression_charisma: warmth, confidence, personality conveyed through expression\n\
        - lighting_color: photographic lighting quality, color harmony, skin tone rendering\n\
        - background_framing: composition, background choice, subject isolation, bokeh\n\
        - photogenic_impact: overall first-impression wow factor and memorability";

    let language_instruction = match lang {
        Language::Korean => "CRITICAL: Write ALL text fields (overall_take, strengths_left, strengths_right, weaknesses_left, weaknesses_right, why_this_won, model_jury_notes) in natural Korean (한국어). Use polite, descriptive Korean prose. Axis keys must remain in English.",
        Language::Japanese => "CRITICAL: Write ALL text fields (overall_take, strengths_left, strengths_right, weaknesses_left, weaknesses_right, why_this_won, model_jury_notes) in natural Japanese (日本語). Use polite, descriptive Japanese prose. Axis keys must remain in English.",
        Language::English => "Write all text fields in clear, descriptive English.",
    };

    format!(
        "You are BetterThanYou, a visual battle judge for AI-generated portrait photos. \
         Evaluate the visual qualities of both portraits strictly based on what's visible in the images. \
         Score each portrait on 10 axes from 0 to 100, then decide a winner.\n\n\
         Axes: {axes}\n\n\
         {rubric}\n\n\
         {lang_instr}\n\n\
         You MUST respond with ONLY a valid JSON object (no markdown, no explanation, no code fences) matching this schema:\n{schema}",
        axes = axis_descriptions,
        rubric = rubric,
        lang_instr = language_instruction,
        schema = schema_str,
    )
}

async fn judge_with_anthropic(left: &LoadedPortrait, right: &LoadedPortrait, model: &str, config: &OpenAiConfig, lang: Language) -> Result<OpenAiJudgeOutput> {
    let api_key = config.api_key.clone()
        .or_else(|| std::env::var("BTY_ANTHROPIC_API_KEY").ok())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| anyhow!("Anthropic judging requires ANTHROPIC_API_KEY or BTY_ANTHROPIC_API_KEY"))?;

    let (left_media_type, left_b64) = parse_data_url(&left.image_data_url);
    let (right_media_type, right_b64) = parse_data_url(&right.image_data_url);

    let prompt = vlm_json_prompt(lang);

    let body = json!({
        "model": model,
        "max_tokens": 4096,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image", "source": {"type": "base64", "media_type": left_media_type, "data": left_b64}},
                {"type": "image", "source": {"type": "base64", "media_type": right_media_type, "data": right_b64}}
            ]
        }]
    });

    let client = Client::new();
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("Anthropic judge failed: HTTP {} {}", response.status(), response.text().await.unwrap_or_default());
    }

    let payload: Value = response.json().await?;
    let output_text = payload
        .get("content")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Anthropic judge returned no output text"))?;

    let parsed: Value = serde_json::from_str(output_text)
        .with_context(|| format!("Failed to parse Anthropic JSON response: {}", &output_text[..output_text.len().min(200)]))?;

    Ok(OpenAiJudgeOutput {
        winner_id: parsed.get("winner_id").and_then(Value::as_str).unwrap_or("left").to_string(),
        left_scores: parse_vlm_axes(&parsed, "left_scores"),
        right_scores: parse_vlm_axes(&parsed, "right_scores"),
        sections: parse_vlm_sections(&parsed)?,
        provider: "anthropic".to_string(),
        model: model.to_string(),
    })
}

async fn judge_with_gemini(left: &LoadedPortrait, right: &LoadedPortrait, model: &str, config: &OpenAiConfig, lang: Language) -> Result<OpenAiJudgeOutput> {
    let api_key = config.api_key.clone()
        .or_else(|| std::env::var("BTY_GEMINI_API_KEY").ok())
        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
        .ok_or_else(|| anyhow!("Gemini judging requires GEMINI_API_KEY or BTY_GEMINI_API_KEY"))?;

    let (left_media_type, left_b64) = parse_data_url(&left.image_data_url);
    let (right_media_type, right_b64) = parse_data_url(&right.image_data_url);

    let prompt = vlm_json_prompt(lang);

    let body = json!({
        "contents": [{
            "parts": [
                {"text": prompt},
                {"inline_data": {"mime_type": left_media_type, "data": left_b64}},
                {"inline_data": {"mime_type": right_media_type, "data": right_b64}}
            ]
        }],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let client = Client::new();
    let response = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("Gemini judge failed: HTTP {} {}", response.status(), response.text().await.unwrap_or_default());
    }

    let payload: Value = response.json().await?;
    let output_text = payload
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .and_then(|parts| parts.first())
        .and_then(|part| part.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Gemini judge returned no output text"))?;

    let parsed: Value = serde_json::from_str(output_text)
        .with_context(|| format!("Failed to parse Gemini JSON response: {}", &output_text[..output_text.len().min(200)]))?;

    Ok(OpenAiJudgeOutput {
        winner_id: parsed.get("winner_id").and_then(Value::as_str).unwrap_or("left").to_string(),
        left_scores: parse_vlm_axes(&parsed, "left_scores"),
        right_scores: parse_vlm_axes(&parsed, "right_scores"),
        sections: parse_vlm_sections(&parsed)?,
        provider: "gemini".to_string(),
        model: model.to_string(),
    })
}

pub async fn analyze_portrait_battle_with_override(options: AnalyzeOptions, openai_override: Option<OpenAiJudgeOutput>) -> Result<BattleResult> {
    let axis_definitions = axis_definitions_with_overrides(&options.axis_weights)?;
    let left = load_portrait(&options.left_source, options.left_label.as_deref(), "left").await?;
    let right = load_portrait(&options.right_source, options.right_label.as_deref(), "right").await?;
    let lang = options.language;

    // ── Always run heuristic first (fast, deterministic) ────────────────
    let heuristic_left = score_portrait(&left, &axis_definitions);
    let heuristic_right = score_portrait(&right, &axis_definitions);
    let heuristic_side_scores = SideScores {
        left: heuristic_left.clone(),
        right: heuristic_right.clone(),
    };

    // ── Determine VLM provider ──────────────────────────────────────────
    let openai_key_present = options.openai_config.api_key.clone().or_else(|| std::env::var("BTY_OPENAI_API_KEY").ok()).or_else(|| std::env::var("OPENAI_API_KEY").ok()).is_some();
    let anthropic_key_present = std::env::var("BTY_ANTHROPIC_API_KEY").is_ok() || std::env::var("ANTHROPIC_API_KEY").is_ok();
    let gemini_key_present = std::env::var("BTY_GEMINI_API_KEY").is_ok() || std::env::var("GEMINI_API_KEY").is_ok();

    let vlm_mode = match options.judge_mode {
        JudgeMode::Openai => Some(JudgeMode::Openai),
        JudgeMode::Anthropic => Some(JudgeMode::Anthropic),
        JudgeMode::Gemini => Some(JudgeMode::Gemini),
        JudgeMode::Auto => {
            if openai_key_present { Some(JudgeMode::Openai) }
            else if anthropic_key_present { Some(JudgeMode::Anthropic) }
            else if gemini_key_present { Some(JudgeMode::Gemini) }
            else { None }
        }
        JudgeMode::Heuristic => None,
    };

    // ── Try VLM if available ────────────────────────────────────────────
    let mut vlm_output: Option<OpenAiJudgeOutput> = None;
    let mut vlm_error: Option<String> = None;
    let mut effective_mode = JudgeMode::Heuristic;
    let mut effective_provider = String::from("local");
    let mut effective_model: Option<String> = None;

    if let Some(mode) = vlm_mode {
        let judged = match mode {
            JudgeMode::Openai => {
                if let Some(override_result) = openai_override {
                    Ok(override_result)
                } else {
                    judge_with_openai(&left, &right, &options.openai_model, &options.openai_config, lang).await
                }
            }
            JudgeMode::Anthropic => {
                judge_with_anthropic(&left, &right, &options.openai_model, &options.openai_config, lang).await
            }
            JudgeMode::Gemini => {
                judge_with_gemini(&left, &right, &options.openai_model, &options.openai_config, lang).await
            }
            _ => unreachable!(),
        };

        match judged {
            Ok(vlm) => {
                effective_mode = mode;
                effective_provider = vlm.provider.clone();
                effective_model = Some(vlm.model.clone());
                vlm_output = Some(vlm);
            }
            Err(error) if matches!(options.judge_mode, JudgeMode::Auto) => {
                vlm_error = Some(error.to_string());
            }
            Err(error) => return Err(error),
        }
    } else if matches!(options.judge_mode, JudgeMode::Auto) {
        vlm_error = Some("No VLM API key detected. Using heuristic judge.".into());
    }

    // ── Build dual scores and select official combined scores ───────────
    let (final_left_scores, final_right_scores, dual_scores, sections, winner_hint): (ScoreBundle, ScoreBundle, Option<DualScores>, BattleSections, Option<String>) =
        if let Some(vlm) = vlm_output.as_ref() {
            // Build VLM side scores
            let vlm_left_bundle = ScoreBundle {
                axes: vlm.left_scores.clone(),
                total: round(compute_total_from_axes(&vlm.left_scores, &axis_definitions)),
                telemetry: None,
            };
            let vlm_right_bundle = ScoreBundle {
                axes: vlm.right_scores.clone(),
                total: round(compute_total_from_axes(&vlm.right_scores, &axis_definitions)),
                telemetry: None,
            };
            let vlm_side_scores = SideScores { left: vlm_left_bundle.clone(), right: vlm_right_bundle.clone() };

            // Combined = 30% heuristic + 70% VLM, preserving heuristic telemetry
            let combined_left_axes = heuristic_left.axes.blend(&vlm_left_bundle.axes, 0.30, 0.70);
            let combined_right_axes = heuristic_right.axes.blend(&vlm_right_bundle.axes, 0.30, 0.70);
            let combined_left = ScoreBundle {
                total: round(compute_total_from_axes(&combined_left_axes, &axis_definitions)),
                axes: combined_left_axes,
                telemetry: heuristic_left.telemetry.clone(),
            };
            let combined_right = ScoreBundle {
                total: round(compute_total_from_axes(&combined_right_axes, &axis_definitions)),
                axes: combined_right_axes,
                telemetry: heuristic_right.telemetry.clone(),
            };

            let dual = DualScores {
                heuristic: heuristic_side_scores.clone(),
                vlm: Some(vlm_side_scores),
            };
            (combined_left, combined_right, Some(dual), vlm.sections.clone(), Some(vlm.winner_id.clone()))
        } else {
            let sections = {
                let axis_cards_tmp = build_axis_cards(&heuristic_left, &heuristic_right);
                let winner_id_tmp = pick_winner(&left, &right, &heuristic_left, &heuristic_right, &axis_cards_tmp, None);
                let winner_tmp = Winner {
                    id: winner_id_tmp.clone(),
                    label: if winner_id_tmp == "left" { left.label.clone() } else { right.label.clone() },
                    total_score: if winner_id_tmp == "left" { heuristic_left.total } else { heuristic_right.total },
                    opponent_score: if winner_id_tmp == "left" { heuristic_right.total } else { heuristic_left.total },
                    margin: round((heuristic_left.total - heuristic_right.total).abs()),
                    decisive: (heuristic_left.total - heuristic_right.total).abs() >= 6.0,
                };
                build_battle_narrative(&left, &right, &heuristic_left, &heuristic_right, &winner_tmp, &axis_cards_tmp)
            };
            let dual = DualScores {
                heuristic: heuristic_side_scores.clone(),
                vlm: None,
            };
            (heuristic_left, heuristic_right, Some(dual), sections, None)
        };

    let mut result = build_result(
        &left,
        &right,
        final_left_scores,
        final_right_scores,
        sections,
        effective_mode,
        &effective_provider,
        effective_model,
        winner_hint.as_deref(),
        vlm_error,
    );
    result.dual_scores = dual_scores;
    result.language = Some(match lang {
        Language::English => "en".to_string(),
        Language::Korean => "ko".to_string(),
        Language::Japanese => "ja".to_string(),
    });
    Ok(result)
}

pub async fn analyze_portrait_battle(options: AnalyzeOptions) -> Result<BattleResult> {
    analyze_portrait_battle_with_override(options, None).await
}

/// Escape HTML-sensitive characters in dynamic text.
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn lang_from_code(code: Option<&str>) -> Language {
    match code {
        Some("ko") => Language::Korean,
        Some("ja") => Language::Japanese,
        _ => Language::English,
    }
}

/// Letter rank + CSS color class from a 0-100 score.
fn score_rank_html(score: f32) -> (&'static str, &'static str) {
    if score >= 95.0 { ("S+", "rank-splus") }
    else if score >= 90.0 { ("S", "rank-s") }
    else if score >= 80.0 { ("A", "rank-a") }
    else if score >= 70.0 { ("B", "rank-b") }
    else if score >= 60.0 { ("C", "rank-c") }
    else if score >= 50.0 { ("D", "rank-d") }
    else { ("F", "rank-f") }
}

/// One-line verdict summary based on margin size (localized).
fn verdict_phrase(lang: Language, margin: f32, winner_label: &str, loser_label: &str) -> String {
    let intensity = if margin >= 15.0 { "crushing" }
        else if margin >= 8.0 { "clear" }
        else if margin >= 4.0 { "controlled" }
        else { "narrow" };
    match (lang, intensity) {
        (Language::Korean, "crushing") => format!("{}이(가) {}을(를) 압도적으로 이겼습니다", winner_label, loser_label),
        (Language::Korean, "clear") => format!("{}이(가) {}보다 확실히 앞섰습니다", winner_label, loser_label),
        (Language::Korean, "controlled") => format!("{}이(가) 안정적으로 {}을(를) 이겼습니다", winner_label, loser_label),
        (Language::Korean, _) => format!("{}이(가) 근소한 차이로 {}을(를) 이겼습니다", winner_label, loser_label),
        (Language::Japanese, "crushing") => format!("{}が{}を圧倒的に打ち破った", winner_label, loser_label),
        (Language::Japanese, "clear") => format!("{}が{}を明確に上回った", winner_label, loser_label),
        (Language::Japanese, "controlled") => format!("{}が{}を着実に制した", winner_label, loser_label),
        (Language::Japanese, _) => format!("{}が{}をわずかに上回った", winner_label, loser_label),
        (_, "crushing") => format!("{} CRUSHED {}", winner_label.to_uppercase(), loser_label.to_uppercase()),
        (_, "clear") => format!("{} clearly outpaced {}", winner_label, loser_label),
        (_, "controlled") => format!("{} controlled the battle over {}", winner_label, loser_label),
        (_, _) => format!("{} narrowly edged out {}", winner_label, loser_label),
    }
}

pub fn render_html_report(result: &BattleResult) -> String {
    let lang = lang_from_code(result.language.as_deref());
    let lang_attr = match lang {
        Language::Korean => "ko",
        Language::Japanese => "ja",
        Language::English => "en",
    };

    let left_is_winner = result.winner.id == "left";
    let right_is_winner = result.winner.id == "right";

    // ── Mini-stats strip rendered ABOVE each photo ──────────────────────
    // Each side gets a 10-item mini grid with icon + short label + score.
    let build_mini_stats = |side: &str| -> String {
        let cells = result
            .axis_cards
            .iter()
            .map(|card| {
                let short = localized_axis_short(lang, &card.key);
                let icon = axis_icon(&card.key);
                let score = if side == "left" { card.left } else { card.right };
                let (rank, rank_class) = score_rank_html(score);
                let is_leader = card.leader == side;
                let lead_class = if is_leader { "mini-lead" } else { "" };
                format!(
                    r#"<div class="mini-stat {lead}"><span class="mini-icon">{icon}</span><span class="mini-name">{name}</span><span class="mini-score">{score:.0}</span><span class="mini-rank {rank_class}">{rank}</span></div>"#,
                    lead = lead_class,
                    icon = icon,
                    name = html_escape(&short),
                    score = score,
                    rank_class = rank_class,
                    rank = rank,
                )
            })
            .collect::<Vec<_>>()
            .join("");
        cells
    };
    let left_mini = build_mini_stats("left");
    let right_mini = build_mini_stats("right");

    // ── Detailed axis rows with icon, label, description, bars ──────────
    let axis_bars = result
        .axis_cards
        .iter()
        .map(|card| {
            let label = localized_axis_label(lang, &card.key);
            let desc = localized_axis_desc(lang, &card.key);
            let icon = axis_icon(&card.key);
            let left_pct = card.left.clamp(0.0, 100.0);
            let right_pct = card.right.clamp(0.0, 100.0);
            let (left_rank, left_rank_class) = score_rank_html(card.left);
            let (right_rank, right_rank_class) = score_rank_html(card.right);
            let left_class = if card.leader == "left" { "bar bar-lead" } else { "bar" };
            let right_class = if card.leader == "right" { "bar bar-lead" } else { "bar" };
            let gap_text = if card.leader == "tie" {
                "=".to_string()
            } else if card.leader == result.winner.id {
                format!("+{:.0}", card.diff)
            } else {
                format!("-{:.0}", card.diff)
            };
            let gap_class = if card.leader == "tie" {
                "gap gap-tie"
            } else if card.leader == result.winner.id {
                "gap gap-win"
            } else {
                "gap gap-lose"
            };
            format!(
                r#"<article class="axis-row"><div class="axis-meta"><div class="axis-head"><span class="axis-icon">{icon}</span><span class="axis-name">{label}</span></div><div class="axis-desc">{desc}</div></div><div class="axis-track"><div class="axis-side"><span class="axis-rank {lrc}">{lrank}</span><span class="axis-num">{left_num:.1}</span><div class="{lbar}" style="width:{left_pct}%"></div></div><div class="{gc}">{gap}</div><div class="axis-side axis-side-right"><div class="{rbar}" style="width:{right_pct}%"></div><span class="axis-num">{right_num:.1}</span><span class="axis-rank {rrc}">{rrank}</span></div></div></article>"#,
                icon = icon,
                label = html_escape(&label),
                desc = html_escape(&desc),
                lrank = left_rank, lrc = left_rank_class,
                rrank = right_rank, rrc = right_rank_class,
                left_num = card.left,
                right_num = card.right,
                left_pct = left_pct,
                right_pct = right_pct,
                lbar = left_class,
                rbar = right_class,
                gc = gap_class,
                gap = gap_text,
            )
        })
        .collect::<Vec<_>>()
        .join("");

    // ── Dual score dashboard ────────────────────────────────────────────
    let dual_dashboard = if let Some(dual) = result.dual_scores.as_ref() {
        let heuristic_block = format!(
            r#"<div class="dual-block"><div class="dual-header"><span class="dual-icon">⚙</span><span>{title}</span></div><div class="dual-scores"><div class="dual-score"><small>{left_label}</small><strong>{left_total:.1}</strong></div><div class="dual-vs">VS</div><div class="dual-score"><small>{right_label}</small><strong>{right_total:.1}</strong></div></div></div>"#,
            title = html_escape(t(lang, "report_heuristic")),
            left_label = html_escape(&result.inputs.left.label),
            right_label = html_escape(&result.inputs.right.label),
            left_total = dual.heuristic.left.total,
            right_total = dual.heuristic.right.total,
        );

        let vlm_block = if let Some(vlm) = dual.vlm.as_ref() {
            format!(
                r#"<div class="dual-block"><div class="dual-header"><span class="dual-icon">✦</span><span>{title}</span></div><div class="dual-scores"><div class="dual-score"><small>{left_label}</small><strong>{left_total:.1}</strong></div><div class="dual-vs">VS</div><div class="dual-score"><small>{right_label}</small><strong>{right_total:.1}</strong></div></div></div>"#,
                title = html_escape(t(lang, "report_vlm")),
                left_label = html_escape(&result.inputs.left.label),
                right_label = html_escape(&result.inputs.right.label),
                left_total = vlm.left.total,
                right_total = vlm.right.total,
            )
        } else {
            String::new()
        };

        format!(
            r#"<section class="dual-dashboard">{heuristic}{vlm}</section>"#,
            heuristic = heuristic_block,
            vlm = vlm_block,
        )
    } else {
        String::new()
    };

    // ── Per-portrait strengths/weaknesses cards ─────────────────────────
    let portrait_analysis = format!(
        r#"<section class="portrait-analysis"><h2 class="section-title">{title}</h2><div class="portrait-grid"><article class="portrait-card {left_winner_class}"><header class="portrait-header"><span class="side-tag tag-left">{left_tag}</span><h3>{left_label}</h3></header><div class="pa-block"><div class="pa-label">✅ {strengths}</div><p>{left_str}</p></div><div class="pa-block"><div class="pa-label">⚠ {weaknesses}</div><p>{left_weak}</p></div></article><article class="portrait-card {right_winner_class}"><header class="portrait-header"><span class="side-tag tag-right">{right_tag}</span><h3>{right_label}</h3></header><div class="pa-block"><div class="pa-label">✅ {strengths}</div><p>{right_str}</p></div><div class="pa-block"><div class="pa-label">⚠ {weaknesses}</div><p>{right_weak}</p></div></article></div></section>"#,
        title = html_escape(t(lang, "report_portrait_analysis")),
        left_winner_class = if left_is_winner { "is-winner" } else { "" },
        right_winner_class = if right_is_winner { "is-winner" } else { "" },
        left_tag = html_escape(t(lang, "report_left")),
        right_tag = html_escape(t(lang, "report_right")),
        left_label = html_escape(&result.inputs.left.label),
        right_label = html_escape(&result.inputs.right.label),
        strengths = html_escape(t(lang, "report_strengths")),
        weaknesses = html_escape(t(lang, "report_weaknesses")),
        left_str = html_escape(&result.sections.strengths.left),
        right_str = html_escape(&result.sections.strengths.right),
        left_weak = html_escape(&result.sections.weaknesses.left),
        right_weak = html_escape(&result.sections.weaknesses.right),
    );

    let decisive_badge = if result.winner.decisive {
        format!(r#"<span class="decisive-badge">{}</span>"#, html_escape(t(lang, "report_decisive")))
    } else {
        String::new()
    };

    let (winner_rank, winner_rank_class) = score_rank_html(result.winner.total_score);
    let (loser_rank, loser_rank_class) = score_rank_html(result.winner.opponent_score);
    let (left_rank_str, left_rank_class_str) = score_rank_html(result.scores.left.total);
    let (right_rank_str, right_rank_class_str) = score_rank_html(result.scores.right.total);
    let loser_label = if left_is_winner { &result.inputs.right.label } else { &result.inputs.left.label };
    let verdict = verdict_phrase(lang, result.winner.margin, &result.winner.label, loser_label);

    // ── Summary counts ──────────────────────────────────────────────────
    let left_wins_count = result.axis_cards.iter().filter(|c| c.leader == "left").count();
    let right_wins_count = result.axis_cards.iter().filter(|c| c.leader == "right").count();
    let tie_count = result.axis_cards.iter().filter(|c| c.leader == "tie").count();

    // ── Summary comparison table ────────────────────────────────────────
    let summary_table = {
        let rows = result.axis_cards.iter().map(|card| {
            let icon = axis_icon(&card.key);
            let label = localized_axis_label(lang, &card.key);
            let (lr, lrc) = score_rank_html(card.left);
            let (rr, rrc) = score_rank_html(card.right);
            let winner_cell = if card.leader == "tie" {
                format!(r#"<td class="sum-winner sum-tie">=</td>"#)
            } else if card.leader == "left" {
                format!(r#"<td class="sum-winner sum-left">◀</td>"#)
            } else {
                format!(r#"<td class="sum-winner sum-right">▶</td>"#)
            };
            let left_class = if card.leader == "left" { "sum-lead" } else { "" };
            let right_class = if card.leader == "right" { "sum-lead" } else { "" };
            format!(
                r#"<tr><td class="sum-axis"><span class="sum-icon">{icon}</span><span>{label}</span></td><td class="sum-num {lc}">{ln:.1} <span class="rank-mini {lrc}">{lr}</span></td>{wc}<td class="sum-num {rc}">{rn:.1} <span class="rank-mini {rrc}">{rr}</span></td><td class="sum-gap">{gap}</td></tr>"#,
                icon = icon,
                label = html_escape(&label),
                lc = left_class,
                ln = card.left,
                lr = lr, lrc = lrc,
                wc = winner_cell,
                rc = right_class,
                rn = card.right,
                rr = rr, rrc = rrc,
                gap = if card.leader == "tie" { "—".to_string() } else { format!("{:.1}", card.diff) },
            )
        }).collect::<Vec<_>>().join("");

        let left_label_esc = html_escape(&result.inputs.left.label);
        let right_label_esc = html_escape(&result.inputs.right.label);
        format!(
            r#"<section><h2 class="section-title">📊 {title}</h2><table class="summary-table"><thead><tr><th>Axis</th><th>{l} <small>{lw} wins</small></th><th></th><th>{r} <small>{rw} wins</small></th><th>Gap</th></tr></thead><tbody>{rows}</tbody></table><p class="summary-caption">{tie_count} ties • Margin: {margin:.1} points</p></section>"#,
            title = html_escape(t(lang, "report_ability_comparison")),
            l = left_label_esc,
            r = right_label_esc,
            lw = left_wins_count,
            rw = right_wins_count,
            rows = rows,
            tie_count = tie_count,
            margin = result.winner.margin,
        )
    };

    format!(
        r#"<!doctype html>
<html lang="{lang_attr}">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{product} ⚔ {winner_name} wins • {left} vs {right}</title>
    <style>
      :root {{
        --bg: #0a0d13;
        --panel: rgba(17, 23, 34, 0.88);
        --line: rgba(255, 255, 255, 0.09);
        --text: #f5efe4;
        --muted: #c7b9a5;
        --accent: #ff8f42;
        --accent-2: #63ebd3;
        --winner: #ffd36b;
        --left-color: #ff8f42;
        --right-color: #64b4ff;
        --green: #50ff78;
        --red: #ff4646;
        --rank-splus: #ffb3ff;
        --rank-s: #ffd36b;
        --rank-a: #50ff78;
        --rank-b: #63ebd3;
        --rank-c: #ff8f42;
        --rank-d: #ff8080;
        --rank-f: #666;
      }}
      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        min-height: 100vh;
        color: var(--text);
        font-family: "Avenir Next", "Trebuchet MS", "Segoe UI", "Apple SD Gothic Neo", "Noto Sans KR", "Noto Sans JP", sans-serif;
        background:
          radial-gradient(circle at top left, rgba(255,143,66,0.24), transparent 36%),
          radial-gradient(circle at right center, rgba(99,235,211,0.14), transparent 28%),
          linear-gradient(145deg, #090b10 0%, #121824 100%);
      }}
      .shell {{
        width: min(1240px, calc(100vw - 32px));
        margin: 0 auto;
        padding: 28px 0 56px;
      }}
      .hero, .axis-row, .narrative-block, .portrait-card, .dual-block {{
        border: 1px solid var(--line);
        border-radius: 20px;
        background: var(--panel);
        backdrop-filter: blur(14px);
      }}
      .hero {{ padding: 28px; box-shadow: 0 24px 70px rgba(0,0,0,0.35); margin-bottom: 22px; text-align: center; }}
      .eyebrow {{ text-transform: uppercase; letter-spacing: 0.18em; font-size: 12px; color: var(--muted); }}
      .winner-pill {{
        display: inline-flex;
        align-items: center;
        gap: 10px;
        padding: 10px 18px;
        border-radius: 999px;
        background: rgba(255, 211, 107, 0.14);
        color: var(--winner);
        margin: 12px 0 8px;
        font-weight: 600;
      }}
      .decisive-badge {{
        display: inline-block;
        padding: 4px 10px;
        border-radius: 999px;
        background: var(--red);
        color: #fff;
        font-size: 11px;
        font-weight: 700;
        letter-spacing: 0.12em;
        margin-left: 8px;
      }}
      h1 {{ margin: 6px 0 4px; font-size: clamp(36px, 6vw, 72px); line-height: 0.95; text-transform: uppercase; color: var(--winner); }}
      h2.section-title {{ margin: 26px 0 12px; font-size: 18px; text-transform: uppercase; letter-spacing: 0.12em; color: var(--text); display: flex; align-items: center; gap: 10px; }}
      h2.section-title::before {{ content: ""; width: 4px; height: 20px; background: var(--accent); border-radius: 2px; }}
      p {{ line-height: 1.7; color: var(--muted); margin: 0; }}
      .verdict {{ font-size: 15px; color: var(--muted); margin-top: 6px; }}
      .rank-badge-big {{ display: inline-block; padding: 4px 14px; border-radius: 999px; font-weight: 800; font-size: 18px; margin: 0 6px; }}
      .rank-splus {{ background: rgba(255,179,255,0.2); color: var(--rank-splus); border: 1px solid var(--rank-splus); }}
      .rank-s {{ background: rgba(255,211,107,0.2); color: var(--rank-s); border: 1px solid var(--rank-s); }}
      .rank-a {{ background: rgba(80,255,120,0.2); color: var(--rank-a); border: 1px solid var(--rank-a); }}
      .rank-b {{ background: rgba(99,235,211,0.2); color: var(--rank-b); border: 1px solid var(--rank-b); }}
      .rank-c {{ background: rgba(255,143,66,0.2); color: var(--rank-c); border: 1px solid var(--rank-c); }}
      .rank-d {{ background: rgba(255,128,128,0.2); color: var(--rank-d); border: 1px solid var(--rank-d); }}
      .rank-f {{ background: rgba(100,100,100,0.2); color: var(--rank-f); border: 1px solid var(--rank-f); }}

      /* ── Battle Card: photos with stats strip on top ───────── */
      .battle-card {{
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 12px;
        margin-bottom: 22px;
      }}
      .battle-side {{
        display: flex;
        flex-direction: column;
        border: 1px solid var(--line);
        border-radius: 20px;
        overflow: hidden;
        background: var(--panel);
        position: relative;
      }}
      .battle-side.is-winner {{
        border: 2px solid var(--winner);
        box-shadow: 0 0 30px rgba(255, 211, 107, 0.25);
      }}

      /* Name header above stats */
      .side-header {{
        padding: 14px 18px 10px;
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        background: linear-gradient(180deg, rgba(25,30,45,0.95) 0%, rgba(20,25,38,0.95) 100%);
      }}
      .side-name {{
        font-size: 18px;
        font-weight: 800;
        color: var(--text);
        word-break: break-all;
      }}
      .side-header .side-tag {{ flex-shrink: 0; }}
      .side-total {{
        display: flex;
        align-items: baseline;
        gap: 8px;
        padding: 0 18px 12px;
        background: linear-gradient(180deg, rgba(20,25,38,0.95) 0%, rgba(15,18,28,0.95) 100%);
      }}
      .side-total-label {{ font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.12em; }}
      .side-total-value {{ font-size: 44px; font-weight: 900; color: var(--winner); font-variant-numeric: tabular-nums; line-height: 1; }}
      .side-total-rank {{ margin-left: 6px; }}

      /* Mini-stats strip */
      .mini-stats {{
        display: grid;
        grid-template-columns: repeat(5, 1fr);
        gap: 3px;
        padding: 10px;
        background: rgba(15,18,28,0.95);
        border-top: 1px solid var(--line);
        border-bottom: 2px solid var(--accent);
      }}
      .mini-stat {{
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 2px;
        padding: 7px 3px;
        border-radius: 8px;
        background: rgba(255,255,255,0.03);
      }}
      .mini-stat.mini-lead {{ background: rgba(80,255,120,0.15); box-shadow: inset 0 0 0 1px rgba(80,255,120,0.4); }}
      .mini-icon {{ font-size: 16px; line-height: 1; }}
      .mini-name {{ font-size: 9px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; color: var(--muted); }}
      .mini-score {{ font-size: 15px; font-weight: 800; color: var(--text); font-variant-numeric: tabular-nums; }}
      .mini-rank {{ font-size: 9px; font-weight: 700; padding: 1px 5px; border-radius: 6px; }}

      /* Photo frame — use contain so full face is visible */
      .photo-wrap {{
        position: relative;
        background: #000;
        min-height: 400px;
        display: flex;
        align-items: center;
        justify-content: center;
      }}
      .photo-wrap img {{
        display: block;
        max-width: 100%;
        max-height: 580px;
        width: auto;
        height: auto;
        object-fit: contain;
      }}
      .crown {{
        position: absolute;
        top: 12px;
        right: 12px;
        font-size: 32px;
        filter: drop-shadow(0 2px 8px rgba(0,0,0,0.9));
        z-index: 2;
      }}
      .side-tag {{
        display: inline-block;
        padding: 5px 12px;
        border-radius: 999px;
        font-size: 11px;
        font-weight: 800;
        letter-spacing: 0.14em;
      }}
      .tag-left {{ background: var(--left-color); color: #000; }}
      .tag-right {{ background: var(--right-color); color: #000; }}

      /* ── Dual Score Dashboard ──────────────────────────────── */
      .dual-dashboard {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
        gap: 14px;
        margin-bottom: 22px;
      }}
      .dual-block {{ padding: 18px 22px; }}
      .dual-header {{ display: flex; align-items: center; gap: 10px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); margin-bottom: 12px; }}
      .dual-icon {{ color: var(--accent-2); font-size: 18px; }}
      .dual-scores {{ display: flex; align-items: center; justify-content: space-between; gap: 12px; }}
      .dual-score {{ text-align: center; flex: 1; }}
      .dual-score small {{ display: block; color: var(--muted); font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; margin-bottom: 6px; }}
      .dual-score strong {{ font-size: 32px; font-weight: 800; color: var(--text); }}
      .dual-vs {{ color: var(--accent); font-weight: 700; font-size: 14px; letter-spacing: 0.15em; }}

      /* ── Summary Comparison Table ───────────────────────────── */
      .summary-table {{
        width: 100%;
        border-collapse: collapse;
        border: 1px solid var(--line);
        border-radius: 16px;
        overflow: hidden;
        background: var(--panel);
      }}
      .summary-table th, .summary-table td {{ padding: 12px 14px; text-align: left; border-bottom: 1px solid var(--line); font-size: 14px; }}
      .summary-table tr:last-child td {{ border-bottom: none; }}
      .summary-table th {{
        background: rgba(25,30,45,0.95);
        color: var(--accent);
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.1em;
        font-weight: 700;
      }}
      .summary-table th small {{ display: block; color: var(--muted); font-size: 10px; font-weight: 500; margin-top: 2px; }}
      .sum-axis {{ display: flex; align-items: center; gap: 8px; font-weight: 600; }}
      .sum-icon {{ font-size: 16px; }}
      .sum-num {{ font-variant-numeric: tabular-nums; font-weight: 600; }}
      .sum-num.sum-lead {{ color: var(--green); }}
      .sum-winner {{ text-align: center; font-weight: 800; width: 40px; }}
      .sum-left {{ color: var(--left-color); }}
      .sum-right {{ color: var(--right-color); }}
      .sum-tie {{ color: var(--accent-2); }}
      .sum-gap {{ font-variant-numeric: tabular-nums; color: var(--muted); width: 60px; text-align: right; }}
      .rank-mini {{ display: inline-block; font-size: 9px; padding: 1px 5px; border-radius: 5px; margin-left: 4px; font-weight: 700; }}
      .summary-caption {{ margin-top: 10px; font-size: 13px; color: var(--muted); text-align: center; }}

      /* ── Detailed Axis Rows ────────────────────────────────── */
      .axis-row {{
        display: grid;
        grid-template-columns: 240px 1fr;
        gap: 18px;
        align-items: center;
        padding: 14px 20px;
        margin-bottom: 8px;
      }}
      .axis-meta {{ display: flex; flex-direction: column; gap: 3px; }}
      .axis-head {{ display: flex; align-items: center; gap: 8px; }}
      .axis-icon {{ font-size: 18px; }}
      .axis-name {{ font-weight: 700; font-size: 14px; color: var(--text); }}
      .axis-desc {{ font-size: 11px; color: var(--muted); letter-spacing: 0.02em; }}
      .axis-track {{ display: grid; grid-template-columns: 1fr 52px 1fr; align-items: center; gap: 10px; }}
      .axis-side {{ display: flex; align-items: center; gap: 6px; }}
      .axis-side:not(.axis-side-right) {{ justify-content: flex-end; }}
      .axis-num {{ font-variant-numeric: tabular-nums; font-size: 12px; color: var(--muted); min-width: 30px; text-align: right; }}
      .axis-side-right .axis-num {{ text-align: left; }}
      .axis-rank {{ font-size: 9px; font-weight: 700; padding: 1px 5px; border-radius: 5px; }}
      .bar {{
        height: 10px;
        border-radius: 5px;
        background: linear-gradient(90deg, rgba(255,143,66,0.5), rgba(255,143,66,0.85));
        min-width: 2%;
      }}
      .axis-side-right .bar {{ background: linear-gradient(90deg, rgba(100,180,255,0.85), rgba(100,180,255,0.5)); }}
      .bar-lead {{ background: linear-gradient(90deg, rgba(80,255,120,0.7), rgba(80,255,120,1)) !important; }}
      .gap {{
        text-align: center;
        font-weight: 700;
        font-size: 13px;
        font-variant-numeric: tabular-nums;
      }}
      .gap-win {{ color: var(--green); }}
      .gap-lose {{ color: var(--red); }}
      .gap-tie {{ color: var(--accent-2); }}

      /* ── Portrait Analysis Cards ──────────────────────────── */
      .portrait-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 16px; }}
      .portrait-card {{ padding: 22px; }}
      .portrait-card.is-winner {{ border-color: var(--winner); box-shadow: 0 0 0 1px var(--winner); }}
      .portrait-header {{ display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }}
      .portrait-header h3 {{ margin: 0; font-size: 22px; }}
      .pa-block {{ margin-bottom: 14px; }}
      .pa-label {{ font-size: 11px; text-transform: uppercase; letter-spacing: 0.14em; color: var(--muted); margin-bottom: 4px; font-weight: 700; }}
      .pa-block p {{ color: var(--text); font-size: 14px; }}

      /* ── Narrative Blocks ──────────────────────────────────── */
      .narrative-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 16px; margin-top: 12px; }}
      .narrative-block {{ padding: 20px; }}
      .narrative-block h3 {{ margin: 0 0 10px; font-size: 15px; text-transform: uppercase; letter-spacing: 0.1em; color: var(--accent); }}
      .narrative-block p {{ color: var(--text); font-size: 14px; }}

      footer {{ margin-top: 30px; color: var(--muted); font-size: 13px; text-align: center; }}

      @media (max-width: 720px) {{
        .battle-card {{ grid-template-columns: 1fr; }}
        .axis-row {{ grid-template-columns: 1fr; }}
        .mini-stats {{ grid-template-columns: repeat(5, 1fr); }}
      }}

      /* ── Street Fighter overlay — aggressive arcade aesthetic ───────── */
      @import url("https://fonts.googleapis.com/css2?family=Bebas+Neue&family=Orbitron:wght@600;700;900&display=swap");
      body {{
        background:
          radial-gradient(circle at 18% 26%, rgba(255, 70, 70, 0.32), transparent 38%),
          radial-gradient(circle at 82% 74%, rgba(64, 158, 255, 0.32), transparent 38%),
          repeating-linear-gradient(
            0deg,
            rgba(255,255,255,0.025) 0px,
            rgba(255,255,255,0.025) 1px,
            transparent 1px,
            transparent 4px
          ),
          linear-gradient(145deg, #060810 0%, #0e1320 100%);
      }}
      h1, .section-title, .side-name, .side-total-value, .dual-score strong, .winner-name {{
        font-family: "Orbitron", "Bebas Neue", "Avenir Next", sans-serif !important;
        letter-spacing: 0.04em !important;
      }}
      .hero h1 {{
        font-family: "Bebas Neue", "Orbitron", sans-serif !important;
        font-size: clamp(56px, 9vw, 110px) !important;
        letter-spacing: 0.1em !important;
        text-transform: uppercase;
        background: linear-gradient(180deg, #ffe9a8 0%, var(--winner) 50%, #ff8a3c 100%);
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        text-shadow: 0 0 28px rgba(255, 211, 107, 0.45);
        animation: sf-pulse 1.6s ease-in-out infinite alternate;
      }}
      @keyframes sf-pulse {{
        from {{ filter: drop-shadow(0 0 8px rgba(255, 211, 107, 0.4)); }}
        to {{ filter: drop-shadow(0 0 22px rgba(255, 211, 107, 0.85)); }}
      }}
      .verdict {{
        font-family: "Bebas Neue", "Orbitron", sans-serif !important;
        font-size: clamp(20px, 2.4vw, 28px) !important;
        letter-spacing: 0.18em !important;
        text-transform: uppercase;
        color: #ff5252 !important;
      }}
      .winner-pill {{
        background: linear-gradient(90deg, rgba(255,70,70,0.25), rgba(255,211,107,0.35), rgba(64,158,255,0.25)) !important;
        border: 1px solid rgba(255, 211, 107, 0.6);
        text-transform: uppercase;
        letter-spacing: 0.16em;
        font-family: "Orbitron", sans-serif;
        font-weight: 700 !important;
        box-shadow: 0 0 30px rgba(255, 211, 107, 0.25);
      }}
      .battle-card {{ position: relative; }}
      .battle-card::before {{
        content: "VS";
        position: absolute;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        font-family: "Bebas Neue", "Orbitron", sans-serif;
        font-size: clamp(60px, 12vw, 160px);
        font-weight: 900;
        letter-spacing: 0.1em;
        background: linear-gradient(180deg, #ffd36b 0%, #ff4646 100%);
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        text-shadow: 0 0 30px rgba(255, 70, 70, 0.5);
        pointer-events: none;
        z-index: 5;
        opacity: 0.9;
        animation: sf-vs-flicker 2s ease-in-out infinite;
      }}
      @keyframes sf-vs-flicker {{
        0%, 100% {{ opacity: 0.92; transform: translate(-50%, -50%) scale(1); }}
        50% {{ opacity: 0.65; transform: translate(-50%, -50%) scale(1.04); }}
      }}
      .battle-side[class*="left"], .battle-side:first-child {{
        border-left: 6px solid var(--red) !important;
        box-shadow: -8px 0 32px rgba(255, 70, 70, 0.18);
      }}
      .battle-side:last-child {{
        border-right: 6px solid #4a9eff !important;
        box-shadow: 8px 0 32px rgba(64, 158, 255, 0.18);
      }}
      .battle-side.is-winner {{
        border-color: var(--winner) !important;
        box-shadow: 0 0 60px rgba(255, 211, 107, 0.4) !important;
      }}
      .side-name {{
        text-transform: uppercase;
        letter-spacing: 0.16em;
        font-weight: 900 !important;
      }}
      .side-total-value {{
        font-family: "Bebas Neue", "Orbitron", sans-serif !important;
        font-size: clamp(54px, 6vw, 76px) !important;
        letter-spacing: 0.05em !important;
        text-shadow: 0 0 18px currentColor;
      }}
      /* Health-bar style score row above each portrait */
      .side-total {{ position: relative; }}
      .side-total::before {{
        content: "";
        position: absolute;
        bottom: 0;
        left: 0;
        height: 4px;
        width: 100%;
        background: linear-gradient(90deg, #ff4646, #ffd36b, #50ff78);
        opacity: 0.85;
        box-shadow: 0 0 12px rgba(255, 211, 107, 0.6);
      }}
      .rank-badge-big {{
        font-family: "Orbitron", sans-serif !important;
        font-weight: 900 !important;
        letter-spacing: 0.08em !important;
        text-shadow: 0 0 12px currentColor;
      }}
      .section-title {{
        font-family: "Bebas Neue", "Orbitron", sans-serif !important;
        font-size: 26px !important;
        letter-spacing: 0.18em !important;
        color: var(--accent) !important;
      }}
      .section-title::before {{ content: "▸ "; color: var(--red); }}
    </style>
  </head>
  <body>
    <main class="shell">
      <section class="hero">
        <div class="eyebrow">{product} • {judge}</div>
        <div class="winner-pill">🏆 {winner_label_text} • {winner_name}{decisive}</div>
        <h1>{winner_name} <span class="rank-badge-big {wrc}">{wrank}</span></h1>
        <p class="verdict">{verdict}</p>
        <p style="margin-top:10px;">{winner_name} <strong style="color:var(--winner);">{winner_total:.1}</strong> vs {loser_label} <strong style="color:var(--muted);">{loser_total:.1}</strong> <span class="rank-badge-big {lrc}">{lrank}</span></p>
      </section>

      <section class="battle-card">
        <div class="battle-side {left_winner_class}">
          <div class="side-header">
            <span class="side-name">{left}</span>
            <span class="side-tag tag-left">◀ {left_tag}</span>
          </div>
          <div class="side-total">
            <span class="side-total-label">{score_label}</span>
            <span class="side-total-value">{left_total:.1}</span>
            <span class="rank-badge-big {left_rank_class} side-total-rank">{left_rank}</span>
          </div>
          <div class="mini-stats">{left_mini}</div>
          <div class="photo-wrap">
            {left_crown}
            <img alt="{left}" src="{left_src}" />
          </div>
        </div>
        <div class="battle-side {right_winner_class}">
          <div class="side-header">
            <span class="side-name">{right}</span>
            <span class="side-tag tag-right">{right_tag} ▶</span>
          </div>
          <div class="side-total">
            <span class="side-total-label">{score_label}</span>
            <span class="side-total-value">{right_total:.1}</span>
            <span class="rank-badge-big {right_rank_class} side-total-rank">{right_rank}</span>
          </div>
          <div class="mini-stats">{right_mini}</div>
          <div class="photo-wrap">
            {right_crown}
            <img alt="{right}" src="{right_src}" />
          </div>
        </div>
      </section>

      {dual_dashboard}

      {summary_table}

      <section>
        <h2 class="section-title">📐 {detail_title}</h2>
        <div class="axis-list">{axis_bars}</div>
      </section>

      {portrait_analysis}

      <section>
        <h2 class="section-title">{analysis_title}</h2>
        <div class="narrative-grid">
          <section class="narrative-block"><h3>💬 {overall_take_title}</h3><p>{overall}</p></section>
          <section class="narrative-block"><h3>🏆 {why_title}</h3><p>{why}</p></section>
          <section class="narrative-block"><h3>📝 {jury_title}</h3><p>{notes}</p></section>
        </div>
      </section>

      <footer>{generated_label} {created_at} • {product}</footer>
    </main>
  </body>
</html>"#,
        lang_attr = lang_attr,
        product = PRODUCT_NAME,
        left = html_escape(&result.inputs.left.label),
        right = html_escape(&result.inputs.right.label),
        winner_name = html_escape(&result.winner.label),
        loser_label = html_escape(loser_label),
        winner_total = result.winner.total_score,
        loser_total = result.winner.opponent_score,
        wrank = winner_rank, wrc = winner_rank_class,
        lrank = loser_rank, lrc = loser_rank_class,
        verdict = html_escape(&verdict),
        winner_label_text = html_escape(t(lang, "report_winner")),
        score_label = html_escape(t(lang, "report_score")),
        judge = html_escape(&result.engine.model.clone().unwrap_or_else(|| result.engine.judge_mode.clone())),
        overall = html_escape(&result.sections.overall_take),
        why = html_escape(&result.sections.why_this_won),
        notes = html_escape(&result.sections.model_jury_notes),
        left_total = result.scores.left.total,
        right_total = result.scores.right.total,
        left_src = result.inputs.left.image_data_url,
        right_src = result.inputs.right.image_data_url,
        left_tag = html_escape(t(lang, "report_left")),
        right_tag = html_escape(t(lang, "report_right")),
        left_mini = left_mini,
        right_mini = right_mini,
        left_rank = left_rank_str,
        left_rank_class = left_rank_class_str,
        right_rank = right_rank_str,
        right_rank_class = right_rank_class_str,
        left_winner_class = if left_is_winner { "is-winner" } else { "" },
        right_winner_class = if right_is_winner { "is-winner" } else { "" },
        left_crown = if left_is_winner { r#"<div class="crown">👑</div>"# } else { "" },
        right_crown = if right_is_winner { r#"<div class="crown">👑</div>"# } else { "" },
        decisive = decisive_badge,
        dual_dashboard = dual_dashboard,
        summary_table = summary_table,
        detail_title = html_escape(t(lang, "report_ability_comparison")),
        analysis_title = html_escape(t(lang, "report_jury_notes")),
        overall_take_title = html_escape(t(lang, "report_overall_take")),
        why_title = html_escape(t(lang, "report_why_won")),
        jury_title = html_escape(t(lang, "report_jury_notes")),
        generated_label = html_escape(t(lang, "report_generated")),
        created_at = result.created_at,
        axis_bars = axis_bars,
        portrait_analysis = portrait_analysis,
    )
}

pub fn default_reports_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reports")
}

pub fn save_battle_artifacts(result: &BattleResult, output_dir: &Path) -> Result<SavedArtifacts> {
    fs::create_dir_all(output_dir)?;
    // Filename encodes the winner side so it's obvious at a glance.
    // Format: {timestamp}-{winner-side}-wins-{left}-vs-{right}
    let winner_side = &result.winner.id; // "left" or "right"
    let stem = format!(
        "{}-{}-wins-{}",
        Utc::now().format("%Y-%m-%dt%H-%M-%S-%3fz"),
        winner_side,
        slugify(&format!("{}-vs-{}", result.inputs.left.label, result.inputs.right.label)),
    );
    let html_path = output_dir.join(format!("{}.html", stem));
    let json_path = output_dir.join(format!("{}.json", stem));
    let latest_html = output_dir.join("latest-battle.html");
    let latest_json = output_dir.join("latest-battle.json");

    fs::write(&html_path, render_html_report(result))?;
    fs::write(&json_path, serde_json::to_string_pretty(result)?)?;
    fs::copy(&html_path, &latest_html)?;
    fs::copy(&json_path, &latest_json)?;

    Ok(SavedArtifacts {
        html_path: html_path.display().to_string(),
        json_path: json_path.display().to_string(),
        latest_html_path: latest_html.display().to_string(),
        latest_json_path: latest_json.display().to_string(),
    })
}

pub fn regenerate_battle_report(battle_json_path: &Path, output_dir: &Path) -> Result<SavedArtifacts> {
    let result: BattleResult = serde_json::from_slice(&fs::read(battle_json_path)?)?;
    save_battle_artifacts(&result, output_dir)
}

pub fn open_path(path: &Path) -> Result<()> {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    Command::new(opener).arg(path).status()?;
    Ok(())
}

pub fn write_clipboard_text(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text.to_string())?;
    Ok(())
}

pub fn read_clipboard_text() -> Result<String> {
    let mut clipboard = arboard::Clipboard::new()?;
    Ok(clipboard.get_text()?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareAsset {
    pub platform: String,
    pub image_path: String,
    pub caption: String,
    pub open_url: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareBundle {
    pub directory: String,
    pub assets: Vec<ShareAsset>,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialShareLink {
    pub platform: String,
    pub share_url: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedShareBundle {
    pub share_page_url: String,
    pub report_url: String,
    pub preview_image_url: String,
    pub provider: String,
    pub qr_ascii: String,
    pub caption: String,
    pub social_links: Vec<SocialShareLink>,
}

fn draw_block(image: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let max_x = (x + w).min(image.width());
    let max_y = (y + h).min(image.height());
    for yy in y..max_y {
        for xx in x..max_x {
            image.put_pixel(xx, yy, color);
        }
    }
}

fn draw_text_8x8(image: &mut RgbaImage, x: u32, y: u32, text: &str, color: Rgba<u8>, scale: u32) {
    let mut cursor_x = x;
    for ch in text.chars() {
        let Some(glyph) = BASIC_FONTS.get(ch) else {
            cursor_x += 8 * scale;
            continue;
        };
        for (row_idx, row) in glyph.iter().enumerate() {
            for col_idx in 0..8u32 {
                if (row >> col_idx) & 1 == 1 {
                    let px = cursor_x + (7 - col_idx) * scale;
                    let py = y + row_idx as u32 * scale;
                    draw_block(image, px, py, scale, scale, color);
                }
            }
        }
        cursor_x += 8 * scale;
    }
}

fn share_caption(result: &BattleResult, platform: &str) -> String {
    // Pick top-3 axes the winner led in for a juicy caption
    let mut lead_axes: Vec<&AxisCard> = result.axis_cards.iter().filter(|c| c.leader == result.winner.id).collect();
    lead_axes.sort_by(|a, b| b.diff.partial_cmp(&a.diff).unwrap_or(std::cmp::Ordering::Equal));
    let top_axes_text = lead_axes
        .iter()
        .take(3)
        .map(|c| format!("{} {}", axis_icon(&c.key), c.label))
        .collect::<Vec<_>>()
        .join(", ");

    let (rank, _) = score_rank_html(result.winner.total_score);
    let decisive_suffix = if result.winner.decisive { " 🔥 DECISIVE WIN" } else { "" };

    let core = format!(
        "🏆 {} wins the BetterThanYou portrait battle!\n\n⚔️ {} ({:.1}) vs {} ({:.1})\n📊 Margin: +{:.1} pts · Rank {}{}\n✨ Top strengths: {}",
        result.winner.label,
        result.inputs.left.label,
        result.scores.left.total,
        result.inputs.right.label,
        result.scores.right.total,
        result.winner.margin,
        rank,
        decisive_suffix,
        if top_axes_text.is_empty() { "balanced across the board".into() } else { top_axes_text },
    );

    match platform {
        "x" => format!("{}\n\n#BetterThanYou #AIPortraits #PortraitBattle", core),
        "linkedin" => format!("{}\n\nGenerated with BetterThanYou — CLI portrait battle tool. #AI #Portraits", core),
        "instagram_post" => format!("{}\n\n#BetterThanYou #PortraitBattle #AIPortraits #AIArt", core),
        "instagram_story" => format!("{}\n\n#BetterThanYou", core),
        "tiktok" => format!("{}\n\n#BetterThanYou #AIPortraits #FYP", core),
        "pinterest" => format!("{}\n\nBetterThanYou portrait battle.", core),
        _ => core,
    }
}

fn share_url(platform: &str, caption: &str) -> Option<String> {
    let encoded = urlencoding::encode(caption);
    match platform {
        "x" => Some(format!("https://twitter.com/intent/tweet?text={}", encoded)),
        "linkedin" => Some("https://www.linkedin.com/feed/".to_string()),
        "instagram_post" => Some("https://www.instagram.com/".to_string()),
        "instagram_story" => Some("https://www.instagram.com/".to_string()),
        "tiktok" => Some("https://www.tiktok.com/upload".to_string()),
        "pinterest" => Some("https://www.pinterest.com/pin-builder/".to_string()),
        _ => None,
    }
}

fn share_note(platform: &str) -> &'static str {
    match platform {
        "x" => "Publishes a public share page first, then opens a prefilled X composer when possible.",
        "linkedin" => "Publishes a public share page first, then opens a LinkedIn share link.",
        "instagram_post" => "Publishes a public image URL and copies the caption. Finish the upload in Instagram.",
        "instagram_story" => "Publishes a public image URL and copies the caption. Finish the upload in Instagram Story.",
        "tiktok" => "Publishes a public image URL and copies the caption. Finish the upload in TikTok.",
        "pinterest" => "Publishes a public share page and image, then opens Pinterest pin creation when possible.",
        _ => "Generated social share asset.",
    }
}

pub fn share_clipboard_text(platform: &str, caption: &str, share_page_url: &str, preview_image_url: &str, report_url: &str) -> String {
    match platform {
        "x" | "linkedin" | "pinterest" => format!("{caption}\n\n{share_page_url}"),
        "instagram_post" | "instagram_story" | "tiktok" => {
            format!("{caption}\n\nPublic image: {preview_image_url}\nBattle report: {report_url}")
        }
        _ => format!("{caption}\n\n{share_page_url}"),
    }
}

fn build_social_share_links(page_url: &str, preview_image_url: &str, caption: &str) -> Vec<SocialShareLink> {
    let page = urlencoding::encode(page_url);
    let media = urlencoding::encode(preview_image_url);
    let description = urlencoding::encode(caption);
    let text = urlencoding::encode(caption);

    vec![
        SocialShareLink {
            platform: "x".to_string(),
            share_url: Some(format!("https://twitter.com/intent/tweet?text={text}&url={page}")),
            note: "Opens a prefilled X post with the public BetterThanYou share page.".to_string(),
        },
        SocialShareLink {
            platform: "linkedin".to_string(),
            share_url: Some(format!("https://www.linkedin.com/sharing/share-offsite/?url={page}")),
            note: "Opens the LinkedIn offsite share flow for the public BetterThanYou page.".to_string(),
        },
        SocialShareLink {
            platform: "instagram_post".to_string(),
            share_url: None,
            note: "Instagram web does not support fully automatic external posting. Use the published image URL and copied caption.".to_string(),
        },
        SocialShareLink {
            platform: "instagram_story".to_string(),
            share_url: None,
            note: "Instagram Story web posting is not exposed as a public share intent. Use the published image URL and copied caption.".to_string(),
        },
        SocialShareLink {
            platform: "tiktok".to_string(),
            share_url: None,
            note: "TikTok web upload cannot be prefilled from a public share intent. Use the published image URL and copied caption.".to_string(),
        },
        SocialShareLink {
            platform: "pinterest".to_string(),
            share_url: Some(format!(
                "https://www.pinterest.com/pin/create/button/?url={page}&media={media}&description={description}"
            )),
            note: "Opens Pinterest pin creation with the public preview image and share page.".to_string(),
        },
    ]
}

fn platform_dimensions(platform: &str) -> (u32, u32) {
    match platform {
        "x" => (1600, 900),
        "linkedin" => (1200, 627),
        "instagram_post" => (1080, 1350),
        "instagram_story" => (1080, 1920),
        "tiktok" => (1080, 1920),
        "pinterest" => (1000, 1500),
        _ => (1200, 900),
    }
}

/// Decode embedded base64 image into a fitted RgbaImage that preserves aspect ratio.
fn decode_portrait_for_share(data_url: &str, max_w: u32, max_h: u32) -> RgbaImage {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_url.split(',').nth(1).unwrap_or(""))
        .unwrap_or_default();
    let img = image::load_from_memory(&bytes).unwrap_or_else(|_| DynamicImage::new_rgba8(64, 64));
    img.resize(max_w, max_h, imageops::FilterType::Triangle).to_rgba8()
}

fn rank_color_rgba(score: f32) -> Rgba<u8> {
    if score >= 95.0 { Rgba([255, 179, 255, 255]) }
    else if score >= 90.0 { Rgba([255, 211, 107, 255]) }
    else if score >= 80.0 { Rgba([80, 255, 120, 255]) }
    else if score >= 70.0 { Rgba([99, 235, 211, 255]) }
    else if score >= 60.0 { Rgba([255, 143, 66, 255]) }
    else if score >= 50.0 { Rgba([255, 128, 128, 255]) }
    else { Rgba([150, 150, 150, 255]) }
}

fn rank_letter(score: f32) -> &'static str {
    if score >= 95.0 { "S+" }
    else if score >= 90.0 { "S" }
    else if score >= 80.0 { "A" }
    else if score >= 70.0 { "B" }
    else if score >= 60.0 { "C" }
    else if score >= 50.0 { "D" }
    else { "F" }
}

fn render_share_image(result: &BattleResult, platform: &str) -> RgbaImage {
    let (width, height) = platform_dimensions(platform);

    // ── Background gradient ──────────────────────────────────────────
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([10, 13, 20, 255]));
    for y in 0..height {
        let t = y as f32 / height as f32;
        let r = (10.0 + 14.0 * (1.0 - t)) as u8;
        let g = (13.0 + 12.0 * (1.0 - t)) as u8;
        let b = (20.0 + 24.0 * (1.0 - t)) as u8;
        for x in 0..width {
            canvas.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }

    // Top accent bar
    draw_block(&mut canvas, 0, 0, width, 14, Rgba([255, 140, 66, 255]));
    // Bottom accent bar
    draw_block(&mut canvas, 0, height.saturating_sub(8), width, 8, Rgba([99, 235, 211, 255]));

    // ── Header ───────────────────────────────────────────────────────
    let header_y = 36u32;
    draw_text_8x8(&mut canvas, 28, header_y, "BETTERTHANYOU", Rgba([255, 255, 255, 255]), 3);
    draw_text_8x8(&mut canvas, 28, header_y + 36, "PORTRAIT BATTLE", Rgba([200, 200, 220, 255]), 1);

    // Winner badge
    let winner_text = format!("WINNER: {}", result.winner.label.to_uppercase());
    draw_text_8x8(&mut canvas, 28, header_y + 60, &winner_text, Rgba([255, 207, 90, 255]), 2);
    let rank = rank_letter(result.winner.total_score);
    let rank_col = rank_color_rgba(result.winner.total_score);
    draw_text_8x8(&mut canvas, 28, header_y + 88, &format!("RANK {}", rank), rank_col, 2);

    // ── Photos: side-by-side, fitted to preserve aspect ratio ───────
    let photo_area_y: u32 = header_y + 130;
    let photo_max_w = (width / 2).saturating_sub(40);
    let photo_max_h = if height >= 1500 { 600 } else { (height as f32 * 0.35) as u32 };

    let left_img = decode_portrait_for_share(&result.inputs.left.image_data_url, photo_max_w, photo_max_h);
    let right_img = decode_portrait_for_share(&result.inputs.right.image_data_url, photo_max_w, photo_max_h);

    let left_x = ((width / 4) as i32 - (left_img.width() / 2) as i32).max(20) as i64;
    let right_x = ((width as i32 * 3 / 4) - (right_img.width() / 2) as i32).max((width / 2 + 20) as i32) as i64;

    // Draw black frames behind photos
    draw_block(&mut canvas, left_x as u32, photo_area_y, left_img.width() + 8, left_img.height() + 8, Rgba([0, 0, 0, 255]));
    draw_block(&mut canvas, right_x as u32, photo_area_y, right_img.width() + 8, right_img.height() + 8, Rgba([0, 0, 0, 255]));

    imageops::overlay(&mut canvas, &left_img, left_x + 4, photo_area_y as i64 + 4);
    imageops::overlay(&mut canvas, &right_img, right_x + 4, photo_area_y as i64 + 4);

    // Winner glow border (gold rectangle)
    let winner_x = if result.winner.id == "left" { left_x } else { right_x };
    let winner_w = if result.winner.id == "left" { left_img.width() + 8 } else { right_img.width() + 8 };
    let winner_h = if result.winner.id == "left" { left_img.height() + 8 } else { right_img.height() + 8 };
    let gold = Rgba([255, 211, 107, 255]);
    let bw = 4u32;
    draw_block(&mut canvas, winner_x as u32, photo_area_y, winner_w, bw, gold); // top
    draw_block(&mut canvas, winner_x as u32, photo_area_y + winner_h - bw, winner_w, bw, gold); // bottom
    draw_block(&mut canvas, winner_x as u32, photo_area_y, bw, winner_h, gold); // left
    draw_block(&mut canvas, winner_x as u32 + winner_w - bw, photo_area_y, bw, winner_h, gold); // right

    // Photo labels (filename) below each photo
    let label_y = photo_area_y + photo_max_h + 16;
    draw_text_8x8(&mut canvas, left_x as u32, label_y, &result.inputs.left.label.to_uppercase(), Rgba([255, 143, 66, 255]), 2);
    draw_text_8x8(&mut canvas, left_x as u32, label_y + 24, &format!("{:.1}  {}", result.scores.left.total, rank_letter(result.scores.left.total)), Rgba([255, 255, 255, 255]), 3);
    draw_text_8x8(&mut canvas, right_x as u32, label_y, &result.inputs.right.label.to_uppercase(), Rgba([100, 180, 255, 255]), 2);
    draw_text_8x8(&mut canvas, right_x as u32, label_y + 24, &format!("{:.1}  {}", result.scores.right.total, rank_letter(result.scores.right.total)), Rgba([255, 255, 255, 255]), 3);

    // ── Axis bars (only for tall formats) ────────────────────────────
    let bar_y_start = label_y + 80;
    let space_for_bars = height.saturating_sub(bar_y_start + 80);
    let row_spacing = if space_for_bars >= 480 { 44u32 } else { 36u32 };
    let needed = result.axis_cards.len() as u32 * row_spacing;

    if space_for_bars >= needed.min(300) {
        for (i, card) in result.axis_cards.iter().enumerate() {
            let y = bar_y_start + (i as u32) * row_spacing;
            if y + row_spacing > height - 60 { break; }

            // Axis label
            let short = localized_axis_short(Language::English, &card.key);
            draw_text_8x8(&mut canvas, 32, y, &short.to_uppercase(), Rgba([255, 214, 107, 255]), 2);

            // Bar geometry
            let bar_left_x: u32 = 140;
            let bar_right_x: u32 = width / 2 + 30;
            let bar_max_w: u32 = (width / 2).saturating_sub(180);
            let bar_h: u32 = 18;
            let by = y + 6;

            // Background tracks
            draw_block(&mut canvas, bar_left_x, by, bar_max_w, bar_h, Rgba([35, 40, 55, 255]));
            draw_block(&mut canvas, bar_right_x, by, bar_max_w, bar_h, Rgba([35, 40, 55, 255]));

            // Filled bars
            let lw = ((card.left / 100.0).clamp(0.0, 1.0) * bar_max_w as f32) as u32;
            let rw_w = ((card.right / 100.0).clamp(0.0, 1.0) * bar_max_w as f32) as u32;
            let l_color = if card.leader == "left" { Rgba([80, 255, 120, 255]) } else { Rgba([255, 143, 66, 255]) };
            let r_color = if card.leader == "right" { Rgba([80, 255, 120, 255]) } else { Rgba([100, 180, 255, 255]) };
            draw_block(&mut canvas, bar_left_x, by, lw, bar_h, l_color);
            draw_block(&mut canvas, bar_right_x, by, rw_w, bar_h, r_color);

            // Numbers at end of each bar
            draw_text_8x8(&mut canvas, bar_left_x + bar_max_w + 4, by + 2, &format!("{:.0}", card.left), Rgba([220, 220, 240, 255]), 1);
            draw_text_8x8(&mut canvas, bar_right_x + bar_max_w + 4, by + 2, &format!("{:.0}", card.right), Rgba([220, 220, 240, 255]), 1);
        }
    }

    // ── Footer with margin and judge ─────────────────────────────────
    let footer_y = height.saturating_sub(60);
    draw_text_8x8(&mut canvas, 28, footer_y, &format!("MARGIN +{:.1}  JUDGE {}", result.winner.margin, result.engine.judge_mode.to_uppercase()), Rgba([180, 180, 200, 255]), 1);
    draw_text_8x8(&mut canvas, 28, footer_y + 18, "github.com/NomaDamas/BetterThanYou", Rgba([99, 235, 211, 255]), 1);

    canvas
}

pub fn generate_share_bundle(result: &BattleResult, output_dir: &Path) -> Result<ShareBundle> {
    fs::create_dir_all(output_dir)?;
    let share_dir = output_dir.join(format!("{}-share", slugify(&result.battle_id)));
    fs::create_dir_all(&share_dir)?;

    let platforms = ["x", "linkedin", "instagram_post", "instagram_story", "tiktok", "pinterest"];
    let mut assets = Vec::new();

    for platform in platforms {
        let file_name = format!("{}.png", platform);
        let path = share_dir.join(file_name);
        let image = render_share_image(result, platform);
        image.save(&path)?;
        assets.push(ShareAsset {
            platform: platform.to_string(),
            image_path: path.display().to_string(),
            caption: share_caption(result, platform),
            open_url: share_url(platform, &share_caption(result, platform)),
            note: share_note(platform).to_string(),
        });
    }

    let bundle = ShareBundle {
        directory: share_dir.display().to_string(),
        assets: assets.clone(),
        manifest_path: share_dir.join("share-pack.json").display().to_string(),
    };
    fs::write(&bundle.manifest_path, serde_json::to_string_pretty(&bundle)?)?;
    Ok(bundle)
}

fn render_public_share_page(
    result: &BattleResult,
    caption: &str,
    preview_image_url: &str,
    report_url: &str,
) -> String {
    let title = format!(
        "{} wins the BetterThanYou battle over {}",
        result.winner.label, if result.winner.id == "left" { &result.inputs.right.label } else { &result.inputs.left.label }
    );
    let description = format!(
        "{} Margin +{:.1}. {}",
        caption.replace('\n', " "),
        result.winner.margin,
        result.sections.why_this_won
    );

    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
    <meta name="description" content="{description}" />
    <meta property="og:type" content="website" />
    <meta property="og:title" content="{title}" />
    <meta property="og:description" content="{description}" />
    <meta property="og:image" content="{preview_image_url}" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="{title}" />
    <meta name="twitter:description" content="{description}" />
    <meta name="twitter:image" content="{preview_image_url}" />
    <style>
      :root {{
        color-scheme: dark;
        --bg: #0b0f17;
        --panel: #111827;
        --text: #f4f7fb;
        --muted: #a9b4c7;
        --accent: #ff8f42;
        --accent-2: #63ebd3;
        --line: rgba(255,255,255,0.1);
      }}
      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        background:
          radial-gradient(circle at top left, rgba(255,143,66,0.22), transparent 28%),
          radial-gradient(circle at top right, rgba(99,235,211,0.18), transparent 24%),
          var(--bg);
        color: var(--text);
      }}
      main {{
        max-width: 880px;
        margin: 0 auto;
        padding: 32px 20px 48px;
      }}
      .eyebrow {{
        color: var(--accent-2);
        font-size: 12px;
        font-weight: 700;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }}
      h1 {{
        margin: 12px 0 6px;
        font-size: clamp(34px, 6vw, 60px);
        line-height: 1;
      }}
      p {{
        color: var(--muted);
        line-height: 1.6;
      }}
      .hero {{
        background: linear-gradient(180deg, rgba(255,255,255,0.04), rgba(255,255,255,0.02));
        border: 1px solid var(--line);
        border-radius: 26px;
        padding: 28px;
      }}
      .image-wrap {{
        margin-top: 22px;
        overflow: hidden;
        border-radius: 22px;
        border: 1px solid var(--line);
        background: rgba(255,255,255,0.02);
      }}
      img {{
        display: block;
        width: 100%;
        height: auto;
      }}
      .stats {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: 12px;
        margin-top: 18px;
      }}
      .card {{
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: 18px;
        padding: 16px;
      }}
      .card strong {{
        display: block;
        font-size: 24px;
        margin-top: 6px;
        color: var(--text);
      }}
      .actions {{
        display: flex;
        flex-wrap: wrap;
        gap: 12px;
        margin-top: 24px;
      }}
      .button {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 12px 16px;
        border-radius: 999px;
        border: 1px solid var(--line);
        color: var(--text);
        text-decoration: none;
        font-weight: 700;
      }}
      .button.primary {{
        background: var(--accent);
        color: #1a0c00;
        border-color: transparent;
      }}

      /* ── Street Fighter overlay ─────────────────────────────────────── */
      @import url("https://fonts.googleapis.com/css2?family=Bebas+Neue&family=Orbitron:wght@600;700;900&display=swap");
      body {{
        background:
          radial-gradient(circle at 18% 26%, rgba(255, 70, 70, 0.32), transparent 38%),
          radial-gradient(circle at 82% 74%, rgba(64, 158, 255, 0.32), transparent 38%),
          repeating-linear-gradient(0deg, rgba(255,255,255,0.025) 0, rgba(255,255,255,0.025) 1px, transparent 1px, transparent 4px),
          var(--bg);
      }}
      .eyebrow {{
        font-family: "Orbitron", sans-serif !important;
        font-weight: 700;
        font-size: 13px !important;
        letter-spacing: 0.32em !important;
      }}
      h1 {{
        font-family: "Bebas Neue", "Orbitron", sans-serif !important;
        font-size: clamp(48px, 9vw, 96px) !important;
        letter-spacing: 0.08em !important;
        text-transform: uppercase;
        background: linear-gradient(180deg, #ffe9a8 0%, #ffd36b 50%, #ff8a3c 100%);
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        text-shadow: 0 0 26px rgba(255, 211, 107, 0.4);
        animation: sf-pulse 1.6s ease-in-out infinite alternate;
      }}
      @keyframes sf-pulse {{
        from {{ filter: drop-shadow(0 0 6px rgba(255, 211, 107, 0.35)); }}
        to {{ filter: drop-shadow(0 0 22px rgba(255, 211, 107, 0.85)); }}
      }}
      .stats {{
        display: grid !important;
        grid-template-columns: 1fr auto 1fr;
        gap: 12px;
        margin-top: 18px;
      }}
      .stats .card {{
        font-family: "Orbitron", sans-serif;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.16em;
        font-size: 12px;
        color: var(--muted);
        text-align: center;
        padding: 14px;
        border: 1px solid var(--line);
        border-radius: 14px;
      }}
      .stats .card strong {{
        display: block;
        margin-top: 6px;
        font-family: "Bebas Neue", "Orbitron", sans-serif;
        font-size: 36px;
        letter-spacing: 0.06em;
        color: var(--accent);
        text-shadow: 0 0 14px rgba(255, 143, 66, 0.6);
      }}
      .stats .card:nth-child(1) {{ border-left: 4px solid #ff4646; }}
      .stats .card:nth-child(3) {{ border-right: 4px solid #4a9eff; }}
      .button.primary {{
        font-family: "Orbitron", sans-serif;
        text-transform: uppercase;
        letter-spacing: 0.18em;
        background: linear-gradient(90deg, #ff4646 0%, #ffd36b 50%, #4a9eff 100%);
        color: #0a0d13;
        box-shadow: 0 0 24px rgba(255, 211, 107, 0.45);
      }}
      .button {{
        font-family: "Orbitron", sans-serif;
        letter-spacing: 0.14em;
        text-transform: uppercase;
        font-size: 13px;
      }}
    </style>
  </head>
  <body>
    <main>
      <section class="hero">
        <div class="eyebrow">BetterThanYou public share</div>
        <h1>{winner}</h1>
        <p>{description}</p>
        <div class="stats">
          <div class="card">Winner<strong>{winner_score:.1}</strong></div>
          <div class="card">Opponent<strong>{opponent_score:.1}</strong></div>
          <div class="card">Margin<strong>+{margin:.1}</strong></div>
        </div>
        <div class="image-wrap">
          <img src="{preview_image_url}" alt="{winner} public preview card" />
        </div>
        <div class="actions">
          <a class="button primary" href="{report_url}">Open full battle report</a>
          <a class="button" href="{preview_image_url}">Open public preview image</a>
        </div>
      </section>
    </main>
  </body>
</html>"#,
        title = html_escape(&title),
        description = html_escape(&description),
        preview_image_url = html_escape(preview_image_url),
        report_url = html_escape(report_url),
        winner = html_escape(&result.winner.label),
        winner_score = result.winner.total_score,
        opponent_score = result.winner.opponent_score,
        margin = result.winner.margin,
    )
}

pub fn load_battle_result_for_html(html_path: &Path) -> Result<BattleResult> {
    let mut candidates = Vec::new();
    if let Some(stem) = html_path.file_stem().and_then(|value| value.to_str()) {
        candidates.push(html_path.with_file_name(format!("{stem}.json")));
    }
    if let Some(parent) = html_path.parent() {
        candidates.push(parent.join("latest-battle.json"));
    }

    for candidate in candidates {
        if candidate.exists() {
            let bytes = fs::read(&candidate)
                .with_context(|| format!("failed to read {}", candidate.display()))?;
            return serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", candidate.display()));
        }
    }

    bail!(
        "No battle JSON found for {}. Expected a sibling .json file or latest-battle.json.",
        html_path.display()
    )
}

// ── Publish: upload report to free hosting and return public URL ──────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedReport {
    pub url: String,
    pub provider: String,
    pub qr_ascii: String,
}

#[derive(Debug, Clone)]
struct PublishedAsset {
    url: String,
    provider: String,
}

/// Render a QR code as ASCII art (printable in terminal).
pub fn qr_ascii(text: &str) -> String {
    use qrcode::render::unicode;
    use qrcode::QrCode;
    let code = match QrCode::new(text.as_bytes()) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    code.render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build()
}

/// Try uploading to litterbox.catbox.moe (temporary, 1h/12h/24h/72h, free, no auth).
/// Returns a direct URL that phone browsers can render as HTML.
async fn try_litterbox(client: &Client, bytes: &[u8], filename: &str, mime: &str) -> Result<String> {
    let form = reqwest::multipart::Form::new()
        .text("reqtype", "fileupload")
        .text("time", "72h")
        .part(
            "fileToUpload",
            reqwest::multipart::Part::bytes(bytes.to_vec())
                .file_name(filename.to_string())
                .mime_str(mime)
                .map_err(|e| anyhow!("invalid mime: {e}"))?,
        );

    let response = client
        .post("https://litterbox.catbox.moe/resources/internals/api.php")
        .header("User-Agent", "BetterThanYou/0.3 (+https://github.com/NomaDamas/BetterThanYou)")
        .multipart(form)
        .send()
        .await
        .with_context(|| "litterbox: network error")?;

    if !response.status().is_success() {
        bail!("litterbox HTTP {}: {}", response.status(), response.text().await.unwrap_or_default());
    }

    let text = response.text().await?.trim().to_string();
    if !text.starts_with("http") {
        bail!("litterbox: unexpected response: {text}");
    }
    Ok(text)
}

/// Try uploading to catbox.moe (permanent, free, no auth).
async fn try_catbox(client: &Client, bytes: &[u8], filename: &str, mime: &str) -> Result<String> {
    let form = reqwest::multipart::Form::new()
        .text("reqtype", "fileupload")
        .part(
            "fileToUpload",
            reqwest::multipart::Part::bytes(bytes.to_vec())
                .file_name(filename.to_string())
                .mime_str(mime)
                .map_err(|e| anyhow!("invalid mime: {e}"))?,
        );

    let response = client
        .post("https://catbox.moe/user/api.php")
        .header("User-Agent", "BetterThanYou/0.3 (+https://github.com/NomaDamas/BetterThanYou)")
        .multipart(form)
        .send()
        .await
        .with_context(|| "catbox.moe: network error")?;

    if !response.status().is_success() {
        bail!("catbox.moe HTTP {}: {}", response.status(), response.text().await.unwrap_or_default());
    }

    let text = response.text().await?.trim().to_string();
    if !text.starts_with("http") {
        bail!("catbox.moe: {text}");
    }
    Ok(text)
}

/// Try uploading to tmpfiles.org (returns JSON with `data.url`).
async fn try_tmpfiles_org(client: &Client, bytes: &[u8], filename: &str, mime: &str) -> Result<String> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str(mime)
        .map_err(|e| anyhow!("invalid mime: {e}"))?;
    let form = reqwest::multipart::Form::new().part("file", part);

    let response = client
        .post("https://tmpfiles.org/api/v1/upload")
        .header("User-Agent", "BetterThanYou/0.3 (+https://github.com/NomaDamas/BetterThanYou)")
        .multipart(form)
        .send()
        .await
        .with_context(|| "tmpfiles.org: network error")?;

    if !response.status().is_success() {
        bail!("tmpfiles.org HTTP {}: {}", response.status(), response.text().await.unwrap_or_default());
    }

    let payload: Value = response.json().await.with_context(|| "tmpfiles.org: invalid JSON")?;
    let raw_url = payload
        .get("data")
        .and_then(|d| d.get("url"))
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow!("tmpfiles.org: missing data.url"))?;
    let direct = raw_url.replacen("tmpfiles.org/", "tmpfiles.org/dl/", 1);
    Ok(direct)
}

/// Try uploading to file.io (returns JSON with `link`, one-time download).
async fn try_file_io(client: &Client, bytes: &[u8], filename: &str, mime: &str) -> Result<String> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str(mime)
        .map_err(|e| anyhow!("invalid mime: {e}"))?;
    let form = reqwest::multipart::Form::new().part("file", part);

    let response = client
        .post("https://file.io/?expires=1w")
        .header("User-Agent", "BetterThanYou/0.3")
        .multipart(form)
        .send()
        .await
        .with_context(|| "file.io: network error")?;

    if !response.status().is_success() {
        bail!("file.io HTTP {}: {}", response.status(), response.text().await.unwrap_or_default());
    }

    let payload: Value = response.json().await.with_context(|| "file.io: invalid JSON")?;
    let link = payload
        .get("link")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow!("file.io: missing link"))?;
    Ok(link.to_string())
}

/// Fetch the latest published release tag from GitHub. Returns `Some("0.8.0")`
/// on success, `None` on any failure (no network, rate limit, etc.) — callers
/// must degrade gracefully so a busted upstream never blocks startup.
pub async fn check_latest_release_version() -> Option<String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;
    let resp = client
        .get("https://api.github.com/repos/NomaDamas/BetterThanYou/releases/latest")
        .header(
            "User-Agent",
            concat!("BetterThanYou/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: Value = resp.json().await.ok()?;
    let tag = json.get("tag_name")?.as_str()?;
    Some(tag.trim_start_matches('v').to_string())
}

/// Returns true if `latest` is strictly newer than `current` using semver-ish
/// numeric comparison. Pre-release suffixes (`-rc.1` etc.) are ignored.
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Option<(u32, u32, u32)> {
        let cleaned = s.split(['-', '+']).next().unwrap_or(s);
        let parts: Vec<&str> = cleaned.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        Some((major, minor, patch))
    };
    match (parse(latest), parse(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Read the configured nomadamas-style publish endpoint and token from env.
/// Returns `Some((base_url, token))` only when both are non-empty; otherwise `None`.
pub fn nomadamas_publish_config() -> Option<(String, String)> {
    let url = std::env::var("BTYU_PUBLISH_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())?;
    let token = std::env::var("BTYU_PUBLISH_TOKEN")
        .ok()
        .filter(|v| !v.trim().is_empty())?;
    Some((url.trim_end_matches('/').to_string(), token))
}

/// Try uploading to a Cloudflare-Worker-backed nomadamas.org-style endpoint.
/// Sends raw bytes (NOT multipart) with `kind` selected from MIME.
async fn try_nomadamas_share(
    client: &Client,
    base_url: &str,
    token: &str,
    bytes: &[u8],
    filename: &str,
    mime: &str,
) -> Result<String> {
    let kind = if mime.starts_with("text/html") {
        "html"
    } else if mime == "image/png" {
        "png"
    } else if mime.starts_with("application/json") {
        "json"
    } else {
        bail!("unsupported mime for nomadamas share: {mime}");
    };

    let endpoint = format!(
        "{}/share?kind={}&filename={}",
        base_url,
        kind,
        urlencoding::encode(filename),
    );

    let response = client
        .post(&endpoint)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", mime)
        .header(
            "User-Agent",
            concat!("BetterThanYou/", env!("CARGO_PKG_VERSION")),
        )
        .body(bytes.to_vec())
        .send()
        .await
        .with_context(|| "nomadamas: network error")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("nomadamas HTTP {}: {}", status, body);
    }

    let payload: Value = response
        .json()
        .await
        .with_context(|| "nomadamas: invalid JSON response")?;
    let url = payload
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow!("nomadamas: response missing `url` field"))?;
    Ok(url.to_string())
}

/// Try uploading to 0x0.st (plain text URL response).
async fn try_0x0_st(client: &Client, bytes: &[u8], filename: &str, mime: &str) -> Result<String> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str(mime)
        .map_err(|e| anyhow!("invalid mime: {e}"))?;
    let form = reqwest::multipart::Form::new().part("file", part);

    let response = client
        .post("https://0x0.st")
        .header("User-Agent", "BetterThanYou/0.3 (+https://github.com/NomaDamas/BetterThanYou)")
        .multipart(form)
        .send()
        .await
        .with_context(|| "0x0.st: network error")?;

    if !response.status().is_success() {
        bail!("0x0.st HTTP {}: {}", response.status(), response.text().await.unwrap_or_default());
    }

    let url = response.text().await?.trim().to_string();
    if !url.starts_with("http") {
        bail!("0x0.st: unexpected response: {url}");
    }
    Ok(url)
}

async fn publish_bytes_to_web(bytes: &[u8], filename: &str, mime: &str) -> Result<PublishedAsset> {
    let client = Client::new();
    let mut last_err: Option<String> = None;

    // Prefer self-hosted Cloudflare endpoint when configured. Falls back silently on failure.
    if let Some((base_url, token)) = nomadamas_publish_config() {
        match try_nomadamas_share(&client, &base_url, &token, bytes, filename, mime).await {
            Ok(url) => {
                return Ok(PublishedAsset {
                    url,
                    provider: "nomadamas.org".to_string(),
                });
            }
            Err(e) => {
                last_err = Some(format!("nomadamas.org: {}", e));
            }
        }
    }

    // Order: catbox (permanent) → litterbox (72h, very reliable) → tmpfiles → file.io → 0x0.st
    for (name, result) in [
        ("catbox.moe", try_catbox(&client, bytes, filename, mime).await),
        ("litterbox.catbox.moe (72h)", try_litterbox(&client, bytes, filename, mime).await),
        ("tmpfiles.org", try_tmpfiles_org(&client, bytes, filename, mime).await),
        ("file.io", try_file_io(&client, bytes, filename, mime).await),
        ("0x0.st", try_0x0_st(&client, bytes, filename, mime).await),
    ] {
        match result {
            Ok(url) => return Ok(PublishedAsset { url, provider: name.to_string() }),
            Err(e) => {
                last_err = Some(format!("{}: {}", name, e));
            }
        }
    }

    bail!(
        "All upload providers failed. Last error: {}",
        last_err.unwrap_or_else(|| "unknown".into())
    )
}

/// Upload an HTML file to a free temporary file host and return the public URL.
/// Tries multiple hosts in order and returns the first that succeeds.
pub async fn publish_html_to_web(html_path: &Path) -> Result<PublishedReport> {
    let bytes = fs::read(html_path).with_context(|| format!("failed to read {}", html_path.display()))?;
    let filename = html_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("battle.html")
        .to_string();
    let published = publish_bytes_to_web(&bytes, &filename, "text/html").await?;
    Ok(PublishedReport {
        qr_ascii: qr_ascii(&published.url),
        url: published.url,
        provider: published.provider,
    })
}

pub async fn publish_share_bundle_to_web(
    result: &BattleResult,
    html_path: &Path,
    output_dir: &Path,
) -> Result<PublishedShareBundle> {
    let share_bundle = generate_share_bundle(result, output_dir)?;
    let preview_asset = share_bundle
        .assets
        .iter()
        .find(|asset| asset.platform == "x")
        .or_else(|| share_bundle.assets.first())
        .ok_or_else(|| anyhow!("share bundle did not contain any assets"))?;
    let preview_path = PathBuf::from(&preview_asset.image_path);

    let preview_bytes = fs::read(&preview_path)
        .with_context(|| format!("failed to read {}", preview_path.display()))?;
    let preview_name = preview_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("share-preview.png")
        .to_string();
    let published_preview = publish_bytes_to_web(&preview_bytes, &preview_name, "image/png").await?;
    let published_report = publish_html_to_web(html_path).await?;

    let caption = share_caption(result, "x");
    let share_page_html = render_public_share_page(
        result,
        &caption,
        &published_preview.url,
        &published_report.url,
    );
    let share_page_name = format!("{}-share-page.html", slugify(&result.battle_id));
    let published_page = publish_bytes_to_web(share_page_html.as_bytes(), &share_page_name, "text/html").await?;
    let social_links = build_social_share_links(&published_page.url, &published_preview.url, &caption);

    Ok(PublishedShareBundle {
        share_page_url: published_page.url.clone(),
        report_url: published_report.url,
        preview_image_url: published_preview.url,
        provider: format!(
            "page: {} | report: {} | image: {}",
            published_page.provider, published_report.provider, published_preview.provider
        ),
        qr_ascii: qr_ascii(&published_page.url),
        caption,
        social_links,
    })
}

// ── Serve: local HTTP server for phone/LAN viewing ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeInfo {
    pub local_url: String,
    pub lan_url: Option<String>,
    pub qr_ascii: String,
    pub port: u16,
}

/// Start a blocking local HTTP server that serves the given directory.
/// Returns when the server is stopped (Ctrl-C). Prints the URL + QR before blocking.
pub fn serve_reports_blocking(reports_dir: &Path, port: u16) -> Result<ServeInfo> {
    use tiny_http::{Header, Response, Server, StatusCode};
    use std::path::PathBuf;

    let bind = format!("0.0.0.0:{}", port);
    let server = Server::http(&bind).map_err(|e| anyhow!("failed to bind {bind}: {e}"))?;

    let lan_ip = local_ip_address::local_ip().ok().map(|ip| ip.to_string());
    let local_url = format!("http://localhost:{}/latest-battle.html", port);
    let lan_url = lan_ip.as_ref().map(|ip| format!("http://{}:{}/latest-battle.html", ip, port));
    let info = ServeInfo {
        local_url: local_url.clone(),
        lan_url: lan_url.clone(),
        qr_ascii: qr_ascii(lan_url.as_deref().unwrap_or(&local_url)),
        port,
    };

    println!();
    println!("\u{1F310} BetterThanYou local server running");
    println!("  \u{2022} Local : {}", local_url);
    if let Some(url) = &lan_url {
        println!("  \u{2022} LAN   : {} (open this on your phone)", url);
    }
    println!();
    println!("Scan with your phone camera:");
    println!("{}", info.qr_ascii);
    println!("Press Ctrl-C to stop the server.");
    println!();

    let reports_dir = reports_dir.to_path_buf();
    for request in server.incoming_requests() {
        let url_path = request.url().split('?').next().unwrap_or("/").to_string();
        let rel = url_path.trim_start_matches('/');
        let target: PathBuf = if rel.is_empty() || rel == "/" {
            reports_dir.join("latest-battle.html")
        } else {
            reports_dir.join(rel)
        };

        // Prevent path traversal: target must be inside reports_dir.
        let canonical_target = target.canonicalize().ok();
        let canonical_root = reports_dir.canonicalize().ok();
        let safe = match (&canonical_target, &canonical_root) {
            (Some(t), Some(r)) => t.starts_with(r),
            _ => false,
        };

        if !safe {
            let _ = request.respond(Response::new_empty(StatusCode(403)));
            continue;
        }

        match fs::read(&target) {
            Ok(bytes) => {
                let mime = if target.extension().and_then(|e| e.to_str()) == Some("html") {
                    "text/html; charset=utf-8"
                } else if target.extension().and_then(|e| e.to_str()) == Some("json") {
                    "application/json"
                } else if target.extension().and_then(|e| e.to_str()) == Some("png") {
                    "image/png"
                } else if target.extension().and_then(|e| e.to_str()) == Some("jpg")
                    || target.extension().and_then(|e| e.to_str()) == Some("jpeg")
                {
                    "image/jpeg"
                } else {
                    "application/octet-stream"
                };
                let header = Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                let response = Response::from_data(bytes).with_header(header);
                let _ = request.respond(response);
            }
            Err(_) => {
                let _ = request.respond(Response::new_empty(StatusCode(404)));
            }
        }
    }

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use tempfile::tempdir;

    fn fixture_image(path: &Path, color: [u8; 4], accent: [u8; 4]) {
        let mut image = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(128, 160);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let in_face = x > 30 && x < 98 && y > 22 && y < 138;
            let in_hair = x > 18 && x < 110 && y > 8 && y < 70;
            let in_shoulders = y > 106 && x > 10 && x < 118;
            *pixel = if in_face { Rgba(accent) } else if in_hair || in_shoulders { Rgba(color) } else { Rgba([16, 18, 24, 255]) };
        }
        image.save(path).unwrap();
    }

    #[tokio::test]
    async fn heuristic_battle_is_deterministic() {
        let dir = tempdir().unwrap();
        let left = dir.path().join("left.png");
        let right = dir.path().join("right.png");
        fixture_image(&left, [240, 180, 150, 255], [255, 240, 228, 255]);
        fixture_image(&right, [32, 60, 112, 255], [122, 240, 212, 255]);

        let result_a = analyze_portrait_battle(AnalyzeOptions {
            left_source: left.display().to_string(),
            right_source: right.display().to_string(),
            left_label: Some("Aurora".into()),
            right_label: Some("Nova".into()),
            judge_mode: JudgeMode::Heuristic,
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            openai_config: OpenAiConfig::default(),
            axis_weights: Vec::new(),
            language: Language::English,
        }).await.unwrap();

        let result_b = analyze_portrait_battle(AnalyzeOptions {
            left_source: left.display().to_string(),
            right_source: right.display().to_string(),
            left_label: Some("Aurora".into()),
            right_label: Some("Nova".into()),
            judge_mode: JudgeMode::Heuristic,
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            openai_config: OpenAiConfig::default(),
            axis_weights: Vec::new(),
            language: Language::English,
        }).await.unwrap();

        assert_eq!(result_a.winner.id, result_b.winner.id);
        assert_eq!(result_a.axis_cards.len(), 10);
    }

    #[tokio::test]
    async fn openai_override_path_works() {
        let dir = tempdir().unwrap();
        let left = dir.path().join("left.png");
        let right = dir.path().join("right.png");
        fixture_image(&left, [240, 180, 150, 255], [255, 240, 228, 255]);
        fixture_image(&right, [32, 60, 112, 255], [122, 240, 212, 255]);

        let result = analyze_portrait_battle_with_override(AnalyzeOptions {
            left_source: left.display().to_string(),
            right_source: right.display().to_string(),
            left_label: Some("Aurora".into()),
            right_label: Some("Nova".into()),
            judge_mode: JudgeMode::Openai,
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            openai_config: OpenAiConfig::default(),
            axis_weights: Vec::new(),
            language: Language::English,
        }, Some(OpenAiJudgeOutput {
            winner_id: "right".into(),
            left_scores: AxisScores {
                facial_symmetry: 70.0, facial_proportions: 65.0, skin_quality: 62.0, eye_expression: 58.0,
                hair_grooming: 60.0, bone_structure: 64.0, expression_charisma: 55.0, lighting_color: 61.0,
                background_framing: 68.0, photogenic_impact: 57.0,
            },
            right_scores: AxisScores {
                facial_symmetry: 82.0, facial_proportions: 84.0, skin_quality: 86.0, eye_expression: 88.0,
                hair_grooming: 80.0, bone_structure: 85.0, expression_charisma: 90.0, lighting_color: 87.0,
                background_framing: 83.0, photogenic_impact: 92.0,
            },
            sections: BattleSections {
                overall_take: "Nova wins on expression and overall impact.".into(),
                strengths: SideTexts { left: "Cleaner symmetry.".into(), right: "Much stronger expression and aura.".into() },
                weaknesses: SideTexts { left: "Feels flat.".into(), right: "Slightly less balanced.".into() },
                why_this_won: "Nova built separation in expression and photogenic impact.".into(),
                model_jury_notes: "Stubbed VLM path.".into(),
            },
            provider: "openai".into(),
            model: DEFAULT_OPENAI_MODEL.into(),
        })).await.unwrap();

        assert_eq!(result.engine.judge_mode, "openai");
        assert_eq!(result.winner.id, "right");
    }

    #[tokio::test]
    async fn public_share_page_contains_social_meta() {
        let dir = tempdir().unwrap();
        let left = dir.path().join("left.png");
        let right = dir.path().join("right.png");
        fixture_image(&left, [240, 180, 150, 255], [255, 240, 228, 255]);
        fixture_image(&right, [32, 60, 112, 255], [122, 240, 212, 255]);

        let result = analyze_portrait_battle(AnalyzeOptions {
            left_source: left.display().to_string(),
            right_source: right.display().to_string(),
            left_label: Some("Aurora".into()),
            right_label: Some("Nova".into()),
            judge_mode: JudgeMode::Heuristic,
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            openai_config: OpenAiConfig::default(),
            axis_weights: Vec::new(),
            language: Language::English,
        }).await.unwrap();

        let html = render_public_share_page(
            &result,
            "Caption line",
            "https://cdn.example.com/preview.png",
            "https://cdn.example.com/report.html",
        );

        assert!(html.contains("og:image"));
        assert!(html.contains("twitter:card"));
        assert!(html.contains("https://cdn.example.com/preview.png"));
        assert!(html.contains("https://cdn.example.com/report.html"));
    }

    #[test]
    fn supported_social_links_include_public_urls() {
        let links = build_social_share_links(
            "https://share.example.com/battle",
            "https://share.example.com/preview.png",
            "Winner caption",
        );

        let x = links.iter().find(|link| link.platform == "x").unwrap();
        assert!(x.share_url.as_ref().unwrap().contains("twitter.com/intent/tweet"));
        assert!(x.share_url.as_ref().unwrap().contains("share.example.com"));

        let pinterest = links.iter().find(|link| link.platform == "pinterest").unwrap();
        assert!(pinterest.share_url.as_ref().unwrap().contains("preview.png"));

        let instagram = links.iter().find(|link| link.platform == "instagram_post").unwrap();
        assert!(instagram.share_url.is_none());
    }
}

const ANSI_RESET: &str = "\u{1b}[0m";
const ANSI_BOLD: &str = "\u{1b}[1m";
const ANSI_AMBER: &str = "\u{1b}[38;2;255;214;107m";
const ANSI_CYAN: &str = "\u{1b}[38;2;0;255;220m";
const ANSI_BLUE: &str = "\u{1b}[38;2;100;180;255m";
const ANSI_DIM: &str = "\u{1b}[38;2;120;120;150m";
const ANSI_GREEN: &str = "\u{1b}[38;2;80;255;120m";
const ANSI_RED: &str = "\u{1b}[38;2;255;70;70m";
const ANSI_MAGENTA: &str = "\u{1b}[38;2;255;60;200m";
const ANSI_PURPLE: &str = "\u{1b}[38;2;180;120;255m";
const ANSI_GOLD: &str = "\u{1b}[38;2;255;215;0m";
const ANSI_ORANGE: &str = "\u{1b}[38;2;255;143;66m";
const ANSI_LIGHT: &str = "\u{1b}[38;2;220;220;240m";

fn paint(text: &str, color: &str, enabled: bool) -> String {
    if enabled { format!("{}{}{}", color, text, ANSI_RESET) } else { text.to_string() }
}

fn game_meter(score: f32, width: usize) -> String {
    let filled = ((score / 100.0) * width as f32).round() as usize;
    let filled = filled.max(1).min(width);
    let empty = width.saturating_sub(filled);
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

fn score_rank_ansi(score: f32) -> (&'static str, &'static str) {
    if score >= 95.0 { ("S+", ANSI_GOLD) }
    else if score >= 90.0 { ("S", ANSI_AMBER) }
    else if score >= 80.0 { ("A", ANSI_GREEN) }
    else if score >= 70.0 { ("B", ANSI_CYAN) }
    else if score >= 60.0 { ("C", ANSI_ORANGE) }
    else if score >= 50.0 { ("D", ANSI_RED) }
    else { ("F", ANSI_RED) }
}

fn meter_color_ansi(score: f32) -> &'static str {
    if score >= 90.0 { ANSI_GOLD }
    else if score >= 75.0 { ANSI_GREEN }
    else if score >= 60.0 { ANSI_CYAN }
    else if score >= 45.0 { ANSI_ORANGE }
    else { ANSI_RED }
}

fn pad_center(value: &str, width: usize) -> String {
    let total = width.saturating_sub(value.len());
    let left = total / 2;
    let right = total - left;
    format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
}

fn boxed_title(title: &str, color: &str, width: usize, enabled: bool) -> [String; 3] {
    let inner = width.saturating_sub(4);
    let centered = pad_center(title, inner);
    [
        paint(&format!("\u{2554}{}\u{2557}", "\u{2550}".repeat(inner)), color, enabled),
        paint(&format!("\u{2551} {} \u{2551}", &centered[1..centered.len().saturating_sub(1)]), color, enabled),
        paint(&format!("\u{255A}{}\u{255D}", "\u{2550}".repeat(inner)), color, enabled),
    ]
}

fn game_panel(title: &str, body: &[String], width: usize, border_color: &str, enabled: bool) -> Vec<String> {
    let inner = width.saturating_sub(4);
    let mut lines = Vec::new();
    let decorated_title = format!("\u{25C6} {} \u{25C6}", title);
    lines.push(paint(&format!("\u{250C}{}\u{2510}", "\u{2500}".repeat(inner)), border_color, enabled));
    lines.push(paint(&format!("\u{2502} {:<width$} \u{2502}", decorated_title, width = inner - 2), border_color, enabled));
    lines.push(paint(&format!("\u{251C}{}\u{2524}", "\u{2500}".repeat(inner)), border_color, enabled));
    for line in body {
        let clipped = if line.chars().count() > inner - 2 { line.chars().take(inner - 2).collect::<String>() } else { line.clone() };
        lines.push(format!("\u{2502} {:<width$} \u{2502}", clipped, width = inner - 2));
    }
    lines.push(paint(&format!("\u{2514}{}\u{2518}", "\u{2500}".repeat(inner)), border_color, enabled));
    lines
}

fn signed_gap(card: &AxisCard, winner_id: &str) -> (String, &'static str) {
    if card.leader == "tie" {
        return (" TIE ".to_string(), ANSI_CYAN);
    }
    let sign = if card.leader == winner_id { "+" } else { "-" };
    let color = if card.leader == winner_id { ANSI_GREEN } else { ANSI_RED };
    (format!("{:>5}", format!("{}{:0.1}", sign, card.diff)), color)
}

fn vs_banner(left_label: &str, right_label: &str, left_score: f32, right_score: f32, width: usize, enabled: bool) -> Vec<String> {
    let inner = width.saturating_sub(4);
    let (left_rank, left_rank_color) = score_rank_ansi(left_score);
    let (right_rank, right_rank_color) = score_rank_ansi(right_score);

    let vs_line = format!(
        "{} [{:.1}] {}  \u{2694} VS \u{2694}  {} [{:.1}] {}",
        left_label.to_uppercase(), left_score, left_rank,
        right_label.to_uppercase(), right_score, right_rank
    );
    let centered_vs = pad_center(&vs_line, inner);

    let mut lines = Vec::new();
    lines.push(paint(&format!("\u{2554}{}\u{2557}", "\u{2550}".repeat(inner)), ANSI_MAGENTA, enabled));
    if enabled {
        let raw_vs = format!(
            "{} [{}] {}  {} \u{2694} VS \u{2694} {}  {} [{}] {}",
            paint(
                &left_label.to_uppercase(),
                ANSI_ORANGE,
                true,
            ),
            paint(&format!("{:.1}", left_score), ANSI_LIGHT, true),
            paint(left_rank, left_rank_color, true),
            ANSI_MAGENTA,
            ANSI_RESET,
            paint(
                &right_label.to_uppercase(),
                ANSI_BLUE,
                true,
            ),
            paint(&format!("{:.1}", right_score), ANSI_LIGHT, true),
            paint(right_rank, right_rank_color, true),
        );
        let padded = pad_center(&raw_vs, inner + 100);
        lines.push(format!("{mag}\u{2551}{rst}{content}{mag}\u{2551}{rst}", mag = ANSI_MAGENTA, rst = ANSI_RESET, content = padded));
    } else {
        lines.push(format!("\u{2551}{}\u{2551}", centered_vs));
    }
    lines.push(paint(&format!("\u{255A}{}\u{255D}", "\u{2550}".repeat(inner)), ANSI_MAGENTA, enabled));
    lines
}

pub fn render_terminal_battle(result: &BattleResult, artifacts: &SavedArtifacts, color: bool) -> String {
    let width = 92;
    let mut lines = Vec::new();

    // ── Logo banner ────────────────────────────────────────────────
    let logo_lines = [
        r" ____       _   _           _____ _                __   __",
        r"| __ )  ___| |_| |_ ___ _ _|_   _| |__   __ _ _ _  \ \ / /__  _   _",
        r"|  _ \ / _ \ __| __/ _ \ '__|| | | '_ \ / _` | '_ \  \ V / _ \| | | |",
        r"| |_) |  __/ |_| ||  __/ |  | | | | | | (_| | | | |  | | (_) | |_| |",
        r"|____/ \___|\__|\__\___|_|  |_| |_| |_|\__,_|_| |_|  |_|\___/ \__,_|",
    ];
    let gradient_colors = [
        "\u{1b}[38;2;255;60;200m",
        "\u{1b}[38;2;230;80;220m",
        "\u{1b}[38;2;200;120;255m",
        "\u{1b}[38;2;150;160;255m",
        "\u{1b}[38;2;100;200;255m",
    ];
    for (i, logo) in logo_lines.iter().enumerate() {
        lines.push(paint(logo, gradient_colors[i], color));
    }
    lines.push(paint("                        C L I   P O R T R A I T   B A T T L E", ANSI_CYAN, color));
    lines.push(String::new());

    // ── VS banner ──────────────────────────────────────────────────
    lines.extend(vs_banner(
        &result.inputs.left.label,
        &result.inputs.right.label,
        result.scores.left.total,
        result.scores.right.total,
        width,
        color,
    ));

    // ── Winner announcement ────────────────────────────────────────
    let winner_text = format!(
        "\u{1F3C6} WINNER: {}  \u{25B2} +{:.1} margin{}",
        result.winner.label.to_uppercase(),
        result.winner.margin,
        if result.winner.decisive { "  DECISIVE!" } else { "" }
    );
    lines.extend(boxed_title(&winner_text, ANSI_GOLD, width, color));
    lines.push(String::new());

    // ── Summary ────────────────────────────────────────────────────
    let judge_line = if let Some(model) = &result.engine.model {
        format!("\u{2696}  Judge: {} via {}", result.engine.judge_mode, model)
    } else {
        format!("\u{2696}  Judge: {}", result.engine.judge_mode)
    };
    let summary = vec![
        format!("\u{1F7E0} Left   : {} {:.1}", result.inputs.left.label, result.scores.left.total),
        format!("\u{1F535} Right  : {} {:.1}", result.inputs.right.label, result.scores.right.total),
        format!("\u{1F4CA} Margin : {:.1} points", result.winner.margin),
        judge_line,
    ];
    lines.extend(game_panel("SCOREBOARD", &summary, width, ANSI_CYAN, color));
    lines.push(String::new());

    // ── Ability stats ──────────────────────────────────────────────
    lines.push(paint("\u{25C6} ABILITY STATS \u{25C6}", ANSI_ORANGE, color));
    lines.push(String::new());
    let term_lang = match result.language.as_deref() {
        Some("ko") => Language::Korean,
        Some("ja") => Language::Japanese,
        _ => Language::English,
    };
    for card in &result.axis_cards {
        let (gap, gap_color) = signed_gap(card, &result.winner.id);
        let gap_text = paint(&gap, gap_color, color);
        let short = localized_axis_short(term_lang, &card.key);
        let icon = axis_icon(&card.key);
        let label_display = format!("{} {}  ({})", icon, short, card.label);

        lines.push(format!(
            "  {}  {}",
            paint(&format!("{:<28}", label_display), ANSI_AMBER, color),
            gap_text
        ));

        let left_meter = paint(&game_meter(card.left, 16), meter_color_ansi(card.left), color);
        let (left_rank, left_rc) = score_rank_ansi(card.left);
        lines.push(format!(
            "    {} {} {} {}",
            paint("L", ANSI_ORANGE, color),
            left_meter,
            format!("{:.1}", card.left),
            paint(left_rank, left_rc, color)
        ));

        let right_meter = paint(&game_meter(card.right, 16), meter_color_ansi(card.right), color);
        let (right_rank, right_rc) = score_rank_ansi(card.right);
        lines.push(format!(
            "    {} {} {} {}",
            paint("R", ANSI_BLUE, color),
            right_meter,
            format!("{:.1}", card.right),
            paint(right_rank, right_rc, color)
        ));
        lines.push(String::new());
    }

    // ── Analysis ───────────────────────────────────────────────────
    let analysis = vec![
        format!("\u{1F4AC} {}", result.sections.overall_take),
        String::new(),
        format!("\u{1F3C6} Why: {}", result.sections.why_this_won),
        String::new(),
        format!("\u{1F4DD} Notes: {}", result.sections.model_jury_notes),
    ];
    lines.extend(game_panel("JUDGE ANALYSIS", &analysis, width, ANSI_PURPLE, color));
    lines.push(String::new());

    // ── Files ──────────────────────────────────────────────────────
    let files = vec![
        format!("\u{1F4C4} HTML report : {}", artifacts.html_path),
        format!("\u{1F4C4} JSON result : {}", artifacts.json_path),
    ];
    lines.extend(game_panel("SAVED ARTIFACTS", &files, width, ANSI_DIM, color));
    lines.join("\n")
}

pub fn render_report_summary(report: &SavedArtifacts, color: bool) -> String {
    let mut lines = Vec::new();
    lines.extend(boxed_title("BETTERTHANYOU // REPORT REBUILT", ANSI_BOLD, 84, color));
    lines.push(paint(&format!("HTML report : {}", report.html_path), ANSI_DIM, color));
    lines.push(paint(&format!("JSON result : {}", report.json_path), ANSI_DIM, color));
    lines.join("\n")
}


fn write_tui_screen(stdout: &mut io::Stdout, screen: &str) -> Result<()> {
    execute!(stdout, cursor::MoveTo(0, 0))?;
    for (row, line) in screen.lines().enumerate() {
        execute!(stdout, cursor::MoveTo(0, row as u16))?;
        write!(stdout, "{}", line)?;
    }
    stdout.flush()?;
    Ok(())
}

pub fn render_open_summary(path: &Path, color: bool) -> String {
    paint(&format!("Opened: {}", path.display()), ANSI_DIM, color)
}

pub fn present_terminal_battle_app(result: &BattleResult, artifacts: &SavedArtifacts, on_open: Option<fn(&Path) -> Result<()>>) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        writeln!(stdout, "{}", render_terminal_battle(result, artifacts, false))?;
        return Ok(());
    }

    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let screen = format!("{}\n{}", render_terminal_battle(result, artifacts, true), paint("Keys: [o] open report  [q] quit", ANSI_DIM, true));
    write_tui_screen(&mut stdout, &screen)?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('o') => {
                    disable_raw_mode()?;
                    execute!(stdout, LeaveAlternateScreen, cursor::Show)?;
                    if let Some(callback) = on_open {
                        callback(Path::new(&artifacts.html_path))?;
                    } else {
                        open_path(Path::new(&artifacts.html_path))?;
                    }
                    return Ok(());
                }
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                    break;
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, cursor::Show)?;
    Ok(())
}
