mod ui;

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Result};
use better_than_you::{
    analyze_portrait_battle, default_reports_dir, generate_share_bundle, open_path,
    read_clipboard_text, regenerate_battle_report, render_open_summary, render_report_summary,
    render_terminal_battle, save_battle_artifacts, write_clipboard_text, AnalyzeOptions, AXIS_DEFINITIONS,
    BattleResult, JudgeMode, Language, t, OPENAI_VLM_MODELS, ANTHROPIC_VLM_MODELS, GEMINI_VLM_MODELS,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, ValueEnum, Serialize, Deserialize)]
enum JudgeCli {
    Auto,
    Heuristic,
    Openai,
    Anthropic,
    Gemini,
}

impl From<JudgeCli> for JudgeMode {
    fn from(value: JudgeCli) -> Self {
        match value {
            JudgeCli::Auto => JudgeMode::Auto,
            JudgeCli::Heuristic => JudgeMode::Heuristic,
            JudgeCli::Openai => JudgeMode::Openai,
            JudgeCli::Anthropic => JudgeMode::Anthropic,
            JudgeCli::Gemini => JudgeMode::Gemini,
        }
    }
}

impl JudgeCli {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Heuristic => "heuristic",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "better-than-you", about = "CLI-first portrait battle tool")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    left: Option<String>,
    right: Option<String>,

    #[arg(long)]
    left_label: Option<String>,
    #[arg(long)]
    right_label: Option<String>,
    #[arg(long)]
    out_dir: Option<PathBuf>,
    #[arg(long)]
    left_clipboard: bool,
    #[arg(long)]
    right_clipboard: bool,
    #[arg(long, value_enum, default_value = "auto")]
    judge: JudgeCli,
    #[arg(long, default_value = "gpt-5.4-mini")]
    model: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    open: bool,
    #[arg(long)]
    no_app: bool,
    #[arg(long = "axis-weight", value_name = "KEY=WEIGHT")]
    axis_weights: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Battle(BattleArgs),
    Report(ReportArgs),
    Open(OpenArgs),
}

#[derive(Parser, Debug, Clone)]
struct BattleArgs {
    left: Option<String>,
    right: Option<String>,
    #[arg(long)]
    left_label: Option<String>,
    #[arg(long)]
    right_label: Option<String>,
    #[arg(long)]
    out_dir: Option<PathBuf>,
    #[arg(long)]
    left_clipboard: bool,
    #[arg(long)]
    right_clipboard: bool,
    #[arg(long, value_enum, default_value = "auto")]
    judge: JudgeCli,
    #[arg(long, default_value = "gpt-5.4-mini")]
    model: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    open: bool,
    #[arg(long)]
    no_app: bool,
    #[arg(long = "axis-weight", value_name = "KEY=WEIGHT")]
    axis_weights: Vec<String>,
}

#[derive(Parser, Debug)]
struct ReportArgs {
    battle_json_path: Option<PathBuf>,
    #[arg(long)]
    out_dir: Option<PathBuf>,
    #[arg(long)]
    open: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Parser, Debug)]
struct OpenArgs {
    target: Option<PathBuf>,
    #[arg(long)]
    out_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppState {
    star_acknowledged: bool,
    openai_api_key: Option<String>,
    #[serde(default)]
    anthropic_api_key: Option<String>,
    #[serde(default)]
    gemini_api_key: Option<String>,
    judge: Option<JudgeCli>,
    model: Option<String>,
    out_dir: Option<PathBuf>,
    #[serde(default)]
    axis_weights: Vec<(String, f32)>,
    #[serde(default)]
    language: Option<Language>,
}

#[derive(Debug, Clone)]
struct SessionState {
    app_state: AppState,
    left: Option<String>,
    right: Option<String>,
    left_label: Option<String>,
    right_label: Option<String>,
    judge: JudgeCli,
    model: String,
    out_dir: PathBuf,
    axis_weights: Vec<(String, f32)>,
    last_json: Option<PathBuf>,
    last_html: Option<PathBuf>,
    last_result: Option<BattleResult>,
    last_pair: Option<(String, String)>,
    language: Language,
}

impl SessionState {
    fn new() -> Self {
        let app_state = load_app_state();
        Self {
            judge: app_state.judge.clone().unwrap_or(JudgeCli::Auto),
            model: app_state.model.clone().unwrap_or_else(|| "gpt-5.4-mini".to_string()),
            out_dir: app_state.out_dir.clone().unwrap_or_else(default_reports_dir),
            axis_weights: app_state.axis_weights.clone(),
            language: app_state.language.unwrap_or(Language::English),
            app_state,
            left: None,
            right: None,
            left_label: None,
            right_label: None,
            last_json: None,
            last_html: None,
            last_result: None,
            last_pair: None,
        }
    }
}

fn app_state_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/better-than-you/state.json"))
}

fn load_app_state() -> AppState {
    let Some(path) = app_state_path() else { return AppState::default(); };
    let Ok(bytes) = fs::read(path) else { return AppState::default(); };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_app_state(state: &AppState) -> Result<()> {
    let Some(path) = app_state_path() else { return Ok(()); };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

fn normalize_input(value: String) -> String {
    value.trim().to_string()
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(normalize_input(input))
}

fn prompt_optional(prompt: &str) -> Result<Option<String>> {
    let value = prompt_line(prompt)?;
    if value.is_empty() { Ok(None) } else { Ok(Some(value)) }
}

fn read_piped_lines() -> Result<Vec<String>> {
    let mut input = String::new();
    io::Read::read_to_string(&mut io::stdin(), &mut input)?;
    Ok(input
        .replace('\r', "")
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn resolve_axis_weight(entry: &str) -> Result<(String, f32), String> {
    let (key, value) = entry
        .split_once('=')
        .ok_or_else(|| format!("axis-weight must be KEY=WEIGHT: {entry}"))?;
    let weight = value.parse::<f32>().map_err(|_| format!("invalid axis weight value for {}: {value}", key))?;
    if !weight.is_finite() || weight < 0.0 {
        return Err(format!("axis-weight must be finite and non-negative for {key}"));
    }
    if !AXIS_DEFINITIONS.iter().any(|axis| axis.key == key) {
        return Err(format!("unknown axis key: {key}"));
    }
    Ok((key.to_string(), weight))
}

fn get_axis_weight(overrides: &[(String, f32)], key: &str) -> f32 {
    overrides
        .iter()
        .find(|(axis_key, _)| axis_key == key)
        .map(|(_, weight)| *weight)
        .or_else(|| {
            AXIS_DEFINITIONS
                .iter()
                .find(|axis| axis.key == key)
                .map(|axis| axis.weight)
        })
        .unwrap_or(0.0)
}

fn parse_axis_weights(raw: &[String]) -> Result<Vec<(String, f32)>> {
    let mut overrides = Vec::new();
    for item in raw {
        overrides.push(resolve_axis_weight(item).map_err(|e| anyhow!(e))?);
    }
    Ok(overrides)
}

fn set_axis_weight(overrides: &mut Vec<(String, f32)>, key: &str, weight: f32) {
    if let Some(existing) = overrides.iter_mut().find(|(axis_key, _)| axis_key == key) {
        existing.1 = weight;
        return;
    }
    overrides.push((key.to_string(), weight));
}

fn resolve_sources(left: Option<String>, right: Option<String>, left_clipboard: bool, right_clipboard: bool) -> Result<(String, String)> {
    let mut left_value = left.map(normalize_input);
    let mut right_value = right.map(normalize_input);

    if left_clipboard {
        left_value = Some(read_clipboard_text()?);
    }
    if right_clipboard {
        right_value = Some(read_clipboard_text()?);
    }

    if left_value.is_some() && right_value.is_some() {
        return Ok((left_value.unwrap(), right_value.unwrap()));
    }

    if !io::stdin().is_terminal() {
        let piped = read_piped_lines()?;
        if left_value.is_none() {
            left_value = piped.get(0).cloned();
        }
        if right_value.is_none() {
            right_value = piped.get(1).cloned();
        }
    } else {
        if left_value.is_none() {
            left_value = Some(prompt_line("Drag or paste LEFT portrait path/URL/data URL: ")?);
        }
        if right_value.is_none() {
            right_value = Some(prompt_line("Drag or paste RIGHT portrait path/URL/data URL: ")?);
        }
    }

    match (left_value, right_value) {
        (Some(left), Some(right)) if !left.is_empty() && !right.is_empty() => Ok((left, right)),
        _ => bail!("Two portrait inputs are required."),
    }
}

fn maybe_print_star_reminder(state: &AppState) {
    if !state.star_acknowledged && !io::stdout().is_terminal() {
        eprintln!("\u{2B50} Star one click = dev gets power-up!  https://github.com/NomaDamas/BetterThanYou");
    }
}

fn select_menu(title: &str, subtitle: &[String], items: &[String], initial_index: usize) -> Result<Option<usize>> {
    match ui::select_menu(title, subtitle, items, initial_index) {
        Ok(result) => Ok(result),
        Err(_) => Ok(Some(initial_index.min(items.len().saturating_sub(1))))
    }
}



fn judge_index(judge: &JudgeCli) -> usize {
    match judge {
        JudgeCli::Auto => 0,
        JudgeCli::Openai => 1,
        JudgeCli::Anthropic => 2,
        JudgeCli::Gemini => 3,
        JudgeCli::Heuristic => 4,
    }
}

fn ensure_openai_ready(state: &mut SessionState) -> Result<()> {
    if state.app_state.openai_api_key.is_some() || std::env::var("OPENAI_API_KEY").is_ok() {
        return Ok(());
    }

    let subtitle = vec![
        "OpenAI judge needs an API key.".to_string(),
        "Choose how to continue.".to_string(),
    ];
    let items = vec![
        "Enter API key now".to_string(),
        "Switch judge to auto".to_string(),
        "Switch judge to heuristic".to_string(),
        "Cancel".to_string(),
    ];

    match select_menu("OpenAI Key Required", &subtitle, &items, 0)? {
        Some(0) => {
            if let Some(key) = ui::text_input("OpenAI API Key", "Paste your API key (sk-...)", "", true)? {
                if !key.is_empty() {
                    state.app_state.openai_api_key = Some(key);
                    save_app_state(&state.app_state)?;
                }
            }
        }
        Some(1) => state.judge = JudgeCli::Auto,
        Some(2) => state.judge = JudgeCli::Heuristic,
        _ => bail!("OpenAI key entry cancelled."),
    }

    Ok(())
}

async fn battle_from_args(args: &BattleArgs, state: Option<&SessionState>) -> Result<(BattleResult, better_than_you::SavedArtifacts)> {
    let (left_source, right_source) = resolve_sources(args.left.clone(), args.right.clone(), args.left_clipboard, args.right_clipboard)?;
    let output_dir = args.out_dir.clone().unwrap_or_else(default_reports_dir);

    let mut options = AnalyzeOptions::new(left_source, right_source);
    options.left_label = args.left_label.clone();
    options.right_label = args.right_label.clone();
    options.judge_mode = args.judge.clone().into();
    options.openai_model = args.model.clone();
    options.axis_weights = parse_axis_weights(&args.axis_weights)?;
    if let Some(session_state) = state {
        options.openai_config.api_key = session_state.app_state.openai_api_key.clone();
        options.language = session_state.language;
        // Set saved keys as env vars so lib.rs can find them
        if let Some(key) = &session_state.app_state.anthropic_api_key {
            std::env::set_var("ANTHROPIC_API_KEY", key);
        }
        if let Some(key) = &session_state.app_state.gemini_api_key {
            std::env::set_var("GEMINI_API_KEY", key);
        }
    }

    let result = analyze_portrait_battle(options).await?;
    let artifacts = save_battle_artifacts(&result, &output_dir)?;
    Ok((result, artifacts))
}

async fn run_battle(args: BattleArgs) -> Result<()> {
    let (result, artifacts) = battle_from_args(&args, None).await?;

    if args.open {
        open_path(PathBuf::from(&artifacts.html_path).as_path())?;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"result": result, "artifacts": artifacts}))?);
        return Ok(());
    }

    if !args.no_app && io::stdin().is_terminal() && io::stdout().is_terminal() {
        ui::present_battle_view(&result, &artifacts, &["Enter/q return".to_string(), "o open report".to_string()], None)?;
    } else {
        println!("{}", render_terminal_battle(&result, &artifacts, io::stdout().is_terminal()));
    }

    Ok(())
}

async fn run_report(args: ReportArgs) -> Result<()> {
    let battle_json = match args.battle_json_path {
        Some(path) => path,
        None if !io::stdin().is_terminal() => {
            let mut piped = read_piped_lines()?;
            PathBuf::from(piped.remove(0))
        }
        None => PathBuf::from(prompt_line("Paste battle JSON path: ")?),
    };
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);
    let artifacts = regenerate_battle_report(&battle_json, &output_dir)?;

    if args.open {
        open_path(PathBuf::from(&artifacts.html_path).as_path())?;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&artifacts)?);
    } else {
        println!("{}", render_report_summary(&artifacts, io::stdout().is_terminal()));
    }
    Ok(())
}

fn run_open(args: OpenArgs) -> Result<()> {
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);
    let path = args.target.unwrap_or_else(|| output_dir.join("latest-battle.html"));
    open_path(&path)?;
    println!("{}", render_open_summary(&path, io::stdout().is_terminal()));
    Ok(())
}

fn run_settings_menu(state: &mut SessionState) -> Result<()> {
    loop {
        let subtitle = vec![
            format!("Current judge: {}", state.judge.as_str()),
            format!("Current model: {}", state.model),
        ];
        let items = vec![
            "Judge mode".to_string(),
            "Model".to_string(),
            "Labels".to_string(),
            "Paste both from clipboard".to_string(),
            "Output directory".to_string(),
            "API keys".to_string(),
            "Aesthetic tuning".to_string(),
            "Language".to_string(),
            "Back".to_string(),
        ];
        match select_menu("Settings", &subtitle, &items, 0)? {
            Some(0) => {
                let judge_items = vec![
                    "Auto (recommended)".to_string(),
                    "OpenAI judge".to_string(),
                    "Anthropic judge".to_string(),
                    "Gemini judge".to_string(),
                    "Heuristic judge".to_string(),
                ];
                if let Some(choice) = select_menu("Judge Mode", &[], &judge_items, judge_index(&state.judge))? {
                    state.judge = match choice {
                        0 => JudgeCli::Auto,
                        1 => JudgeCli::Openai,
                        2 => JudgeCli::Anthropic,
                        3 => JudgeCli::Gemini,
                        _ => JudgeCli::Heuristic,
                    };
                    state.app_state.judge = Some(state.judge.clone());
                    save_app_state(&state.app_state)?;
                }
            }
            Some(1) => {
                let model_list: Vec<String> = match state.judge {
                    JudgeCli::Anthropic => ANTHROPIC_VLM_MODELS.iter().map(|s| s.to_string()).collect(),
                    JudgeCli::Gemini => GEMINI_VLM_MODELS.iter().map(|s| s.to_string()).collect(),
                    _ => OPENAI_VLM_MODELS.iter().map(|s| s.to_string()).collect(),
                };
                let mut items_with_custom = model_list.clone();
                items_with_custom.push("Custom (type model name)".to_string());
                let current_index = model_list.iter().position(|m| m == &state.model).unwrap_or(0);
                if let Some(choice) = select_menu("Select Model", &[], &items_with_custom, current_index)? {
                    if choice < model_list.len() {
                        state.model = model_list[choice].clone();
                    } else if let Some(model) = ui::text_input("Custom Model", "Enter model name", "", false)? {
                        if !model.is_empty() { state.model = model; }
                    }
                    state.app_state.model = Some(state.model.clone());
                    save_app_state(&state.app_state)?;
                }
            }
            Some(2) => {
                if let Some(label) = ui::text_input("Left Label", "Optional name for the left portrait", state.left_label.as_deref().unwrap_or(""), false)? {
                    state.left_label = if label.is_empty() { None } else { Some(label) };
                }
                if let Some(label) = ui::text_input("Right Label", "Optional name for the right portrait", state.right_label.as_deref().unwrap_or(""), false)? {
                    state.right_label = if label.is_empty() { None } else { Some(label) };
                }
            }
            Some(3) => {
                let clip = read_clipboard_text()?;
                let parts: Vec<String> = clip.replace('\r', "").split('\n').map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect();
                if parts.len() >= 2 {
                    state.left = Some(parts[0].clone());
                    state.right = Some(parts[1].clone());
                }
            }
            Some(4) => {
                if let Some(out) = ui::text_input("Output Directory", "Path where reports are saved", &state.out_dir.display().to_string(), false)? {
                    if !out.is_empty() {
                        state.out_dir = PathBuf::from(out);
                        state.app_state.out_dir = Some(state.out_dir.clone());
                        save_app_state(&state.app_state)?;
                    }
                }
            }
            Some(5) => {
                let openai_status = if state.app_state.openai_api_key.is_some() { " \u{2714}" } else { "" };
                let anthropic_status = if state.app_state.anthropic_api_key.is_some() { " \u{2714}" } else { "" };
                let gemini_status = if state.app_state.gemini_api_key.is_some() { " \u{2714}" } else { "" };
                let items = vec![
                    format!("Set OpenAI API key{}", openai_status),
                    format!("Set Anthropic API key{}", anthropic_status),
                    format!("Set Gemini API key{}", gemini_status),
                    "Clear all saved keys".to_string(),
                    "Back".to_string(),
                ];
                match select_menu("API Keys", &[], &items, 0)? {
                    Some(0) => {
                        if let Some(key) = ui::text_input("OpenAI API Key", "Paste your OpenAI API key (sk-...)", "", true)? {
                            if !key.is_empty() {
                                state.app_state.openai_api_key = Some(key);
                                save_app_state(&state.app_state)?;
                            }
                        }
                    }
                    Some(1) => {
                        if let Some(key) = ui::text_input("Anthropic API Key", "Paste your Anthropic API key", "", true)? {
                            if !key.is_empty() {
                                state.app_state.anthropic_api_key = Some(key);
                                save_app_state(&state.app_state)?;
                            }
                        }
                    }
                    Some(2) => {
                        if let Some(key) = ui::text_input("Gemini API Key", "Paste your Gemini API key", "", true)? {
                            if !key.is_empty() {
                                state.app_state.gemini_api_key = Some(key);
                                save_app_state(&state.app_state)?;
                            }
                        }
                    }
                    Some(3) => {
                        state.app_state.openai_api_key = None;
                        state.app_state.anthropic_api_key = None;
                        state.app_state.gemini_api_key = None;
                        save_app_state(&state.app_state)?;
                    }
                    _ => {}
                }
            }
            Some(7) => {
                let lang_items = vec!["English".to_string(), "한국어".to_string(), "日本語".to_string()];
                let current = match state.language { Language::English => 0, Language::Korean => 1, Language::Japanese => 2 };
                if let Some(choice) = select_menu("Language", &[], &lang_items, current)? {
                    state.language = match choice { 1 => Language::Korean, 2 => Language::Japanese, _ => Language::English };
                    state.app_state.language = Some(state.language);
                    save_app_state(&state.app_state)?;
                }
            }
            Some(6) => loop {
                let subtitle = vec![
                    "Aesthetic tuning changes only affect total-score weighting.".to_string(),
                    "Set axis weight with 0+ numbers. Empty keeps previous value.".to_string(),
                ];
                let mut items: Vec<String> = AXIS_DEFINITIONS
                    .iter()
                    .map(|axis| format!("{} ({:.1})", axis.label, get_axis_weight(&state.axis_weights, axis.key)))
                    .collect();
                items.push("Reset to defaults".to_string());
                items.push("Back".to_string());
                match select_menu("Aesthetic Tuning", &subtitle, &items, 0)? {
                    Some(index) if index < AXIS_DEFINITIONS.len() => {
                        let axis = &AXIS_DEFINITIONS[index];
                        let current = get_axis_weight(&state.axis_weights, axis.key);
                        let value = ui::text_input(
                            &format!("{} Weight", axis.label),
                            "Enter a non-negative number (empty = keep current)",
                            &format!("{:.1}", current),
                            false,
                        )?;
                        let Some(raw) = value else { continue; };
                        if raw.is_empty() { continue; }
                        let weight: f32 = raw.parse().map_err(|_| anyhow!("Invalid weight: {raw}"))?;
                        if !weight.is_finite() || weight < 0.0 {
                            bail!("Axis weight must be a finite non-negative number.");
                        }
                        set_axis_weight(&mut state.axis_weights, axis.key, weight);
                        state.app_state.axis_weights = state.axis_weights.clone();
                        save_app_state(&state.app_state)?;
                    }
                    Some(index) if index == AXIS_DEFINITIONS.len() => {
                        state.axis_weights.clear();
                        state.app_state.axis_weights.clear();
                        save_app_state(&state.app_state)?;
                    }
                    _ => break,
                }
            },
            _ => return Ok(()),
        }
    }
}

fn run_share_menu(state: &mut SessionState) -> Result<()> {
    let Some(result) = state.last_result.as_ref() else {
        let _ = select_menu("Share", &["No latest result to share.".to_string()], &["Back".to_string()], 0)?;
        return Ok(());
    };
    let bundle = generate_share_bundle(result, &state.out_dir)?;
    loop {
        let subtitle = vec![
            format!("Share folder: {}", bundle.directory),
            "Choosing a platform copies the caption to clipboard first.".to_string(),
        ];
        let mut items = bundle.assets.iter().map(|asset| asset.platform.clone()).collect::<Vec<_>>();
        items.push("Open share folder".to_string());
        items.push("Back".to_string());

        match select_menu("Share Latest Result", &subtitle, &items, 0)? {
            Some(index) if index < bundle.assets.len() => {
                let asset = &bundle.assets[index];
                let _ = write_clipboard_text(&asset.caption);
                if let Some(url) = &asset.open_url {
                    let _ = Command::new("open").arg(url).status();
                } else {
                    open_path(PathBuf::from(&asset.image_path).as_path())?;
                }
            }
            Some(index) if index == bundle.assets.len() => {
                open_path(PathBuf::from(&bundle.directory).as_path())?;
            }
            _ => return Ok(()),
        }
    }
}

async fn run_interactive_app() -> Result<()> {
    let mut state = SessionState::new();

    // Show animated splash screen before main menu
    if let Ok(star_pressed) = ui::splash_screen(state.app_state.star_acknowledged) {
        if star_pressed {
            let star_url = "https://github.com/NomaDamas/BetterThanYou";
            let _ = Command::new("open").arg(star_url).status();
            state.app_state.star_acknowledged = true;
            let _ = save_app_state(&state.app_state);
        }
    }

    loop {
        let lang = state.language;
        let mut items = vec![
            t(lang, "start_battle").to_string(),
            t(lang, "open_report").to_string(),
            t(lang, "share_result").to_string(),
            t(lang, "settings").to_string(),
        ];
        if !state.app_state.star_acknowledged {
            items.push(t(lang, "star_github").to_string());
        }
        items.push(t(lang, "quit").to_string());

        let mut subtitle = vec![
            "Drop two portraits, get a winner-first battle card, then decide what to do next.".to_string(),
            format!("Judge: {}", state.judge.as_str()),
            format!("Model: {}", state.model),
            format!("Output: {}", state.out_dir.display()),
        ];
        if !state.app_state.star_acknowledged {
            subtitle.push(String::new());
            subtitle.push("\u{2B50} Star one click = dev gets power-up!  github.com/NomaDamas/BetterThanYou".to_string());
        }

        match select_menu("BetterThanYou", &subtitle, &items, 0)? {
            Some(0) => {
                if state.left.is_none() || state.right.is_none() {
                    match ui::battle_input_screen(
                        state.left.as_deref(),
                        state.right.as_deref(),
                    )? {
                        Some((left, right)) => {
                            state.left = Some(left);
                            state.right = Some(right);
                        }
                        None => continue, // User pressed ESC
                    }
                }
                match state.judge {
                    JudgeCli::Openai => ensure_openai_ready(&mut state)?,
                    _ => {}
                }
                let args = BattleArgs {
                    left: state.left.clone(),
                    right: state.right.clone(),
                    left_label: state.left_label.clone(),
                    right_label: state.right_label.clone(),
                    out_dir: Some(state.out_dir.clone()),
                    left_clipboard: false,
                    right_clipboard: false,
                    judge: state.judge.clone(),
                    model: state.model.clone(),
                    json: false,
                    open: false,
                    no_app: false,
                    axis_weights: state.axis_weights.iter().map(|(k, v)| format!("{}={}", k, v)).collect(),
                };
                // Set API keys as env vars
                if let Some(key) = &state.app_state.anthropic_api_key {
                    std::env::set_var("ANTHROPIC_API_KEY", key);
                }
                if let Some(key) = &state.app_state.gemini_api_key {
                    std::env::set_var("GEMINI_API_KEY", key);
                }

                // Show a simple "analyzing" screen, run analysis, then show result
                // Use a background thread for animation + main thread for async analysis
                let done = Arc::new(AtomicBool::new(false));
                let done_anim = done.clone();
                let left_p = state.left.clone().unwrap_or_default();
                let right_p = state.right.clone().unwrap_or_default();

                // Start animation in background OS thread (NOT tokio thread)
                let anim_thread = std::thread::spawn(move || {
                    let _ = ui::battle_loading_screen(&left_p, &right_p, done_anim);
                });

                // Run analysis on main async thread
                let analysis_result = battle_from_args(&args, Some(&state)).await;

                // Stop animation
                done.store(true, Ordering::Relaxed);
                // Wait for animation thread to fully exit and restore terminal
                let _ = anim_thread.join();
                // Ensure terminal is fully reset after animation thread's TuiSession drop
                let _ = crossterm::terminal::disable_raw_mode();
                let _ = crossterm::execute!(
                    io::stdout(),
                    crossterm::terminal::LeaveAlternateScreen,
                    crossterm::cursor::Show
                );
                // Brief pause to let terminal settle after mode transitions
                std::thread::sleep(std::time::Duration::from_millis(100));
                // Drain any stale events from the loading animation
                let _ = crossterm::terminal::enable_raw_mode();
                while crossterm::event::poll(std::time::Duration::from_millis(1)).unwrap_or(false) {
                    let _ = crossterm::event::read();
                }
                let _ = crossterm::terminal::disable_raw_mode();

                let (result, artifacts) = match analysis_result {
                    Ok(r) => r,
                    Err(e) => {
                        let msg = format!("{}", e);
                        let _ = select_menu("Battle Failed", &[msg], &["Back".to_string()], 0);
                        state.left = None;
                        state.right = None;
                        continue;
                    }
                };
                state.last_pair = Some((state.left.clone().unwrap_or_default(), state.right.clone().unwrap_or_default()));
                state.last_json = Some(PathBuf::from(&artifacts.json_path));
                state.last_html = Some(PathBuf::from(&artifacts.html_path));
                state.last_result = Some(result.clone());
                if let Err(_) = ui::present_battle_view(&result, &artifacts, &["Enter/q return".to_string(), "o open report".to_string()], None) {
                    // TUI error - continue to menu anyway
                }

                loop {
                    let lang = state.language;
                    let mut next_items = vec![
                        t(lang, "rematch").to_string(),
                        t(lang, "new_portraits").to_string(),
                        t(lang, "share_result").to_string(),
                        t(lang, "open_report").to_string(),
                        t(lang, "settings").to_string(),
                    ];
                    if !state.app_state.star_acknowledged {
                        next_items.push(t(lang, "star_github").to_string());
                    }
                    next_items.push(t(lang, "back").to_string());
                    next_items.push(t(lang, "quit").to_string());

                    let next_subtitle = vec![
                        format!("Winner: {}", result.winner.label),
                        format!("Judge: {}", state.judge.as_str()),
                        format!("HTML: {}", artifacts.html_path),
                    ];

                    match select_menu("What next?", &next_subtitle, &next_items, 0)? {
                        Some(0) => break,
                        Some(1) => {
                            state.left = None;
                            state.right = None;
                            break;
                        }
                        Some(2) => run_share_menu(&mut state)?,
                        Some(3) => {
                            let path = state.last_html.clone().unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                            open_path(&path)?;
                        }
                        Some(4) => run_settings_menu(&mut state)?,
                        Some(5) if !state.app_state.star_acknowledged => {
                            let star_url = "https://github.com/NomaDamas/BetterThanYou";
                            let _ = Command::new("open").arg(star_url).status();
                            state.app_state.star_acknowledged = true;
                            save_app_state(&state.app_state)?;
                        }
                        Some(index) if (!state.app_state.star_acknowledged && index == 6) || (state.app_state.star_acknowledged && index == 5) => break,
                        _ => return Ok(()),
                    }
                }
            }
            Some(1) => {
                let path = state.last_html.clone().unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                open_path(&path)?;
            }
            Some(2) => run_share_menu(&mut state)?,
            Some(3) => run_settings_menu(&mut state)?,
            Some(4) if !state.app_state.star_acknowledged => {
                let star_url = "https://github.com/NomaDamas/BetterThanYou";
                let _ = Command::new("open").arg(star_url).status();
                state.app_state.star_acknowledged = true;
                save_app_state(&state.app_state)?;
            }
            _ => return Ok(()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Battle(args)) => run_battle(args).await,
        Some(Commands::Report(args)) => run_report(args).await,
        Some(Commands::Open(args)) => run_open(args),
        None => {
            let app_state = load_app_state();
            maybe_print_star_reminder(&app_state);
            let has_no_args = cli.left.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
                && cli.right.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
                && cli.left_label.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
                && cli.right_label.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
                && !cli.left_clipboard
                && !cli.right_clipboard
                && !cli.json
                && !cli.open
                && !cli.no_app;

            if has_no_args {
                return run_interactive_app().await;
            }

            let args = BattleArgs {
                left: cli.left.and_then(|value| {
                    let trimmed = value.trim().to_string();
                    (!trimmed.is_empty()).then_some(trimmed)
                }),
                right: cli.right.and_then(|value| {
                    let trimmed = value.trim().to_string();
                    (!trimmed.is_empty()).then_some(trimmed)
                }),
                left_label: cli.left_label.and_then(|value| {
                    let trimmed = value.trim().to_string();
                    (!trimmed.is_empty()).then_some(trimmed)
                }),
                right_label: cli.right_label.and_then(|value| {
                    let trimmed = value.trim().to_string();
                    (!trimmed.is_empty()).then_some(trimmed)
                }),
                out_dir: cli.out_dir,
                left_clipboard: cli.left_clipboard,
                right_clipboard: cli.right_clipboard,
                judge: cli.judge,
                model: cli.model,
                json: cli.json,
                open: cli.open,
                no_app: cli.no_app,
                axis_weights: cli.axis_weights,
            };
            run_battle(args).await
        }
    }
}
