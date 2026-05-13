mod ui;

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use better_than_you::{
    analyze_portrait_battle, check_latest_release_version, clear_all_reports, default_reports_dir,
    generate_share_bundle, is_newer_version, open_path, prune_old_reports,
    publish_share_bundle_to_web, read_clipboard_text, regenerate_battle_report,
    render_open_summary, render_report_summary, render_terminal_battle, save_battle_artifacts,
    serve_reports_blocking, share_clipboard_text, t, write_clipboard_text, AnalyzeOptions,
    BattleResult, JudgeMode, Language, PublishedShareBundle, ANTHROPIC_VLM_MODELS,
    AXIS_DEFINITIONS, GEMINI_VLM_MODELS, GROK_VLM_MODELS, OPENAI_VLM_MODELS, REPORTS_KEEP_RECENT,
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
    Grok,
}

impl From<JudgeCli> for JudgeMode {
    fn from(value: JudgeCli) -> Self {
        match value {
            JudgeCli::Auto => JudgeMode::Auto,
            JudgeCli::Heuristic => JudgeMode::Heuristic,
            JudgeCli::Openai => JudgeMode::Openai,
            JudgeCli::Anthropic => JudgeMode::Anthropic,
            JudgeCli::Gemini => JudgeMode::Gemini,
            JudgeCli::Grok => JudgeMode::Grok,
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
            Self::Grok => "grok",
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "better-than-you",
    version,
    about = "CLI-first portrait battle tool"
)]
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
    /// Skip the automatic GitHub release version check + self-update on startup.
    #[arg(long)]
    no_update: bool,
    #[arg(long = "axis-weight", value_name = "KEY=WEIGHT")]
    axis_weights: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Battle(BattleArgs),
    Report(ReportArgs),
    Open(OpenArgs),
    /// Serve the reports directory over HTTP on your LAN for phone viewing.
    Serve(ServeArgs),
    /// Publish the latest or specified battle report to a public web URL.
    Publish(PublishArgs),
}

#[derive(Parser, Debug)]
struct ServeArgs {
    /// Directory to serve. Defaults to the configured reports directory.
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// Port to bind. Defaults to 8080.
    #[arg(long, default_value_t = 8080)]
    port: u16,
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

#[derive(Parser, Debug)]
struct PublishArgs {
    /// Battle JSON path. Defaults to latest-battle.json in the reports directory.
    battle_json_path: Option<PathBuf>,
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// Copy the public share URL to the clipboard.
    #[arg(long)]
    copy: bool,
    /// Open the public share page after publishing.
    #[arg(long)]
    open: bool,
    /// Print structured JSON instead of the terminal summary.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppState {
    star_acknowledged: bool,
    #[serde(default)]
    star_ack_source: Option<String>,
    openai_api_key: Option<String>,
    #[serde(default)]
    anthropic_api_key: Option<String>,
    #[serde(default)]
    gemini_api_key: Option<String>,
    #[serde(default)]
    grok_api_key: Option<String>,
    #[serde(default)]
    publish_url: Option<String>,
    #[serde(default)]
    publish_token: Option<String>,
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
        apply_publish_config_from_state(&app_state);
        Self {
            judge: app_state.judge.clone().unwrap_or(JudgeCli::Auto),
            model: app_state
                .model
                .clone()
                .unwrap_or_else(|| "gpt-5.4-mini".to_string()),
            out_dir: app_state
                .out_dir
                .clone()
                .unwrap_or_else(default_reports_dir),
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
    let Some(path) = app_state_path() else {
        return AppState::default();
    };
    let Ok(bytes) = fs::read(path) else {
        return AppState::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_app_state(state: &AppState) -> Result<()> {
    let Some(path) = app_state_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

fn default_publish_url() -> &'static str {
    "https://better-than-you.nomadamas.org"
}

fn effective_publish_url(state: &AppState) -> String {
    state
        .publish_url
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("BTYU_PUBLISH_URL").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_publish_url().to_string())
}

fn publish_token_configured(state: &AppState) -> bool {
    state
        .publish_token
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        || [
            "BTYU_PUBLISH_TOKEN",
            "BTYU_CLOUDFLARE_PUBLISH_TOKEN",
            "CLOUDFLARE_PUBLISH_TOKEN",
            "PUBLISH_TOKEN",
        ]
        .iter()
        .any(|key| {
            std::env::var(key)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
}

fn apply_publish_config_from_state(state: &AppState) {
    if let Some(url) = state
        .publish_url
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        std::env::set_var("BTYU_PUBLISH_URL", url);
    }
    if let Some(token) = state
        .publish_token
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        std::env::set_var("BTYU_PUBLISH_TOKEN", token);
    }
}

fn star_acknowledged(state: &AppState) -> bool {
    state.star_acknowledged && matches!(state.star_ack_source.as_deref(), Some("gh" | "web"))
}

fn star_repo_via_gh() -> bool {
    Command::new("gh")
        .env("GH_PROMPT_DISABLED", "1")
        .args(["repo", "star", "NomaDamas/BetterThanYou", "--yes"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn open_star_repo_page() {
    let star_url = "https://github.com/NomaDamas/BetterThanYou";
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(opener).arg(star_url).status();
}

fn handle_star_request(state: &mut AppState) -> Result<bool> {
    if star_repo_via_gh() {
        state.star_acknowledged = true;
        state.star_ack_source = Some("gh".to_string());
        save_app_state(state)?;
        return Ok(true);
    }

    state.star_acknowledged = true;
    state.star_ack_source = Some("web".to_string());
    save_app_state(state)?;
    open_star_repo_page();
    Ok(false)
}

fn remove_file_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn cleanup_runtime_resources(skip_update_temp: bool) {
    let tmp = std::env::temp_dir();

    // This app owns these paths. Do not clean arbitrary Cargo target dirs here:
    // users may run BetterThanYou from inside unrelated Rust projects.
    if !skip_update_temp {
        let _ = fs::remove_dir_all(tmp.join("btyu-update"));
    }

    if let Ok(entries) = fs::read_dir(&tmp) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name.starts_with("btyu-preview-") && name.ends_with(".png") {
                remove_file_if_exists(&path);
            }
        }
    }

    let update_log = tmp.join("btyu-update.log");
    let stale_or_large = fs::metadata(&update_log)
        .ok()
        .map(|meta| {
            let old = meta
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .map(|age| age.as_secs() > 24 * 60 * 60)
                .unwrap_or(false);
            old || meta.len() > 2 * 1024 * 1024
        })
        .unwrap_or(false);
    if stale_or_large {
        remove_file_if_exists(&update_log);
    }
}

/// Check whether `cargo` is reachable so we can drive a self-install. Without
/// it we can only notify the user about a new release.
fn cargo_available() -> bool {
    Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Self-update flow. Called once at the top of `main()`:
/// 1. Skip only if `--no-update` is set.
/// 2. Hit GitHub for the latest release tag.
/// 3. If newer AND cargo is on PATH → run `cargo install --git ... --force`,
///    re-exec the upgraded binary with the same arguments.
/// 4. On any failure (offline, no cargo, install fails), continue silently
///    on the current version so a busted upstream never blocks the user.
async fn auto_update_check(skip: bool) {
    if skip {
        return;
    }

    let Some(latest) = check_latest_release_version().await else {
        return;
    };

    let current = env!("CARGO_PKG_VERSION");
    if !is_newer_version(&latest, current) {
        return;
    }

    println!(
        "\u{1F195}  BetterThanYou v{} is available (you're on v{}).",
        latest, current
    );

    if !cargo_available() {
        eprintln!("   Auto-install needs the Rust toolchain (https://rustup.rs).");
        eprintln!("   Or run: brew upgrade NomaDamas/better-than-you/better-than-you");
        return;
    }

    // Build into a throwaway --root and stream cargo's output into a log file
    // (not /dev/null). The log persists at /tmp/btyu-update.log for any post-
    // mortem the user wants. cargo's stdout/stderr is kept OFF the terminal
    // because compiler warnings would otherwise corrupt the terminal state
    // right before ratatui-image queries it for rendering protocol — that
    // was the "mosaic image" symptom we hit in v0.8.5.
    let temp_root = std::env::temp_dir().join("btyu-update");
    let _ = std::fs::remove_dir_all(&temp_root);
    let log_path = std::env::temp_dir().join("btyu-update.log");
    let log_file = std::fs::File::create(&log_path).ok();
    let log_clone = log_file.as_ref().and_then(|f| f.try_clone().ok());
    let latest_tag = format!("v{}", latest);

    let mut cmd = Command::new("cargo");
    cmd.args([
        "install",
        "--git",
        "https://github.com/NomaDamas/BetterThanYou",
        "--tag",
        latest_tag.as_str(),
        "--root",
        temp_root.to_string_lossy().as_ref(),
        "--force",
        "--quiet",
    ]);
    cmd.env("CARGO_TARGET_DIR", temp_root.join("target"));
    if let Some(f) = log_file {
        cmd.stdout(std::process::Stdio::from(f));
    } else {
        cmd.stdout(std::process::Stdio::null());
    }
    if let Some(f) = log_clone {
        cmd.stderr(std::process::Stdio::from(f));
    } else {
        cmd.stderr(std::process::Stdio::null());
    }

    // Spinner loop while cargo runs — uses simple \r overwrite so we don't
    // emit any ANSI sequences that might confuse a basic tty.
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\u{26A0}  Auto-update failed to spawn cargo: {}", e);
            return;
        }
    };
    let frames = [
        '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}',
        '\u{2827}', '\u{2807}', '\u{280F}',
    ];
    let start = std::time::Instant::now();
    let mut i = 0usize;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) => {
                use std::io::Write as _;
                let elapsed = start.elapsed().as_secs();
                print!(
                    "\r{} Updating BetterThanYou \u{2192} v{}  ({}s)   ",
                    frames[i], latest, elapsed
                );
                let _ = io::stdout().flush();
                i = (i + 1) % frames.len();
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            Err(e) => break Err(e),
        }
    };
    // Clear the spinner line.
    print!("\r{}\r", " ".repeat(70));
    let _ = io::stdout().flush();

    if !matches!(status, Ok(ref s) if s.success()) {
        eprintln!(
            "\u{26A0}  Auto-update build failed; continuing on current v{}.",
            current
        );
        // Show the tail of the log so the user can see WHY it failed
        // without having to dig for the file themselves.
        if let Ok(text) = std::fs::read_to_string(&log_path) {
            let tail: Vec<&str> = text.lines().rev().take(20).collect();
            if !tail.is_empty() {
                eprintln!("   Last lines from {}:", log_path.display());
                for line in tail.iter().rev() {
                    eprintln!("   | {}", line);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&temp_root);
        return;
    }

    let new_binary = temp_root.join("bin").join("better-than-you");
    if !new_binary.exists() {
        eprintln!("\u{26A0}  Update build finished but binary missing; continuing.");
        return;
    }

    // Resolve the running binary through any symlinks (e.g.
    // /opt/homebrew/bin/better-than-you → Cellar/.../bin/better-than-you).
    let current_exe = match std::env::current_exe() {
        Ok(p) => p.canonicalize().unwrap_or(p),
        Err(_) => {
            eprintln!("\u{26A0}  Could not locate current binary; aborting in-place update.");
            return;
        }
    };

    // Atomic-ish in-place swap: copy new binary to a sibling temp, then rename
    // over the original. macOS allows renaming over an open executable.
    let staging = current_exe.with_extension("new-update");
    if let Err(e) = std::fs::copy(&new_binary, &staging) {
        eprintln!(
            "\u{26A0}  Could not stage new binary at {}: {} — install left in {}.",
            staging.display(),
            e,
            new_binary.display()
        );
        return;
    }
    // Make sure the staged file is executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&staging) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = std::fs::set_permissions(&staging, perms);
        }
    }
    if let Err(e) = std::fs::rename(&staging, &current_exe) {
        eprintln!(
            "\u{26A0}  Could not replace {}: {}\n   New binary kept at {}; copy it manually if needed.",
            current_exe.display(),
            e,
            staging.display()
        );
        return;
    }

    println!(
        "\u{2728} Updated {} \u{2192} v{} (build log: {}) — relaunching...",
        current_exe.display(),
        latest,
        log_path.display()
    );
    let _ = std::fs::remove_dir_all(&temp_root);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let _err = Command::new(&current_exe).args(&args).exec();
        eprintln!("\u{26A0}  Re-exec failed: {:?}", _err);
    }
    #[cfg(not(unix))]
    {
        eprintln!(
            "Update installed at {}. Re-run `better-than-you`.",
            current_exe.display()
        );
        std::process::exit(0);
    }
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
    let weight = value
        .parse::<f32>()
        .map_err(|_| format!("invalid axis weight value for {}: {value}", key))?;
    if !weight.is_finite() || weight < 0.0 {
        return Err(format!(
            "axis-weight must be finite and non-negative for {key}"
        ));
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

fn resolve_sources(
    left: Option<String>,
    right: Option<String>,
    left_clipboard: bool,
    right_clipboard: bool,
) -> Result<(String, String)> {
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
            left_value = Some(prompt_line(
                "Drag or paste LEFT portrait path/URL/data URL: ",
            )?);
        }
        if right_value.is_none() {
            right_value = Some(prompt_line(
                "Drag or paste RIGHT portrait path/URL/data URL: ",
            )?);
        }
    }

    match (left_value, right_value) {
        (Some(left), Some(right)) if !left.is_empty() && !right.is_empty() => Ok((left, right)),
        _ => bail!("Two portrait inputs are required."),
    }
}

fn maybe_print_star_reminder(state: &AppState) {
    if !star_acknowledged(state) && !io::stdout().is_terminal() {
        eprintln!("\u{2B50} Star one click = dev gets power-up!  https://github.com/NomaDamas/BetterThanYou");
    }
}

fn select_menu(
    title: &str,
    subtitle: &[String],
    items: &[String],
    initial_index: usize,
) -> Result<Option<usize>> {
    match ui::select_menu(title, subtitle, items, initial_index) {
        Ok(result) => Ok(result),
        Err(_) => Ok(Some(initial_index.min(items.len().saturating_sub(1)))),
    }
}

fn judge_index(judge: &JudgeCli) -> usize {
    match judge {
        JudgeCli::Auto => 0,
        JudgeCli::Openai => 1,
        JudgeCli::Anthropic => 2,
        JudgeCli::Gemini => 3,
        JudgeCli::Grok => 4,
        JudgeCli::Heuristic => 5,
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
            if let Some(key) =
                ui::text_input("OpenAI API Key", "Paste your API key (sk-...)", "", true)?
            {
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

async fn battle_from_args(
    args: &BattleArgs,
    state: Option<&SessionState>,
) -> Result<(BattleResult, better_than_you::SavedArtifacts)> {
    let (left_source, right_source) = resolve_sources(
        args.left.clone(),
        args.right.clone(),
        args.left_clipboard,
        args.right_clipboard,
    )?;
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
        if let Some(key) = &session_state.app_state.grok_api_key {
            std::env::set_var("XAI_API_KEY", key);
        }
    }

    let result = analyze_portrait_battle(options).await?;
    let artifacts = save_battle_artifacts(&result, &output_dir)?;
    Ok((result, artifacts))
}

async fn run_battle(args: BattleArgs) -> Result<()> {
    let (result, artifacts) = battle_from_args(&args, None).await?;
    let html_path = PathBuf::from(&artifacts.html_path);

    if args.open {
        open_path(html_path.as_path())?;
    }

    if args.json {
        let payload = serde_json::json!({
            "result": result,
            "artifacts": artifacts,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if !args.no_app && io::stdin().is_terminal() && io::stdout().is_terminal() {
        let exit = ui::present_battle_view(
            &result,
            &artifacts,
            &["Enter/q return".to_string(), "o open report".to_string()],
        )?;
        if matches!(exit, ui::BattleViewExit::OpenRequest) {
            handle_open_request(&html_path).await?;
        }
    } else {
        println!(
            "{}",
            render_terminal_battle(&result, &artifacts, io::stdout().is_terminal())
        );
    }

    Ok(())
}

fn persist_published_share(output_dir: &Path, published: &PublishedShareBundle) -> Result<()> {
    fs::write(
        output_dir.join("latest-published.json"),
        serde_json::to_vec_pretty(published)?,
    )?;
    Ok(())
}

async fn publish_current_share(
    result: &BattleResult,
    html_path: &Path,
    output_dir: &Path,
) -> Result<PublishedShareBundle> {
    let published = publish_share_bundle_to_web(result, html_path, output_dir).await?;
    persist_published_share(output_dir, &published)?;
    Ok(published)
}

async fn handle_open_request(html_path: &Path) -> Result<()> {
    open_path(html_path)?;
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
        println!(
            "{}",
            render_report_summary(&artifacts, io::stdout().is_terminal())
        );
    }
    Ok(())
}

async fn run_open(args: OpenArgs) -> Result<()> {
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);

    // Explicit target — always honor, even if local. Caller asked for that file.
    if let Some(target) = args.target {
        open_path(&target)?;
        println!(
            "{}",
            render_open_summary(&target, io::stdout().is_terminal())
        );
        return Ok(());
    }

    let html_path = output_dir.join("latest-battle.html");
    if !html_path.exists() {
        bail!("No report to open. Run a battle first: better-than-you battle <left> <right>");
    }
    open_path(&html_path)?;
    println!(
        "{}",
        render_open_summary(&html_path, io::stdout().is_terminal())
    );
    Ok(())
}

fn run_serve(args: ServeArgs) -> Result<()> {
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);
    if !output_dir.exists() {
        bail!("Reports directory does not exist: {}", output_dir.display());
    }
    // This blocks until Ctrl-C.
    let _ = serve_reports_blocking(&output_dir, args.port)?;
    Ok(())
}

async fn run_publish(args: PublishArgs) -> Result<()> {
    let app_state = load_app_state();
    apply_publish_config_from_state(&app_state);
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);
    let battle_json = args
        .battle_json_path
        .unwrap_or_else(|| output_dir.join("latest-battle.json"));
    if !battle_json.exists() {
        bail!(
            "No battle JSON to publish. Run a battle first: better-than-you battle <left> <right>"
        );
    }

    let artifacts = regenerate_battle_report(&battle_json, &output_dir)?;
    let bytes = fs::read(&battle_json)?;
    let result: BattleResult = serde_json::from_slice(&bytes)?;
    let published =
        publish_current_share(&result, &PathBuf::from(&artifacts.html_path), &output_dir).await?;

    if args.copy {
        let _ = write_clipboard_text(&published.share_page_url);
    }
    if args.open {
        open_path(PathBuf::from(&published.share_page_url).as_path())?;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&published)?);
    } else {
        println!("\u{1F310} BetterThanYou public web report");
        println!("  \u{2022} Share page: {}", published.share_page_url);
        println!("  \u{2022} Report    : {}", published.report_url);
        println!("  \u{2022} Preview   : {}", published.preview_image_url);
        println!("  \u{2022} Provider  : {}", published.provider);
        if args.copy {
            println!("  \u{2022} Copied share URL to clipboard");
        }
        println!();
        println!("{}", published.qr_ascii);
    }

    Ok(())
}

fn run_settings_menu(state: &mut SessionState) -> Result<()> {
    loop {
        let subtitle = vec![
            format!("Current judge: {}", state.judge.as_str()),
            format!("Current model: {}", state.model),
            format!(
                "Public share URL: {}",
                effective_publish_url(&state.app_state)
            ),
            format!(
                "Public share token: {}",
                if publish_token_configured(&state.app_state) {
                    "configured"
                } else {
                    "not configured"
                }
            ),
        ];
        let items = vec![
            "Judge mode".to_string(),
            "Model".to_string(),
            "Labels".to_string(),
            "Paste both from clipboard".to_string(),
            "Output directory".to_string(),
            "API keys".to_string(),
            "Public share (Cloudflare)".to_string(),
            "Aesthetic tuning".to_string(),
            "Language".to_string(),
            "Clear reports history".to_string(),
            "Back".to_string(),
        ];
        match select_menu("Settings", &subtitle, &items, 0)? {
            Some(0) => {
                let judge_items = vec![
                    "Auto (recommended)".to_string(),
                    "OpenAI judge".to_string(),
                    "Anthropic judge".to_string(),
                    "Gemini judge".to_string(),
                    "Grok judge".to_string(),
                    "Heuristic judge".to_string(),
                ];
                if let Some(choice) =
                    select_menu("Judge Mode", &[], &judge_items, judge_index(&state.judge))?
                {
                    state.judge = match choice {
                        0 => JudgeCli::Auto,
                        1 => JudgeCli::Openai,
                        2 => JudgeCli::Anthropic,
                        3 => JudgeCli::Gemini,
                        4 => JudgeCli::Grok,
                        _ => JudgeCli::Heuristic,
                    };
                    state.app_state.judge = Some(state.judge.clone());
                    save_app_state(&state.app_state)?;
                }
            }
            Some(1) => {
                let model_list: Vec<String> = match state.judge {
                    JudgeCli::Anthropic => {
                        ANTHROPIC_VLM_MODELS.iter().map(|s| s.to_string()).collect()
                    }
                    JudgeCli::Gemini => GEMINI_VLM_MODELS.iter().map(|s| s.to_string()).collect(),
                    JudgeCli::Grok => GROK_VLM_MODELS.iter().map(|s| s.to_string()).collect(),
                    _ => OPENAI_VLM_MODELS.iter().map(|s| s.to_string()).collect(),
                };
                let mut items_with_custom = model_list.clone();
                items_with_custom.push("Custom (type model name)".to_string());
                let current_index = model_list
                    .iter()
                    .position(|m| m == &state.model)
                    .unwrap_or(0);
                if let Some(choice) =
                    select_menu("Select Model", &[], &items_with_custom, current_index)?
                {
                    if choice < model_list.len() {
                        state.model = model_list[choice].clone();
                    } else if let Some(model) =
                        ui::text_input("Custom Model", "Enter model name", "", false)?
                    {
                        if !model.is_empty() {
                            state.model = model;
                        }
                    }
                    state.app_state.model = Some(state.model.clone());
                    save_app_state(&state.app_state)?;
                }
            }
            Some(2) => {
                if let Some(label) = ui::text_input(
                    "Left Label",
                    "Optional name for the left portrait",
                    state.left_label.as_deref().unwrap_or(""),
                    false,
                )? {
                    state.left_label = if label.is_empty() { None } else { Some(label) };
                }
                if let Some(label) = ui::text_input(
                    "Right Label",
                    "Optional name for the right portrait",
                    state.right_label.as_deref().unwrap_or(""),
                    false,
                )? {
                    state.right_label = if label.is_empty() { None } else { Some(label) };
                }
            }
            Some(3) => {
                let clip = read_clipboard_text()?;
                let parts: Vec<String> = clip
                    .replace('\r', "")
                    .split('\n')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .collect();
                if parts.len() >= 2 {
                    state.left = Some(parts[0].clone());
                    state.right = Some(parts[1].clone());
                }
            }
            Some(4) => {
                if let Some(out) = ui::text_input(
                    "Output Directory",
                    "Path where reports are saved",
                    &state.out_dir.display().to_string(),
                    false,
                )? {
                    if !out.is_empty() {
                        state.out_dir = PathBuf::from(out);
                        state.app_state.out_dir = Some(state.out_dir.clone());
                        save_app_state(&state.app_state)?;
                    }
                }
            }
            Some(5) => {
                let openai_status = if state.app_state.openai_api_key.is_some() {
                    " \u{2714}"
                } else {
                    ""
                };
                let anthropic_status = if state.app_state.anthropic_api_key.is_some() {
                    " \u{2714}"
                } else {
                    ""
                };
                let gemini_status = if state.app_state.gemini_api_key.is_some() {
                    " \u{2714}"
                } else {
                    ""
                };
                let grok_status = if state.app_state.grok_api_key.is_some() {
                    " \u{2714}"
                } else {
                    ""
                };
                let items = vec![
                    format!("Set OpenAI API key{}", openai_status),
                    format!("Set Anthropic API key{}", anthropic_status),
                    format!("Set Gemini API key{}", gemini_status),
                    format!("Set Grok/xAI API key{}", grok_status),
                    "Clear all saved keys".to_string(),
                    "Back".to_string(),
                ];
                match select_menu("API Keys", &[], &items, 0)? {
                    Some(0) => {
                        if let Some(key) = ui::text_input(
                            "OpenAI API Key",
                            "Paste your OpenAI API key (sk-...)",
                            "",
                            true,
                        )? {
                            if !key.is_empty() {
                                state.app_state.openai_api_key = Some(key);
                                save_app_state(&state.app_state)?;
                            }
                        }
                    }
                    Some(1) => {
                        if let Some(key) = ui::text_input(
                            "Anthropic API Key",
                            "Paste your Anthropic API key",
                            "",
                            true,
                        )? {
                            if !key.is_empty() {
                                state.app_state.anthropic_api_key = Some(key);
                                save_app_state(&state.app_state)?;
                            }
                        }
                    }
                    Some(2) => {
                        if let Some(key) =
                            ui::text_input("Gemini API Key", "Paste your Gemini API key", "", true)?
                        {
                            if !key.is_empty() {
                                state.app_state.gemini_api_key = Some(key);
                                save_app_state(&state.app_state)?;
                            }
                        }
                    }
                    Some(3) => {
                        if let Some(key) =
                            ui::text_input("Grok/xAI API Key", "Paste your xAI API key", "", true)?
                        {
                            if !key.is_empty() {
                                state.app_state.grok_api_key = Some(key);
                                save_app_state(&state.app_state)?;
                            }
                        }
                    }
                    Some(4) => {
                        state.app_state.openai_api_key = None;
                        state.app_state.anthropic_api_key = None;
                        state.app_state.gemini_api_key = None;
                        state.app_state.grok_api_key = None;
                        save_app_state(&state.app_state)?;
                    }
                    _ => {}
                }
            }
            Some(6) => {
                let url_default = effective_publish_url(&state.app_state);
                if let Some(url) = ui::text_input(
                    "Public Share URL",
                    "Cloudflare Worker URL, e.g. https://share.example.com",
                    &url_default,
                    false,
                )? {
                    if !url.trim().is_empty() {
                        state.app_state.publish_url =
                            Some(url.trim().trim_end_matches('/').to_string());
                    }
                }
                if let Some(token) = ui::text_input(
                    "Public Share Token",
                    "Paste the Worker PUBLISH_TOKEN secret. Required for friend-share links.",
                    "",
                    true,
                )? {
                    if !token.trim().is_empty() {
                        state.app_state.publish_token = Some(token.trim().to_string());
                    }
                }
                apply_publish_config_from_state(&state.app_state);
                save_app_state(&state.app_state)?;
            }
            Some(8) => {
                let lang_items = vec![
                    "English".to_string(),
                    "한국어".to_string(),
                    "日本語".to_string(),
                ];
                let current = match state.language {
                    Language::English => 0,
                    Language::Korean => 1,
                    Language::Japanese => 2,
                };
                if let Some(choice) = select_menu("Language", &[], &lang_items, current)? {
                    state.language = match choice {
                        1 => Language::Korean,
                        2 => Language::Japanese,
                        _ => Language::English,
                    };
                    state.app_state.language = Some(state.language);
                    save_app_state(&state.app_state)?;
                }
            }
            Some(7) => loop {
                let subtitle = vec![
                    "Aesthetic tuning changes only affect total-score weighting.".to_string(),
                    "Set axis weight with 0+ numbers. Empty keeps previous value.".to_string(),
                ];
                let mut items: Vec<String> = AXIS_DEFINITIONS
                    .iter()
                    .map(|axis| {
                        format!(
                            "{} ({:.1})",
                            axis.label,
                            get_axis_weight(&state.axis_weights, axis.key)
                        )
                    })
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
                        let Some(raw) = value else {
                            continue;
                        };
                        if raw.is_empty() {
                            continue;
                        }
                        let weight: f32 =
                            raw.parse().map_err(|_| anyhow!("Invalid weight: {raw}"))?;
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
            Some(9) => {
                let confirm_items = vec![
                    "Yes, delete all saved reports".to_string(),
                    "Cancel".to_string(),
                ];
                let count_msg = match fs::read_dir(&state.out_dir) {
                    Ok(rd) => rd.flatten().count().to_string(),
                    Err(_) => "?".to_string(),
                };
                let subtitle = vec![
                    format!(
                        "This wipes every saved battle artifact in {}",
                        state.out_dir.display()
                    ),
                    format!(
                        "Currently {} files. The latest-* pointers will be removed too.",
                        count_msg
                    ),
                ];
                if let Some(0) = select_menu("Clear Reports History", &subtitle, &confirm_items, 1)?
                {
                    let n = clear_all_reports(&state.out_dir);
                    state.last_html = None;
                    state.last_json = None;
                    state.last_result = None;
                    let _ = select_menu(
                        "Cleared",
                        &[format!("Deleted {} entries.", n)],
                        &["Back".to_string()],
                        0,
                    );
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn run_share_menu(state: &mut SessionState) -> Result<()> {
    let Some(result) = state.last_result.as_ref() else {
        let _ = select_menu(
            "Share",
            &["No latest result to share.".to_string()],
            &["Back".to_string()],
            0,
        )?;
        return Ok(());
    };
    let Some(html_path) = state.last_html.as_ref() else {
        let _ = select_menu(
            "Share",
            &["No latest HTML report to publish.".to_string()],
            &["Back".to_string()],
            0,
        )?;
        return Ok(());
    };
    let bundle = generate_share_bundle(result, &state.out_dir)?;
    loop {
        let subtitle = vec![
            format!("Share folder: {}", bundle.directory),
            "Choosing a platform publishes the report, copies public links, then opens the platform.".to_string(),
        ];
        let mut items = bundle
            .assets
            .iter()
            .map(|asset| asset.platform.clone())
            .collect::<Vec<_>>();
        items.push("Open share folder".to_string());
        items.push("Back".to_string());

        match select_menu("Share Latest Result", &subtitle, &items, 0)? {
            Some(index) if index < bundle.assets.len() => {
                let asset = &bundle.assets[index];
                match publish_current_share(result, html_path, &state.out_dir).await {
                    Ok(published) => {
                        let clipboard = share_clipboard_text(
                            &asset.platform,
                            &asset.caption,
                            &published.share_page_url,
                            &published.preview_image_url,
                            &published.report_url,
                        );
                        let _ = write_clipboard_text(&clipboard);
                        let platform_url = published
                            .social_links
                            .iter()
                            .find(|link| link.platform == asset.platform)
                            .and_then(|link| link.share_url.as_deref())
                            .or(asset.open_url.as_deref())
                            .unwrap_or(&published.share_page_url);
                        open_path(PathBuf::from(platform_url).as_path())?;

                        let summary = vec![
                            format!("Share page: {}", published.share_page_url),
                            format!("Report: {}", published.report_url),
                            format!("Preview: {}", published.preview_image_url),
                            "Public share text copied to clipboard.".to_string(),
                        ];
                        let _ =
                            select_menu("Published SNS Share", &summary, &["Back".to_string()], 0);
                    }
                    Err(error) => {
                        let _ = select_menu(
                            "Publish Failed",
                            &[format!("{}", error)],
                            &["Back".to_string()],
                            0,
                        );
                    }
                }
            }
            Some(index) if index == bundle.assets.len() => {
                open_path(PathBuf::from(&bundle.directory).as_path())?;
            }
            _ => return Ok(()),
        }
    }
}

async fn run_publish_web_menu(state: &mut SessionState) -> Result<()> {
    let Some(result) = state.last_result.as_ref() else {
        let _ = select_menu(
            "Public Web Share",
            &["No latest result to publish.".to_string()],
            &["Back".to_string()],
            0,
        )?;
        return Ok(());
    };
    let Some(html_path) = state.last_html.as_ref() else {
        let _ = select_menu(
            "Public Web Share",
            &["No latest HTML report to publish.".to_string()],
            &["Back".to_string()],
            0,
        )?;
        return Ok(());
    };

    let subtitle = vec![
        "Publishing report + preview image to the configured Cloudflare endpoint first.".to_string(),
        "If a publish token is configured, all shared URLs stay on the Cloudflare domain or the publish fails visibly.".to_string(),
    ];
    let proceed = select_menu(
        "Public Web Share",
        &subtitle,
        &["Publish now".to_string(), "Cancel".to_string()],
        0,
    )?;
    if !matches!(proceed, Some(0)) {
        return Ok(());
    }

    match publish_share_bundle_to_web(result, html_path, &state.out_dir).await {
        Ok(published) => {
            persist_published_share(&state.out_dir, &published)?;
            let _ = write_clipboard_text(&published.share_page_url);
            let items = vec![
                "Open public share page".to_string(),
                "Open public report".to_string(),
                "Back".to_string(),
            ];
            let summary = vec![
                format!("Share page: {}", published.share_page_url),
                format!("Report: {}", published.report_url),
                format!("Preview: {}", published.preview_image_url),
                format!("Provider: {}", published.provider),
                "Share URL copied to clipboard.".to_string(),
            ];
            match select_menu("Published", &summary, &items, 0)? {
                Some(0) => open_path(PathBuf::from(&published.share_page_url).as_path())?,
                Some(1) => open_path(PathBuf::from(&published.report_url).as_path())?,
                _ => {}
            }
        }
        Err(error) => {
            let _ = select_menu(
                "Publish Failed",
                &[format!("{}", error)],
                &["Back".to_string()],
                0,
            );
        }
    }
    Ok(())
}

async fn run_interactive_app() -> Result<()> {
    let mut state = SessionState::new();

    // Show animated splash screen before main menu
    if let Ok(star_pressed) = ui::splash_screen(star_acknowledged(&state.app_state)) {
        if star_pressed {
            let _ = handle_star_request(&mut state.app_state);
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
        if !star_acknowledged(&state.app_state) {
            items.push(t(lang, "star_github").to_string());
        }
        items.push(t(lang, "quit").to_string());

        let no_keys = state.app_state.openai_api_key.is_none()
            && state.app_state.anthropic_api_key.is_none()
            && state.app_state.gemini_api_key.is_none()
            && state.app_state.grok_api_key.is_none()
            && std::env::var("OPENAI_API_KEY").is_err()
            && std::env::var("ANTHROPIC_API_KEY").is_err()
            && std::env::var("GEMINI_API_KEY").is_err()
            && std::env::var("XAI_API_KEY").is_err()
            && std::env::var("GROK_API_KEY").is_err();

        let mut subtitle = vec![
            format!("BetterThanYou v{}", env!("CARGO_PKG_VERSION")),
            "Drop two portraits, get a winner-first battle card, then decide what to do next."
                .to_string(),
            format!("Judge: {}", state.judge.as_str()),
            format!("Model: {}", state.model),
            format!("Output: {}", state.out_dir.display()),
        ];
        if no_keys {
            subtitle.push(String::new());
            subtitle.push("\u{26A0}  No API key configured.".to_string());
            subtitle.push(
                "   Go to Settings -> API Keys to add OpenAI / Anthropic / Gemini / Grok."
                    .to_string(),
            );
            subtitle.push(
                "   Until then, battles run with the local heuristic judge only.".to_string(),
            );
        }
        if !star_acknowledged(&state.app_state) {
            subtitle.push(String::new());
            subtitle.push(
                "\u{2B50} Star one click = dev gets power-up!  github.com/NomaDamas/BetterThanYou"
                    .to_string(),
            );
        }

        match select_menu("BetterThanYou", &subtitle, &items, 0)? {
            Some(0) => {
                if state.left.is_none() || state.right.is_none() {
                    match ui::battle_input_screen(state.left.as_deref(), state.right.as_deref())? {
                        Some((left, right)) => {
                            state.left = Some(left);
                            state.right = Some(right);
                            // Optional per-portrait label customization right
                            // after path entry. Empty input → fall back to
                            // filename stem (handled in load_portrait).
                            let left_default = state.left_label.clone().unwrap_or_default();
                            if let Some(label) = ui::text_input(
                                "Left fighter name (optional)",
                                "Press Enter to use the filename, or type a custom name",
                                &left_default,
                                false,
                            )? {
                                let trimmed = label.trim().to_string();
                                state.left_label = if trimmed.is_empty() {
                                    None
                                } else {
                                    Some(trimmed)
                                };
                            }
                            let right_default = state.right_label.clone().unwrap_or_default();
                            if let Some(label) = ui::text_input(
                                "Right fighter name (optional)",
                                "Press Enter to use the filename, or type a custom name",
                                &right_default,
                                false,
                            )? {
                                let trimmed = label.trim().to_string();
                                state.right_label = if trimmed.is_empty() {
                                    None
                                } else {
                                    Some(trimmed)
                                };
                            }
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
                    axis_weights: state
                        .axis_weights
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect(),
                };
                // Set API keys as env vars
                if let Some(key) = &state.app_state.anthropic_api_key {
                    std::env::set_var("ANTHROPIC_API_KEY", key);
                }
                if let Some(key) = &state.app_state.gemini_api_key {
                    std::env::set_var("GEMINI_API_KEY", key);
                }
                if let Some(key) = &state.app_state.grok_api_key {
                    std::env::set_var("XAI_API_KEY", key);
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

                // Handle the error case first — we need to stop the loading
                // screen before showing select_menu.
                let (result, artifacts) = match analysis_result {
                    Ok(r) => r,
                    Err(e) => {
                        done.store(true, Ordering::Relaxed);
                        let _ = anim_thread.join();
                        let _ = crossterm::terminal::disable_raw_mode();
                        let _ = crossterm::execute!(
                            io::stdout(),
                            crossterm::terminal::LeaveAlternateScreen,
                            crossterm::cursor::Show
                        );
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        let msg = format!("{}", e);
                        let _ = select_menu("Battle Failed", &[msg], &["Back".to_string()], 0);
                        state.left = None;
                        state.right = None;
                        continue;
                    }
                };

                state.last_pair = Some((
                    state.left.clone().unwrap_or_default(),
                    state.right.clone().unwrap_or_default(),
                ));
                state.last_json = Some(PathBuf::from(&artifacts.json_path));
                state.last_html = Some(PathBuf::from(&artifacts.html_path));
                state.last_result = Some(result.clone());

                // NOW stop the animation, since we're about to switch to the
                // result view.
                done.store(true, Ordering::Relaxed);
                let _ = anim_thread.join();
                let _ = crossterm::terminal::disable_raw_mode();
                let _ = crossterm::execute!(
                    io::stdout(),
                    crossterm::terminal::LeaveAlternateScreen,
                    crossterm::cursor::Show
                );
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = crossterm::terminal::enable_raw_mode();
                while crossterm::event::poll(std::time::Duration::from_millis(1)).unwrap_or(false) {
                    let _ = crossterm::event::read();
                }
                let _ = crossterm::terminal::disable_raw_mode();

                match ui::present_battle_view(
                    &result,
                    &artifacts,
                    &["Enter/q return".to_string(), "o open report".to_string()],
                ) {
                    Ok(ui::BattleViewExit::OpenRequest) => {
                        handle_open_request(&PathBuf::from(&artifacts.html_path)).await?;
                    }
                    _ => {}
                }

                loop {
                    let lang = state.language;
                    // Build items with explicit keys so we can match by action rather than numeric index.
                    let mut item_keys: Vec<&'static str> = vec![
                        "rematch",
                        "new_portraits",
                        "share_result",
                        "publish_web",
                        "serve_lan",
                        "open_report",
                        "settings",
                    ];
                    if !star_acknowledged(&state.app_state) {
                        item_keys.push("star_github");
                    }
                    item_keys.push("back");
                    item_keys.push("quit");

                    let next_items: Vec<String> =
                        item_keys.iter().map(|k| t(lang, k).to_string()).collect();

                    let next_subtitle = vec![
                        format!("Winner: {}", result.winner.label),
                        format!("Judge: {}", state.judge.as_str()),
                        format!("HTML: {}", artifacts.html_path),
                    ];

                    let choice = match select_menu("What next?", &next_subtitle, &next_items, 0)? {
                        Some(i) => i,
                        None => return Ok(()),
                    };
                    let action = item_keys.get(choice).copied().unwrap_or("quit");

                    match action {
                        "rematch" => break,
                        "new_portraits" => {
                            state.left = None;
                            state.right = None;
                            break;
                        }
                        "share_result" => run_share_menu(&mut state).await?,
                        "publish_web" => run_publish_web_menu(&mut state).await?,
                        "serve_lan" => {
                            let out_dir = state.out_dir.clone();
                            let _ = select_menu(
                                "LAN Serve",
                                &[
                                    "Local server starts on port 8080 after you confirm.".to_string(),
                                    "The TUI exits temporarily — press Ctrl-C in the terminal to stop the server.".to_string(),
                                    "Once stopped, you'll return to this menu.".to_string(),
                                ],
                                &["Start server".to_string(), "Cancel".to_string()],
                                0,
                            )
                            .ok()
                            .flatten()
                            .filter(|i| *i == 0)
                            .map(|_| {
                                if let Err(e) = serve_reports_blocking(&out_dir, 8080) {
                                    let _ = select_menu("Serve Failed", &[format!("{}", e)], &["Back".to_string()], 0);
                                }
                            });
                        }
                        "open_report" => {
                            let path = state
                                .last_html
                                .clone()
                                .unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                            open_path(&path)?;
                        }
                        "settings" => run_settings_menu(&mut state)?,
                        "star_github" => {
                            let _ = handle_star_request(&mut state.app_state)?;
                        }
                        "back" => break,
                        _ => return Ok(()),
                    }
                }
            }
            Some(1) => {
                let path = state
                    .last_html
                    .clone()
                    .unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                open_path(&path)?;
            }
            Some(2) => run_share_menu(&mut state).await?,
            Some(3) => run_settings_menu(&mut state)?,
            Some(4) if !star_acknowledged(&state.app_state) => {
                let _ = handle_star_request(&mut state.app_state)?;
            }
            _ => return Ok(()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Auto-update check FIRST, before any other startup work. We skip during
    // `serve` (would interrupt anyone browsing) and when the user opted out.
    let skip_update = cli.no_update || matches!(cli.command, Some(Commands::Serve(_)));
    cleanup_runtime_resources(skip_update);
    auto_update_check(skip_update).await;

    let app_state = load_app_state();

    // Auto-prune old reports on every invocation. Skips the active `serve`
    // session (deleting files mid-serve would 404 anyone currently browsing).
    if !matches!(cli.command, Some(Commands::Serve(_))) {
        let out_dir = app_state
            .out_dir
            .clone()
            .or_else(|| {
                cli.out_dir.clone().or_else(|| match &cli.command {
                    Some(Commands::Battle(a)) => a.out_dir.clone(),
                    Some(Commands::Report(a)) => a.out_dir.clone(),
                    Some(Commands::Open(a)) => a.out_dir.clone(),
                    Some(Commands::Publish(a)) => a.out_dir.clone(),
                    _ => None,
                })
            })
            .unwrap_or_else(default_reports_dir);
        let _ = prune_old_reports(&out_dir, REPORTS_KEEP_RECENT);
    }

    match cli.command {
        Some(Commands::Battle(args)) => run_battle(args).await,
        Some(Commands::Report(args)) => run_report(args).await,
        Some(Commands::Open(args)) => run_open(args).await,
        Some(Commands::Serve(args)) => run_serve(args),
        Some(Commands::Publish(args)) => run_publish(args).await,
        None => {
            let app_state = load_app_state();
            maybe_print_star_reminder(&app_state);
            let has_no_args = cli
                .left
                .as_ref()
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
                && cli
                    .right
                    .as_ref()
                    .map(|v| v.trim().is_empty())
                    .unwrap_or(true)
                && cli
                    .left_label
                    .as_ref()
                    .map(|v| v.trim().is_empty())
                    .unwrap_or(true)
                && cli
                    .right_label
                    .as_ref()
                    .map(|v| v.trim().is_empty())
                    .unwrap_or(true)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_acknowledged_accepts_terminal_or_web_source() {
        let plain_true = AppState {
            star_acknowledged: true,
            star_ack_source: None,
            ..AppState::default()
        };
        assert!(!star_acknowledged(&plain_true));

        let gh_true = AppState {
            star_acknowledged: true,
            star_ack_source: Some("gh".to_string()),
            ..AppState::default()
        };
        assert!(star_acknowledged(&gh_true));

        let web_true = AppState {
            star_acknowledged: true,
            star_ack_source: Some("web".to_string()),
            ..AppState::default()
        };
        assert!(star_acknowledged(&web_true));
    }
}
