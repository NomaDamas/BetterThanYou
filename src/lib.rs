use std::fmt::Write as _;
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
use image::{imageops, DynamicImage, Rgba, RgbaImage};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const PRODUCT_NAME: &str = "BetterThanYou";
pub const ENGINE_VERSION: &str = "deterministic-heuristic-v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4.1-mini";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JudgeMode {
    Auto,
    Heuristic,
    Openai,
}

impl JudgeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Heuristic => "heuristic",
            Self::Openai => "openai",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AxisDefinition {
    pub key: &'static str,
    pub label: &'static str,
    pub weight: f32,
}

pub const AXIS_DEFINITIONS: [AxisDefinition; 6] = [
    AxisDefinition { key: "symmetry_harmony", label: "Symmetry & Harmony", weight: 1.0 },
    AxisDefinition { key: "lighting_contrast", label: "Lighting & Contrast", weight: 1.0 },
    AxisDefinition { key: "sharpness_detail", label: "Sharpness & Detail", weight: 1.0 },
    AxisDefinition { key: "color_vitality", label: "Color Vitality", weight: 1.0 },
    AxisDefinition { key: "composition_presence", label: "Composition & Presence", weight: 1.1 },
    AxisDefinition { key: "style_aura", label: "Style Aura", weight: 1.1 },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisScores {
    pub symmetry_harmony: f32,
    pub lighting_contrast: f32,
    pub sharpness_detail: f32,
    pub color_vitality: f32,
    pub composition_presence: f32,
    pub style_aura: f32,
}

impl AxisScores {
    pub fn get(&self, key: &str) -> f32 {
        match key {
            "symmetry_harmony" => self.symmetry_harmony,
            "lighting_contrast" => self.lighting_contrast,
            "sharpness_detail" => self.sharpness_detail,
            "color_vitality" => self.color_vitality,
            "composition_presence" => self.composition_presence,
            "style_aura" => self.style_aura,
            _ => 0.0,
        }
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
    pub axis_cards: Vec<AxisCard>,
    pub winner: Winner,
    pub sections: BattleSections,
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

    let image = image::load_from_memory(&bytes).context("failed to decode image")?;
    let mime = infer_mime_type(&normalized);
    let hash = hash_bytes(&bytes);
    let final_label = label.map(str::to_string).unwrap_or_else(|| side.to_string());

    Ok(LoadedPortrait {
        id: side.to_string(),
        label: final_label,
        source_type: if normalized.starts_with("http") { "url".into() } else if normalized.starts_with("data:image/") { "data-url".into() } else if Path::new(&normalized).exists() { "path".into() } else { "base64".into() },
        width: image.width(),
        height: image.height(),
        hash,
        image_data_url: format!("data:{};base64,{}", mime, base64::engine::general_purpose::STANDARD.encode(&bytes)),
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

fn score_portrait(portrait: &LoadedPortrait) -> ScoreBundle {
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

    let axes = AxisScores {
        symmetry_harmony: round(clamp(100.0 - mirror_difference * 145.0 + hash_signal(&portrait.hash, 0, 4.0), 28.0, 99.0)),
        lighting_contrast: round(clamp(dynamic_range * 62.0 + luminance_deviation * 85.0 + hash_signal(&portrait.hash, 1, 4.0), 24.0, 99.0)),
        sharpness_detail: round(clamp(edge_strength * 190.0 + luminance_deviation * 18.0 + hash_signal(&portrait.hash, 2, 4.0), 22.0, 99.0)),
        color_vitality: round(clamp(average(&saturations) * 76.0 + saturation_deviation * 70.0 + color_spread * 32.0 + hash_signal(&portrait.hash, 3, 4.0), 18.0, 99.0)),
        composition_presence: round(clamp(center_presence * 100.0 + edge_strength * 22.0 + hash_signal(&portrait.hash, 4, 4.0), 20.0, 99.0)),
        style_aura: round(clamp(palette_mood * 48.0 + center_presence * 28.0 + average(&saturations) * 22.0 + dynamic_range * 12.0 + hash_signal(&portrait.hash, 5, 4.0), 20.0, 99.0)),
    };

    ScoreBundle {
        total: round(compute_total_from_axes(&axes)),
        axes,
        telemetry: Some(ScoreTelemetry {
            mirror_difference: round(mirror_difference),
            edge_strength: round(edge_strength),
            center_presence: round(center_presence),
            dynamic_range: round(dynamic_range),
        }),
    }
}

fn compute_total_from_axes(axes: &AxisScores) -> f32 {
    let weighted: f32 = AXIS_DEFINITIONS.iter().map(|axis| axes.get(axis.key) * axis.weight).sum();
    let weights: f32 = AXIS_DEFINITIONS.iter().map(|axis| axis.weight).sum();
    weighted / weights
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
        why_this_won: format!("{} won because it led {} of 6 axes and created its best separation in {} by {:.1} points.", winner.label, lead_axes.len(), decisive.label.to_lowercase(), decisive.diff),
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
        final_sections.model_jury_notes = format!("{} Fallback: {}", final_sections.model_jury_notes, fallback_reason);
    }

    BattleResult {
        battle_id,
        product_name: PRODUCT_NAME.to_string(),
        created_at: Utc::now().to_rfc3339(),
        engine: EngineMeta {
            version: match judge_mode {
                JudgeMode::Heuristic => ENGINE_VERSION.to_string(),
                JudgeMode::Auto => if provider == "openai" { format!("openai-{}", model.clone().unwrap_or_default()) } else { ENGINE_VERSION.to_string() },
                JudgeMode::Openai => format!("openai-{}", model.clone().unwrap_or_default()),
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
        axis_cards,
        winner,
        sections: final_sections,
    }
}

async fn judge_with_openai(left: &LoadedPortrait, right: &LoadedPortrait, model: &str, config: &OpenAiConfig) -> Result<OpenAiJudgeOutput> {
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

    let prompt = format!(
        "You are BetterThanYou, a visual battle judge for fictional AI-generated adult portraits. Judge only image result and presentation quality. Return one winner and score both portraits on every axis from 0 to 100. Axes: {}",
        AXIS_DEFINITIONS.iter().map(|axis| format!("{}: {}", axis.key, axis.label)).collect::<Vec<_>>().join(", ")
    );

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
        AxisScores {
            symmetry_harmony: round(scores.get("symmetry_harmony").and_then(Value::as_f64).unwrap_or(0.0) as f32),
            lighting_contrast: round(scores.get("lighting_contrast").and_then(Value::as_f64).unwrap_or(0.0) as f32),
            sharpness_detail: round(scores.get("sharpness_detail").and_then(Value::as_f64).unwrap_or(0.0) as f32),
            color_vitality: round(scores.get("color_vitality").and_then(Value::as_f64).unwrap_or(0.0) as f32),
            composition_presence: round(scores.get("composition_presence").and_then(Value::as_f64).unwrap_or(0.0) as f32),
            style_aura: round(scores.get("style_aura").and_then(Value::as_f64).unwrap_or(0.0) as f32),
        }
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

pub async fn analyze_portrait_battle_with_override(options: AnalyzeOptions, openai_override: Option<OpenAiJudgeOutput>) -> Result<BattleResult> {
    let left = load_portrait(&options.left_source, options.left_label.as_deref(), "left").await?;
    let right = load_portrait(&options.right_source, options.right_label.as_deref(), "right").await?;

    let api_key_present = options.openai_config.api_key.clone().or_else(|| std::env::var("BTY_OPENAI_API_KEY").ok()).or_else(|| std::env::var("OPENAI_API_KEY").ok()).is_some();
    let should_use_openai = matches!(options.judge_mode, JudgeMode::Openai) || (matches!(options.judge_mode, JudgeMode::Auto) && api_key_present);

    if should_use_openai {
        let judged = if let Some(override_result) = openai_override {
            Ok(override_result)
        } else {
            judge_with_openai(&left, &right, &options.openai_model, &options.openai_config).await
        };

        match judged {
            Ok(vlm) => {
                let left_axes = vlm.left_scores.clone();
                let right_axes = vlm.right_scores.clone();
                let left_scores = ScoreBundle { axes: left_axes.clone(), total: round(compute_total_from_axes(&left_axes)), telemetry: None };
                let right_scores = ScoreBundle { axes: right_axes.clone(), total: round(compute_total_from_axes(&right_axes)), telemetry: None };
                return Ok(build_result(&left, &right, left_scores, right_scores, vlm.sections, JudgeMode::Openai, &vlm.provider, Some(vlm.model), Some(&vlm.winner_id), None));
            }
            Err(error) if matches!(options.judge_mode, JudgeMode::Auto) => {
                let left_scores = score_portrait(&left);
                let right_scores = score_portrait(&right);
                let axis_cards = build_axis_cards(&left_scores, &right_scores);
                let winner_id = pick_winner(&left, &right, &left_scores, &right_scores, &axis_cards, None);
                let winner = Winner {
                    id: winner_id.clone(),
                    label: if winner_id == "left" { left.label.clone() } else { right.label.clone() },
                    total_score: if winner_id == "left" { left_scores.total } else { right_scores.total },
                    opponent_score: if winner_id == "left" { right_scores.total } else { left_scores.total },
                    margin: round((left_scores.total - right_scores.total).abs()),
                    decisive: (left_scores.total - right_scores.total).abs() >= 6.0,
                };
                let sections = build_battle_narrative(&left, &right, &left_scores, &right_scores, &winner, &axis_cards);
                return Ok(build_result(&left, &right, left_scores, right_scores, sections, JudgeMode::Heuristic, "local", None, None, Some(error.to_string())));
            }
            Err(error) => return Err(error),
        }
    }

    let left_scores = score_portrait(&left);
    let right_scores = score_portrait(&right);
    let axis_cards = build_axis_cards(&left_scores, &right_scores);
    let winner_id = pick_winner(&left, &right, &left_scores, &right_scores, &axis_cards, None);
    let winner = Winner {
        id: winner_id.clone(),
        label: if winner_id == "left" { left.label.clone() } else { right.label.clone() },
        total_score: if winner_id == "left" { left_scores.total } else { right_scores.total },
        opponent_score: if winner_id == "left" { right_scores.total } else { left_scores.total },
        margin: round((left_scores.total - right_scores.total).abs()),
        decisive: (left_scores.total - right_scores.total).abs() >= 6.0,
    };
    let sections = build_battle_narrative(&left, &right, &left_scores, &right_scores, &winner, &axis_cards);
    Ok(build_result(&left, &right, left_scores, right_scores, sections, JudgeMode::Heuristic, "local", None, Some(&winner_id), if matches!(options.judge_mode, JudgeMode::Auto) { Some("No OPENAI_API_KEY detected. Using heuristic judge.".into()) } else { None }))
}

pub async fn analyze_portrait_battle(options: AnalyzeOptions) -> Result<BattleResult> {
    analyze_portrait_battle_with_override(options, None).await
}

pub fn render_html_report(result: &BattleResult) -> String {
    let axis_cards = result
        .axis_cards
        .iter()
        .map(|card| {
            format!(
                r#"<article class="axis-card"><header><span>{}</span><strong>{:.1} pt gap</strong></header><div class="axis-values"><div><small>{}</small><b>{:.1}</b></div><div><small>{}</small><b>{:.1}</b></div></div></article>"#,
                card.label,
                card.diff,
                result.inputs.left.label,
                card.left,
                result.inputs.right.label,
                card.right,
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{product} • {left} vs {right}</title>
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
      }}
      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        min-height: 100vh;
        color: var(--text);
        font-family: "Avenir Next", "Trebuchet MS", "Segoe UI", sans-serif;
        background:
          radial-gradient(circle at top left, rgba(255,143,66,0.24), transparent 36%),
          radial-gradient(circle at right center, rgba(99,235,211,0.14), transparent 28%),
          linear-gradient(145deg, #090b10 0%, #121824 100%);
      }}
      .shell {{
        width: min(1180px, calc(100vw - 32px));
        margin: 0 auto;
        padding: 28px 0 56px;
      }}
      .hero, .score-panel, .axis-card, .narrative-block, .input-card {{
        border: 1px solid var(--line);
        border-radius: 24px;
        background: var(--panel);
        backdrop-filter: blur(14px);
      }}
      .hero {{ padding: 28px; box-shadow: 0 24px 70px rgba(0,0,0,0.35); }}
      .eyebrow {{ text-transform: uppercase; letter-spacing: 0.18em; font-size: 12px; color: var(--muted); }}
      .winner-pill {{
        display: inline-flex;
        padding: 10px 16px;
        border-radius: 999px;
        background: rgba(255, 211, 107, 0.12);
        color: var(--winner);
        margin-bottom: 12px;
      }}
      h1 {{ margin: 0; font-size: clamp(42px, 7vw, 88px); line-height: 0.92; text-transform: uppercase; }}
      p {{ line-height: 1.7; color: var(--muted); }}
      .totals, .inputs, .axis-grid, .narrative-grid {{ display: grid; gap: 16px; }}
      .totals {{ grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); margin-top: 18px; }}
      .inputs {{ grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); margin-top: 22px; }}
      .axis-grid, .narrative-grid {{ grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); margin-top: 18px; }}
      .score-panel, .axis-card, .narrative-block {{ padding: 18px; }}
      .score-panel strong {{ display: block; font-size: 40px; margin-top: 8px; color: var(--text); }}
      .axis-card header, .axis-values {{ display: flex; justify-content: space-between; gap: 12px; }}
      .axis-card header {{ margin-bottom: 14px; }}
      .axis-values small, .score-panel small {{ color: var(--muted); display: block; }}
      .input-card {{ overflow: hidden; }}
      .input-card img {{ display: block; width: 100%; aspect-ratio: 4/5; object-fit: cover; }}
      .input-copy {{ padding: 18px; }}
      .input-copy h2, .narrative-block h3 {{ margin: 0 0 8px; }}
      footer {{ margin-top: 22px; color: var(--muted); font-size: 14px; }}
    </style>
  </head>
  <body>
    <main class="shell">
      <section class="hero">
        <div class="eyebrow">winner first • {judge}</div>
        <div class="winner-pill">Winner • {winner}</div>
        <h1>{winner}</h1>
        <p>{overall}</p>
        <div class="totals">
          <article class="score-panel"><small>{left}</small><strong>{left_total:.1}</strong></article>
          <article class="score-panel"><small>{right}</small><strong>{right_total:.1}</strong></article>
          <article class="score-panel"><small>Judge</small><strong>{judge}</strong></article>
        </div>
      </section>

      <section class="inputs">
        <article class="input-card">
          <img alt="{left}" src="{left_src}" />
          <div class="input-copy">
            <h2>{left}</h2>
            <p>{left_strength}</p>
          </div>
        </article>
        <article class="input-card">
          <img alt="{right}" src="{right_src}" />
          <div class="input-copy">
            <h2>{right}</h2>
            <p>{right_strength}</p>
          </div>
        </article>
      </section>

      <section>
        <div class="eyebrow">ability comparison</div>
        <div class="axis-grid">{axis_cards}</div>
      </section>

      <section>
        <div class="eyebrow">analysis</div>
        <div class="narrative-grid">
          <section class="narrative-block"><h3>Overall Take</h3><p>{overall}</p></section>
          <section class="narrative-block"><h3>Why This Won</h3><p>{why}</p></section>
          <section class="narrative-block"><h3>Model Jury Notes</h3><p>{notes}</p></section>
        </div>
      </section>

      <footer>Generated {created_at} • {product}</footer>
    </main>
  </body>
</html>"#,
        product = PRODUCT_NAME,
        left = result.inputs.left.label,
        right = result.inputs.right.label,
        winner = result.winner.label,
        judge = result.engine.model.clone().unwrap_or_else(|| result.engine.judge_mode.clone()),
        overall = result.sections.overall_take,
        why = result.sections.why_this_won,
        notes = result.sections.model_jury_notes,
        left_total = result.scores.left.total,
        right_total = result.scores.right.total,
        left_src = result.inputs.left.image_data_url,
        right_src = result.inputs.right.image_data_url,
        left_strength = result.sections.strengths.left,
        right_strength = result.sections.strengths.right,
        created_at = result.created_at,
        axis_cards = axis_cards,
    )
}

pub fn default_reports_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reports")
}

pub fn save_battle_artifacts(result: &BattleResult, output_dir: &Path) -> Result<SavedArtifacts> {
    fs::create_dir_all(output_dir)?;
    let stem = format!("{}-{}", Utc::now().format("%Y-%m-%dt%H-%M-%S-%3fz"), slugify(&format!("{}-{}", result.inputs.left.label, result.inputs.right.label)));
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
    let core = format!("{} beats {} on BetterThanYou. Winner: {}. Margin: {:.1}.", result.inputs.left.label, result.inputs.right.label, result.winner.label, result.winner.margin);
    match platform {
        "x" => format!("{} #BetterThanYou #AIPortraits", core),
        "linkedin" => format!("{} Winner-first portrait battle result generated with BetterThanYou.", core),
        "instagram_post" => format!("{} Portrait battle result. Upload the saved card to your feed.", core),
        "instagram_story" => format!("{} Story-ready asset generated by BetterThanYou.", core),
        "tiktok" => format!("{} Use the story-size card as a cover or upload asset.", core),
        "pinterest" => format!("{} Pin-ready vertical asset generated by BetterThanYou.", core),
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
        "x" => "Opens a prefilled X compose link when available.",
        "linkedin" => "Opens LinkedIn. Upload the generated card manually from the share folder.",
        "instagram_post" => "Use the generated feed asset for Instagram post upload.",
        "instagram_story" => "Use the generated 9:16 asset for Instagram Story upload.",
        "tiktok" => "Use the generated 9:16 asset for TikTok upload or cover art.",
        "pinterest" => "Use the generated vertical asset for Pinterest pin upload.",
        _ => "Generated social share asset.",
    }
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

fn render_share_image(result: &BattleResult, platform: &str) -> RgbaImage {
    let (width, height) = platform_dimensions(platform);
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([10, 13, 20, 255]));
    draw_block(&mut canvas, 0, 0, width, height / 3, Rgba([24, 28, 44, 255]));
    draw_block(&mut canvas, 0, height / 3, width, height / 3, Rgba([14, 18, 28, 255]));
    draw_block(&mut canvas, 0, (height / 3) * 2, width, height / 3 + 4, Rgba([18, 22, 34, 255]));
    draw_block(&mut canvas, 0, 0, width, 20, Rgba([255, 140, 66, 255]));

    let left_image = image::load_from_memory(&base64::engine::general_purpose::STANDARD.decode(result.inputs.left.image_data_url.split(',').nth(1).unwrap_or("")).unwrap_or_default())
        .unwrap_or_else(|_| DynamicImage::new_rgba8(64, 64))
        .resize(width / 2 - 48, height / 2, imageops::FilterType::Triangle)
        .to_rgba8();
    let right_image = image::load_from_memory(&base64::engine::general_purpose::STANDARD.decode(result.inputs.right.image_data_url.split(',').nth(1).unwrap_or("")).unwrap_or_default())
        .unwrap_or_else(|_| DynamicImage::new_rgba8(64, 64))
        .resize(width / 2 - 48, height / 2, imageops::FilterType::Triangle)
        .to_rgba8();

    imageops::overlay(&mut canvas, &left_image, 24, 100);
    imageops::overlay(&mut canvas, &right_image, (width / 2 + 24) as i64, 100);

    draw_text_8x8(&mut canvas, 28, 32, "BETTERTHANYOU", Rgba([245, 239, 228, 255]), 3);
    draw_text_8x8(&mut canvas, 28, 62, &format!("WINNER // {}", result.winner.label.to_uppercase()), Rgba([255, 207, 90, 255]), 2);
    draw_text_8x8(&mut canvas, 28, height - 180, &format!("LEFT  {:.1}", result.scores.left.total), Rgba([120, 240, 212, 255]), 2);
    draw_text_8x8(&mut canvas, width / 2 + 24, height - 180, &format!("RIGHT {:.1}", result.scores.right.total), Rgba([141, 183, 255, 255]), 2);
    draw_text_8x8(&mut canvas, 28, height - 140, &format!("JUDGE {}", result.engine.judge_mode.to_uppercase()), Rgba([210, 197, 178, 255]), 2);
    draw_text_8x8(&mut canvas, 28, height - 100, &format!("MARGIN {:.1}", result.winner.margin), Rgba([245, 239, 228, 255]), 2);
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
        }).await.unwrap();

        let result_b = analyze_portrait_battle(AnalyzeOptions {
            left_source: left.display().to_string(),
            right_source: right.display().to_string(),
            left_label: Some("Aurora".into()),
            right_label: Some("Nova".into()),
            judge_mode: JudgeMode::Heuristic,
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            openai_config: OpenAiConfig::default(),
        }).await.unwrap();

        assert_eq!(result_a.winner.id, result_b.winner.id);
        assert_eq!(result_a.axis_cards.len(), 6);
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
        }, Some(OpenAiJudgeOutput {
            winner_id: "right".into(),
            left_scores: AxisScores { symmetry_harmony: 70.0, lighting_contrast: 61.0, sharpness_detail: 55.0, color_vitality: 52.0, composition_presence: 68.0, style_aura: 64.0 },
            right_scores: AxisScores { symmetry_harmony: 82.0, lighting_contrast: 84.0, sharpness_detail: 72.0, color_vitality: 91.0, composition_presence: 88.0, style_aura: 90.0 },
            sections: BattleSections {
                overall_take: "Nova wins on color and presence.".into(),
                strengths: SideTexts { left: "Cleaner symmetry.".into(), right: "Much stronger color and aura.".into() },
                weaknesses: SideTexts { left: "Feels flat.".into(), right: "Slightly less balanced.".into() },
                why_this_won: "Nova built separation in color vitality and style aura.".into(),
                model_jury_notes: "Stubbed VLM path.".into(),
            },
            provider: "openai".into(),
            model: DEFAULT_OPENAI_MODEL.into(),
        })).await.unwrap();

        assert_eq!(result.engine.judge_mode, "openai");
        assert_eq!(result.winner.id, "right");
    }
}

const ANSI_RESET: &str = "\u{1b}[0m";
const ANSI_BOLD: &str = "\u{1b}[1m";
const ANSI_AMBER: &str = "\u{1b}[38;5;215m";
const ANSI_CYAN: &str = "\u{1b}[38;5;80m";
const ANSI_BLUE: &str = "\u{1b}[38;5;111m";
const ANSI_DIM: &str = "\u{1b}[38;5;246m";
const ANSI_GREEN: &str = "\u{1b}[38;5;120m";
const ANSI_RED: &str = "\u{1b}[38;5;203m";

fn paint(text: &str, color: &str, enabled: bool) -> String {
    if enabled { format!("{}{}{}", color, text, ANSI_RESET) } else { text.to_string() }
}

fn meter(score: f32) -> String {
    let filled = ((score / 5.0).round() as usize).max(1).min(20);
    format!("{}{}", "█".repeat(filled), "░".repeat(20 - filled))
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
        paint(&format!("╔{}╗", "═".repeat(inner)), color, enabled),
        paint(&format!("║ {} ║", &centered[1..centered.len().saturating_sub(1)]), color, enabled),
        paint(&format!("╚{}╝", "═".repeat(inner)), color, enabled),
    ]
}


fn info_panel(title: &str, body: &[String], width: usize, color: &str, enabled: bool) -> Vec<String> {
    let inner = width.saturating_sub(4);
    let mut lines = Vec::new();
    lines.push(paint(&format!("┌{}┐", "─".repeat(inner)), color, enabled));
    lines.push(paint(&format!("│ {:<width$} │", title, width = inner - 2), color, enabled));
    lines.push(paint(&format!("├{}┤", "─".repeat(inner)), color, enabled));
    for line in body {
        let clipped = if line.chars().count() > inner - 2 { line.chars().take(inner - 2).collect::<String>() } else { line.clone() };
        lines.push(format!("│ {:<width$} │", clipped, width = inner - 2));
    }
    lines.push(paint(&format!("└{}┘", "─".repeat(inner)), color, enabled));
    lines
}

fn signed_gap(card: &AxisCard, winner_id: &str) -> (String, &'static str) {
    if card.leader == "tie" {
        return ("TIE ".to_string(), ANSI_CYAN);
    }
    let sign = if card.leader == winner_id { "+" } else { "-" };
    let color = if card.leader == winner_id { ANSI_GREEN } else { ANSI_RED };
    (format!("{:>5}", format!("{}{:0.1}", sign, card.diff)), color)
}

pub fn render_terminal_battle(result: &BattleResult, artifacts: &SavedArtifacts, color: bool) -> String {
    let width = 92;
    let winner_color = if result.winner.id == "left" { ANSI_AMBER } else { ANSI_BLUE };
    let mut lines = Vec::new();
    lines.extend(boxed_title("BETTERTHANYOU // CLI PORTRAIT BATTLE", ANSI_BOLD, width, color));
    lines.extend(boxed_title(&format!("WINNER // {}", result.winner.label.to_uppercase()), winner_color, width, color));

    let judge_line = if let Some(model) = &result.engine.model {
        format!("Judge: {} via {}", result.engine.judge_mode, model)
    } else {
        format!("Judge: {}", result.engine.judge_mode)
    };

    let summary = vec![
        format!("Left total   : {:.1}", result.scores.left.total),
        format!("Right total  : {:.1}", result.scores.right.total),
        format!("Margin       : {:.1} points", result.winner.margin),
        judge_line,
    ];
    lines.extend(info_panel("SUMMARY", &summary, width, ANSI_DIM, color));
    lines.push(String::new());
    lines.push(paint("ABILITY COMPARISON", ANSI_CYAN, color));
    lines.push(paint("Axis                      Left                         Right                        Gap", ANSI_DIM, color));
    for card in &result.axis_cards {
        let (gap, gap_color) = signed_gap(card, &result.winner.id);
        let gap_text = paint(&gap, gap_color, color);
        lines.push(format!(
            "{:<24} {:>5} {} | {:>5} {}  {}",
            card.label,
            format!("{:.1}", card.left),
            meter(card.left),
            format!("{:.1}", card.right),
            meter(card.right),
            gap_text
        ));
    }
    lines.push(String::new());
    let analysis = vec![
        result.sections.overall_take.clone(),
        String::new(),
        format!("Why: {}", result.sections.why_this_won),
        String::new(),
        format!("Notes: {}", result.sections.model_jury_notes),
    ];
    lines.extend(info_panel("ANALYSIS", &analysis, width, ANSI_CYAN, color));
    lines.push(String::new());
    let files = vec![
        format!("HTML report : {}", artifacts.html_path),
        format!("JSON result : {}", artifacts.json_path),
    ];
    lines.extend(info_panel("SAVE FILES", &files, width, ANSI_DIM, color));
    lines.join("
")
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
