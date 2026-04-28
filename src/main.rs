mod ui;

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Result};
use better_than_you::{
    analyze_portrait_battle, check_latest_release_version, clear_all_reports, default_reports_dir,
    generate_share_bundle, is_newer_version, nomadamas_publish_config, open_path,
    load_battle_result_for_html, prune_old_reports, publish_html_to_web,
    publish_share_bundle_to_web, read_clipboard_text, regenerate_battle_report,
    render_open_summary, render_report_summary, render_terminal_battle, save_battle_artifacts,
    serve_reports_blocking, write_clipboard_text,
    AnalyzeOptions, AXIS_DEFINITIONS, BattleResult, JudgeMode, Language, PublishedShareBundle, t,
    OPENAI_VLM_MODELS, ANTHROPIC_VLM_MODELS, GEMINI_VLM_MODELS, REPORTS_KEEP_RECENT,
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
    #[arg(long)]
    no_publish: bool,
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
    /// Upload the latest (or given) HTML report to a free web host and return a shareable URL.
    Publish(PublishArgs),
    /// Serve the reports directory over HTTP on your LAN for phone viewing.
    Serve(ServeArgs),
}

#[derive(Parser, Debug)]
struct PublishArgs {
    /// Path to the HTML report. Defaults to <out-dir>/latest-battle.html
    target: Option<PathBuf>,
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// Also copy the returned URL to the clipboard.
    #[arg(long)]
    copy: bool,
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
    /// Skip auto-publish to nomadamas.org even when BTYU_PUBLISH_URL/TOKEN are configured.
    #[arg(long)]
    no_publish: bool,
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
    #[serde(default)]
    publish_base_url: Option<String>,
    #[serde(default)]
    publish_token: Option<String>,
    /// Unix timestamp of the last successful update check, to throttle GitHub
    /// API hits to ≤ once every 30 minutes when already up to date.
    #[serde(default)]
    last_update_check: Option<i64>,
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
    last_published_share: Option<PublishedShareBundle>,
    last_published_battle_id: Option<String>,
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
            last_published_share: None,
            last_published_battle_id: None,
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
/// 1. Skip if `--no-update` is set, or if we already checked < 30 min ago.
/// 2. Hit GitHub for the latest release tag (3-second timeout).
/// 3. If newer AND cargo is on PATH → run `cargo install --git ... --force`,
///    re-exec the upgraded binary with the same arguments.
/// 4. On any failure (offline, no cargo, install fails), continue silently
///    on the current version so a busted upstream never blocks the user.
async fn auto_update_check(skip: bool) {
    if skip {
        return;
    }

    // Throttle: only check every 30 minutes.
    let app_state_path = app_state_path();
    let mut state = load_app_state();
    let now = chrono::Utc::now().timestamp();
    if let Some(last) = state.last_update_check {
        if now - last < 1800 {
            return;
        }
    }

    let Some(latest) = check_latest_release_version().await else {
        return;
    };
    state.last_update_check = Some(now);
    if let Some(ref _path) = app_state_path {
        let _ = save_app_state(&state);
    }

    let current = env!("CARGO_PKG_VERSION");
    if !is_newer_version(&latest, current) {
        return;
    }

    println!(
        "\u{1F195}  BetterThanYou v{} is available (you're on v{}).",
        latest, current
    );

    if !cargo_available() {
        eprintln!(
            "   Auto-install needs the Rust toolchain (https://rustup.rs)."
        );
        eprintln!(
            "   Or run: brew upgrade NomaDamas/better-than-you/better-than-you"
        );
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

    let mut cmd = Command::new("cargo");
    cmd.args([
        "install",
        "--git",
        "https://github.com/NomaDamas/BetterThanYou",
        "--root",
        temp_root.to_string_lossy().as_ref(),
        "--force",
        "--quiet",
    ]);
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
    let frames = ['\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}', '\u{2827}', '\u{2807}', '\u{280F}'];
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

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let _err = Command::new(&current_exe).args(&args).exec();
        eprintln!("\u{26A0}  Re-exec failed: {:?}", _err);
    }
    #[cfg(not(unix))]
    {
        eprintln!("Update installed at {}. Re-run `better-than-you`.", current_exe.display());
        std::process::exit(0);
    }
}

/// Export saved publish endpoint config into the process env so `lib.rs::publish_bytes_to_web`
/// can pick it up. Real env vars set by the user always win.
fn apply_publish_env(state: &AppState) {
    if std::env::var("BTYU_PUBLISH_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_none()
    {
        if let Some(url) = state.publish_base_url.as_ref().filter(|v| !v.trim().is_empty()) {
            std::env::set_var("BTYU_PUBLISH_URL", url);
        }
    }
    if std::env::var("BTYU_PUBLISH_TOKEN")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_none()
    {
        if let Some(token) = state.publish_token.as_ref().filter(|v| !v.trim().is_empty()) {
            std::env::set_var("BTYU_PUBLISH_TOKEN", token);
        }
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

/// Sidecar wraps the published bundle with the originating battle_id so the
/// TUI 'o' key can detect whether the cached URL matches the battle on
/// screen. `#[serde(flatten)]` keeps the JSON compatible with the previous
/// flat shape — older sidecars without `battle_id` still parse as a
/// `PublishedShareBundle` for `run_open` (which just wants the latest URL).
#[derive(Serialize, Deserialize)]
struct LatestPublishedSidecar {
    battle_id: String,
    #[serde(flatten)]
    bundle: PublishedShareBundle,
}

/// If a publish endpoint is configured, upload the freshly written report to
/// it and persist a sidecar `latest-published.json` so `open` can later prefer
/// the public URL over the local file. Failures degrade gracefully.
///
/// `verbose=true` is for the non-TUI subcommand path; `verbose=false` keeps
/// stdout clean inside the interactive TUI flow (otherwise prints leak above
/// the alt-screen frame and break the UX).
async fn auto_publish_if_configured(
    result: &BattleResult,
    html_path: &std::path::Path,
    out_dir: &std::path::Path,
    verbose: bool,
) -> Option<PublishedShareBundle> {
    if nomadamas_publish_config().is_none() {
        return None;
    }
    if verbose {
        println!("\u{2601}  Auto-publishing to your share endpoint...");
    }
    match publish_share_bundle_to_web(result, html_path, out_dir).await {
        Ok(bundle) => {
            let sidecar = out_dir.join("latest-published.json");
            let payload = LatestPublishedSidecar {
                battle_id: result.battle_id.clone(),
                bundle: bundle.clone(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&payload) {
                let _ = fs::write(&sidecar, json);
            }
            if verbose {
                println!("\u{2728} Published: {}", bundle.share_page_url);
                println!("   Report  : {}", bundle.report_url);
                println!("   Preview : {}", bundle.preview_image_url);
            }
            Some(bundle)
        }
        Err(e) => {
            if verbose {
                eprintln!("\u{26A0}  Auto-publish failed: {} (kept local copy)", e);
            }
            None
        }
    }
}

async fn run_battle(args: BattleArgs) -> Result<()> {
    let (result, artifacts) = battle_from_args(&args, None).await?;
    let output_dir = args.out_dir.clone().unwrap_or_else(default_reports_dir);

    let html_path = PathBuf::from(&artifacts.html_path);
    let tui_mode = !args.no_app && io::stdin().is_terminal() && io::stdout().is_terminal();
    let published = if !args.no_publish {
        auto_publish_if_configured(&result, &html_path, &output_dir, !tui_mode).await
    } else {
        None
    };

    if args.open {
        match published.as_ref() {
            Some(bundle) => open_external_url(&bundle.share_page_url),
            None => open_path(html_path.as_path())?,
        }
    }

    if args.json {
        let payload = serde_json::json!({
            "result": result,
            "artifacts": artifacts,
            "published": published,
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
            handle_open_request(&result, &html_path, &output_dir).await;
        }
    } else {
        println!("{}", render_terminal_battle(&result, &artifacts, io::stdout().is_terminal()));
    }

    Ok(())
}

/// Honor an `o` keypress from the battle view: prefer the public URL
/// (sidecar → cached → publish-on-demand), only fall back to the local
/// `file://` if publishing isn't configured at all.
async fn handle_open_request(
    result: &BattleResult,
    html_path: &std::path::Path,
    out_dir: &std::path::Path,
) {
    let sidecar = out_dir.join("latest-published.json");
    // Only trust the sidecar when its battle_id matches the battle the user
    // is looking at. Otherwise (no sidecar, parse error, mismatched id) we
    // re-publish on demand for THIS battle. Without this guard, a TUI 'o'
    // press on a battle whose auto-publish failed transiently would open the
    // PREVIOUS battle's URL, which is the bug this commit closes.
    if sidecar.exists() {
        if let Ok(text) = fs::read_to_string(&sidecar) {
            if let Ok(s) = serde_json::from_str::<LatestPublishedSidecar>(&text) {
                if s.battle_id == result.battle_id {
                    open_external_url(&s.bundle.share_page_url);
                    return;
                }
            }
        }
    }
    if nomadamas_publish_config().is_some() {
        if let Some(bundle) = auto_publish_if_configured(result, html_path, out_dir, false).await {
            open_external_url(&bundle.share_page_url);
            return;
        }
    }
    let _ = open_path(html_path);
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

async fn run_open(args: OpenArgs) -> Result<()> {
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);

    // Explicit target — always honor, even if local. Caller asked for that file.
    if let Some(target) = args.target {
        open_path(&target)?;
        println!("{}", render_open_summary(&target, io::stdout().is_terminal()));
        return Ok(());
    }

    let sidecar = output_dir.join("latest-published.json");
    let html_path = output_dir.join("latest-battle.html");

    // 1) Sidecar present → open the public URL.
    if sidecar.exists() {
        if let Ok(text) = fs::read_to_string(&sidecar) {
            if let Ok(bundle) = serde_json::from_str::<PublishedShareBundle>(&text) {
                open_external_url(&bundle.share_page_url);
                println!("Opened public share page: {}", bundle.share_page_url);
                println!("  Report : {}", bundle.report_url);
                return Ok(());
            }
        }
    }

    // 2) No sidecar but we have a battle and publish endpoint → upload now,
    //    save sidecar, open the public URL. Reports must go via Cloudflare.
    if html_path.exists() && nomadamas_publish_config().is_some() {
        if let Ok(result) = load_battle_result_for_html(&html_path) {
            println!("\u{2601}  Publishing to your share endpoint before opening...");
            match publish_share_bundle_to_web(&result, &html_path, &output_dir).await {
                Ok(bundle) => {
                    if let Ok(json) = serde_json::to_string_pretty(&bundle) {
                        let _ = fs::write(&sidecar, json);
                    }
                    open_external_url(&bundle.share_page_url);
                    println!("\u{2728} Published & opened: {}", bundle.share_page_url);
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("\u{26A0}  On-demand publish failed: {} — falling back to local file.", e);
                }
            }
        }
    }

    // 3) Last-resort fallback: local file (only when publish is impossible).
    if !html_path.exists() {
        bail!(
            "No report to open. Run a battle first: better-than-you battle <left> <right>"
        );
    }
    open_path(&html_path)?;
    println!("{}", render_open_summary(&html_path, io::stdout().is_terminal()));
    Ok(())
}

async fn run_publish(args: PublishArgs) -> Result<()> {
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);
    let path = args.target.unwrap_or_else(|| output_dir.join("latest-battle.html"));
    if !path.exists() {
        bail!("Report not found: {}. Run a battle first.", path.display());
    }
    println!("\u{2601}  Uploading {} ...", path.display());
    let rich_publish = load_battle_result_for_html(&path)
        .ok()
        .and_then(|result| Some((result, path.clone())));

    println!();
    if let Some((result, html_path)) = rich_publish {
        let published = publish_share_bundle_to_web(&result, &html_path, &output_dir).await?;
        println!("\u{2728} Published public share bundle!");
        println!("  Share page : {}", published.share_page_url);
        println!("  Report     : {}", published.report_url);
        println!("  Preview    : {}", published.preview_image_url);
        println!("  Hosts      : {}", published.provider);
        println!();
        println!("Scan with your phone camera:");
        println!("{}", published.qr_ascii);
        println!();
        println!("SNS links:");
        for link in &published.social_links {
            match &link.share_url {
                Some(url) => println!("  {}: {}", link.platform, url),
                None => println!("  {}: {}", link.platform, link.note),
            }
        }
        if args.copy {
            let _ = better_than_you::write_clipboard_text(&format!(
                "{}\n\n{}",
                published.caption, published.share_page_url
            ));
            println!("(Caption + public share URL copied to clipboard)");
        }
    } else {
        let published = publish_html_to_web(&path).await?;
        println!("\u{2728} Published successfully!");
        println!("  URL : {}", published.url);
        println!("  Host: {}", published.provider);
        println!();
        println!("Scan with your phone camera:");
        println!("{}", published.qr_ascii);
        if args.copy {
            let _ = better_than_you::write_clipboard_text(&published.url);
            println!("(URL copied to clipboard)");
        }
    }
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

fn open_external_url(url: &str) {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(opener).arg(url).status();
}

async fn ensure_published_share_bundle(state: &mut SessionState) -> Result<PublishedShareBundle> {
    let result = state
        .last_result
        .clone()
        .ok_or_else(|| anyhow!("No latest result to publish."))?;
    if state.last_published_battle_id.as_deref() == Some(result.battle_id.as_str()) {
        if let Some(existing) = state.last_published_share.clone() {
            return Ok(existing);
        }
    }

    let html_path = state
        .last_html
        .clone()
        .unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
    let published = publish_share_bundle_to_web(&result, &html_path, &state.out_dir).await?;
    state.last_published_battle_id = Some(result.battle_id.clone());
    state.last_published_share = Some(published.clone());
    Ok(published)
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
            "Public sharing (nomadamas.org)".to_string(),
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
            Some(8) => loop {
                let url_status = match state.app_state.publish_base_url.as_deref() {
                    Some(v) if !v.trim().is_empty() => v.to_string(),
                    _ => "(not set)".to_string(),
                };
                let token_status = match state.app_state.publish_token.as_deref() {
                    Some(v) if v.len() > 4 => {
                        let last4 = &v[v.len() - 4..];
                        format!("\u{2022}\u{2022}\u{2022}\u{2022}{}", last4)
                    }
                    Some(v) if !v.is_empty() => "\u{2022}\u{2022}\u{2022}\u{2022}".to_string(),
                    _ => "(not set)".to_string(),
                };
                let subtitle = vec![
                    "Used by `publish` to upload reports to your Cloudflare Worker.".to_string(),
                    format!("URL: {}", url_status),
                    format!("Token: {}", token_status),
                ];
                let items = vec![
                    "Set publish URL (BTYU_PUBLISH_URL)".to_string(),
                    "Set publish token (BTYU_PUBLISH_TOKEN)".to_string(),
                    "Clear publish URL".to_string(),
                    "Clear publish token".to_string(),
                    "Back".to_string(),
                ];
                match select_menu("Public Sharing", &subtitle, &items, 0)? {
                    Some(0) => {
                        let current = state.app_state.publish_base_url.clone().unwrap_or_default();
                        if let Some(url) = ui::text_input(
                            "Publish URL",
                            "e.g. https://nomadamas.org (empty = clear)",
                            &current,
                            false,
                        )? {
                            let trimmed = url.trim().trim_end_matches('/').to_string();
                            if trimmed.is_empty() {
                                state.app_state.publish_base_url = None;
                                std::env::remove_var("BTYU_PUBLISH_URL");
                            } else {
                                state.app_state.publish_base_url = Some(trimmed.clone());
                                std::env::set_var("BTYU_PUBLISH_URL", trimmed);
                            }
                            save_app_state(&state.app_state)?;
                        }
                    }
                    Some(1) => {
                        if let Some(token) = ui::text_input(
                            "Publish Token",
                            "Bearer token from your Cloudflare Worker (empty = clear)",
                            "",
                            true,
                        )? {
                            let trimmed = token.trim().to_string();
                            if trimmed.is_empty() {
                                state.app_state.publish_token = None;
                                std::env::remove_var("BTYU_PUBLISH_TOKEN");
                            } else {
                                state.app_state.publish_token = Some(trimmed.clone());
                                std::env::set_var("BTYU_PUBLISH_TOKEN", trimmed);
                            }
                            save_app_state(&state.app_state)?;
                        }
                    }
                    Some(2) => {
                        state.app_state.publish_base_url = None;
                        std::env::remove_var("BTYU_PUBLISH_URL");
                        save_app_state(&state.app_state)?;
                    }
                    Some(3) => {
                        state.app_state.publish_token = None;
                        std::env::remove_var("BTYU_PUBLISH_TOKEN");
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
                    "(The Cloudflare-hosted public copies are NOT touched.)".to_string(),
                ];
                if let Some(0) = select_menu("Clear Reports History", &subtitle, &confirm_items, 1)? {
                    let n = clear_all_reports(&state.out_dir);
                    state.last_html = None;
                    state.last_json = None;
                    state.last_result = None;
                    state.last_published_share = None;
                    state.last_published_battle_id = None;
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
        let _ = select_menu("Share", &["No latest result to share.".to_string()], &["Back".to_string()], 0)?;
        return Ok(());
    };
    let bundle = generate_share_bundle(result, &state.out_dir)?;
    loop {
        let subtitle = vec![
            format!("Share folder: {}", bundle.directory),
            "Choosing a platform copies the caption first and tries to publish a public share page.".to_string(),
        ];
        let mut items = bundle.assets.iter().map(|asset| asset.platform.clone()).collect::<Vec<_>>();
        items.push("Open share folder".to_string());
        items.push("Back".to_string());

        match select_menu("Share Latest Result", &subtitle, &items, 0)? {
            Some(index) if index < bundle.assets.len() => {
                let asset = &bundle.assets[index];
                let published = ensure_published_share_bundle(state).await.ok();

                if let Some(published) = published {
                    let clipboard = better_than_you::share_clipboard_text(
                        &asset.platform,
                        &asset.caption,
                        &published.share_page_url,
                        &published.preview_image_url,
                        &published.report_url,
                    );
                    let _ = write_clipboard_text(&clipboard);

                    if let Some(link) = published
                        .social_links
                        .iter()
                        .find(|link| link.platform == asset.platform)
                    {
                        if let Some(url) = &link.share_url {
                            open_external_url(url);
                        } else if let Some(url) = &asset.open_url {
                            open_external_url(url);
                            open_path(PathBuf::from(&asset.image_path).as_path())?;
                        } else {
                            open_path(PathBuf::from(&asset.image_path).as_path())?;
                        }
                    } else if let Some(url) = &asset.open_url {
                        open_external_url(url);
                    } else {
                        open_path(PathBuf::from(&asset.image_path).as_path())?;
                    }
                } else {
                    let _ = write_clipboard_text(&asset.caption);
                    if let Some(url) = &asset.open_url {
                        open_external_url(url);
                    } else {
                        open_path(PathBuf::from(&asset.image_path).as_path())?;
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

        let no_keys = state.app_state.openai_api_key.is_none()
            && state.app_state.anthropic_api_key.is_none()
            && state.app_state.gemini_api_key.is_none()
            && std::env::var("OPENAI_API_KEY").is_err()
            && std::env::var("ANTHROPIC_API_KEY").is_err()
            && std::env::var("GEMINI_API_KEY").is_err();

        let mut subtitle = vec![
            format!("BetterThanYou v{}", env!("CARGO_PKG_VERSION")),
            "Drop two portraits, get a winner-first battle card, then decide what to do next.".to_string(),
            format!("Judge: {}", state.judge.as_str()),
            format!("Model: {}", state.model),
            format!("Output: {}", state.out_dir.display()),
        ];
        if no_keys {
            subtitle.push(String::new());
            subtitle.push(
                "\u{26A0}  No VLM API key set — running heuristic only.".to_string(),
            );
            subtitle.push(
                "   For richer per-axis VLM analysis, add a key in Settings → API keys.".to_string(),
            );
        }
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
                                state.left_label = if trimmed.is_empty() { None } else { Some(trimmed) };
                            }
                            let right_default = state.right_label.clone().unwrap_or_default();
                            if let Some(label) = ui::text_input(
                                "Right fighter name (optional)",
                                "Press Enter to use the filename, or type a custom name",
                                &right_default,
                                false,
                            )? {
                                let trimmed = label.trim().to_string();
                                state.right_label = if trimmed.is_empty() { None } else { Some(trimmed) };
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
                    no_publish: false,
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
                state.last_published_share = None;
                state.last_published_battle_id = None;

                // Auto-publish if user has set up nomadamas-style sharing.
                // verbose=false so prints don't leak above the TUI alt-screen.
                let html_path_for_publish = PathBuf::from(&artifacts.html_path);
                if let Some(bundle) =
                    auto_publish_if_configured(&result, &html_path_for_publish, &state.out_dir, false).await
                {
                    state.last_published_share = Some(bundle.clone());
                    state.last_published_battle_id = Some(result.battle_id.clone());
                }

                match ui::present_battle_view(
                    &result,
                    &artifacts,
                    &["Enter/q return".to_string(), "o open report".to_string()],
                ) {
                    Ok(ui::BattleViewExit::OpenRequest) => {
                        handle_open_request(
                            &result,
                            &PathBuf::from(&artifacts.html_path),
                            &state.out_dir,
                        )
                        .await;
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
                    if !state.app_state.star_acknowledged {
                        item_keys.push("star_github");
                    }
                    item_keys.push("back");
                    item_keys.push("quit");

                    let next_items: Vec<String> = item_keys.iter().map(|k| t(lang, k).to_string()).collect();

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
                            state.last_published_share = None;
                            state.last_published_battle_id = None;
                            break;
                        }
                        "share_result" => run_share_menu(&mut state).await?,
                        "publish_web" => {
                            let path = state.last_html.clone().unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                            let publish_result = if let Some(result) = state.last_result.clone() {
                                publish_share_bundle_to_web(&result, &path, &state.out_dir).await
                                    .map(|bundle| {
                                        (
                                            bundle.share_page_url.clone(),
                                            bundle.provider.clone(),
                                            bundle.qr_ascii.clone(),
                                            Some(bundle),
                                        )
                                    })
                            } else {
                                publish_html_to_web(&path).await
                                    .map(|published| (published.url, published.provider, published.qr_ascii, None))
                            };
                            match publish_result {
                                Ok(published) => {
                                    let _ = write_clipboard_text(&published.0);
                                    if let Some(bundle) = published.3.clone() {
                                        state.last_published_battle_id = state.last_result.as_ref().map(|result| result.battle_id.clone());
                                        state.last_published_share = Some(bundle);
                                    }
                                    let mut subtitle = vec![
                                        format!("URL: {}", published.0),
                                        format!("Host: {}", published.1),
                                        String::new(),
                                        "(URL copied to clipboard)".to_string(),
                                        String::new(),
                                        "Scan with your phone camera:".to_string(),
                                    ];
                                    for qr_line in published.2.lines() {
                                        subtitle.push(qr_line.to_string());
                                    }
                                    let _ = select_menu("Published", &subtitle, &["OK".to_string()], 0);
                                }
                                Err(e) => {
                                    let _ = select_menu("Publish Failed", &[format!("{}", e)], &["Back".to_string()], 0);
                                }
                            }
                        }
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
                            let path = state.last_html.clone().unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                            open_path(&path)?;
                        }
                        "settings" => run_settings_menu(&mut state)?,
                        "star_github" => {
                            let star_url = "https://github.com/NomaDamas/BetterThanYou";
                            let _ = Command::new("open").arg(star_url).status();
                            state.app_state.star_acknowledged = true;
                            save_app_state(&state.app_state)?;
                        }
                        "back" => break,
                        _ => return Ok(()),
                    }
                }
            }
            Some(1) => {
                let path = state.last_html.clone().unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                open_path(&path)?;
            }
            Some(2) => run_share_menu(&mut state).await?,
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

    // Auto-update check FIRST, before any other startup work. We skip during
    // `serve` (would interrupt anyone browsing) and when the user opted out.
    let skip_update = cli.no_update || matches!(cli.command, Some(Commands::Serve(_)));
    auto_update_check(skip_update).await;

    let app_state = load_app_state();
    apply_publish_env(&app_state);

    // Auto-prune old reports on every invocation. Skips the active `serve`
    // session (deleting files mid-serve would 404 anyone currently browsing).
    if !matches!(cli.command, Some(Commands::Serve(_))) {
        let out_dir = app_state
            .out_dir
            .clone()
            .or_else(|| {
                cli.out_dir
                    .clone()
                    .or_else(|| match &cli.command {
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
        Some(Commands::Publish(args)) => run_publish(args).await,
        Some(Commands::Serve(args)) => run_serve(args),
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
                no_publish: cli.no_publish,
                axis_weights: cli.axis_weights,
            };
            run_battle(args).await
        }
    }
}
