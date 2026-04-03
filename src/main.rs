use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, bail, Result};
use better_than_you::{
    analyze_portrait_battle, default_reports_dir, generate_share_bundle, open_path,
    present_terminal_battle_app, read_clipboard_text, regenerate_battle_report,
    render_open_summary, render_report_summary, render_terminal_battle, save_battle_artifacts,
    write_clipboard_text, AnalyzeOptions, BattleResult, JudgeMode,
};
use clap::{Parser, Subcommand, ValueEnum};
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, ValueEnum, Serialize, Deserialize)]
enum JudgeCli {
    Auto,
    Heuristic,
    Openai,
}

impl From<JudgeCli> for JudgeMode {
    fn from(value: JudgeCli) -> Self {
        match value {
            JudgeCli::Auto => JudgeMode::Auto,
            JudgeCli::Heuristic => JudgeMode::Heuristic,
            JudgeCli::Openai => JudgeMode::Openai,
        }
    }
}

impl JudgeCli {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Heuristic => "heuristic",
            Self::Openai => "openai",
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
    #[arg(long, default_value = "gpt-4.1-mini")]
    model: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    open: bool,
    #[arg(long)]
    no_app: bool,
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
    #[arg(long, default_value = "gpt-4.1-mini")]
    model: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    open: bool,
    #[arg(long)]
    no_app: bool,
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
    judge: Option<JudgeCli>,
    model: Option<String>,
    out_dir: Option<PathBuf>,
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
    last_json: Option<PathBuf>,
    last_html: Option<PathBuf>,
    last_result: Option<BattleResult>,
    last_pair: Option<(String, String)>,
}

impl SessionState {
    fn new() -> Self {
        let app_state = load_app_state();
        Self {
            judge: app_state.judge.clone().unwrap_or(JudgeCli::Auto),
            model: app_state.model.clone().unwrap_or_else(|| "gpt-4.1-mini".to_string()),
            out_dir: app_state.out_dir.clone().unwrap_or_else(default_reports_dir),
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
    if !state.star_acknowledged {
        eprintln!("Support BetterThanYou by starring https://github.com/NomaDamas/BetterThanYou . Run better-than-you with no args and press 's' to open the star page and hide this reminder.");
    }
}

fn select_menu(title: &str, subtitle: &[String], items: &[String], initial_index: usize) -> Result<Option<usize>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Ok(Some(initial_index.min(items.len().saturating_sub(1))));
    }

    let mut stdout = io::stdout();
    let mut selected = initial_index.min(items.len().saturating_sub(1));

    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

    loop {
        let screen = render_menu_screen(title, subtitle, items, selected);
        write_menu_screen(&mut stdout, &screen)?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                }
                KeyCode::Down => {
                    if selected + 1 < items.len() {
                        selected += 1;
                    }
                }
                KeyCode::Enter => {
                    break;
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    disable_raw_mode()?;
                    execute!(stdout, LeaveAlternateScreen, cursor::Show)?;
                    return Ok(None);
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, cursor::Show)?;
    Ok(Some(selected))
}


fn render_menu_screen(title: &str, subtitle: &[String], items: &[String], selected: usize) -> String {
    let mut lines = Vec::new();
    lines.push(title.to_string());
    lines.push(String::new());
    lines.extend(subtitle.iter().cloned());
    if !subtitle.is_empty() {
        lines.push(String::new());
    }
    for (index, item) in items.iter().enumerate() {
        if index == selected {
            lines.push(format!("  › {}", item));
        } else {
            lines.push(format!("    {}", item));
        }
    }
    lines.join("
")
}

fn write_menu_screen(stdout: &mut io::Stdout, screen: &str) -> Result<()> {
    execute!(stdout, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
    stdout.write_all(screen.replace("
", "
").as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn judge_index(judge: &JudgeCli) -> usize {
    match judge {
        JudgeCli::Auto => 0,
        JudgeCli::Openai => 1,
        JudgeCli::Heuristic => 2,
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
            if let Some(key) = prompt_optional("OpenAI API key > ")? {
                state.app_state.openai_api_key = Some(key);
                save_app_state(&state.app_state)?;
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
    if let Some(session_state) = state {
        options.openai_config.api_key = session_state.app_state.openai_api_key.clone();
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
        present_terminal_battle_app(&result, &artifacts, None)?;
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

fn render_start_subtitle(state: &SessionState) -> Vec<String> {
    vec![
        format!("Judge: {}", state.judge.as_str()),
        format!("Model: {}", state.model),
        format!("Left: {}", state.left.as_deref().unwrap_or("(not set)")),
        format!("Right: {}", state.right.as_deref().unwrap_or("(not set)")),
        format!("Output: {}", state.out_dir.display()),
    ]
}

fn run_settings_menu(state: &mut SessionState) -> Result<()> {
    loop {
        let subtitle = vec![
            format!("Current judge: {}", state.judge.as_str()),
            format!("Current model: {}", state.model),
        ];
        let items = vec![
            "Judge mode".to_string(),
            "OpenAI model".to_string(),
            "Labels".to_string(),
            "Paste both from clipboard".to_string(),
            "Output directory".to_string(),
            "OpenAI API key".to_string(),
            "Back".to_string(),
        ];
        match select_menu("Settings", &subtitle, &items, 0)? {
            Some(0) => {
                let judge_items = vec![
                    "Auto (recommended)".to_string(),
                    "OpenAI judge".to_string(),
                    "Heuristic judge".to_string(),
                ];
                if let Some(choice) = select_menu("Judge Mode", &[], &judge_items, judge_index(&state.judge))? {
                    state.judge = match choice {
                        0 => JudgeCli::Auto,
                        1 => JudgeCli::Openai,
                        _ => JudgeCli::Heuristic,
                    };
                    state.app_state.judge = Some(state.judge.clone());
                    save_app_state(&state.app_state)?;
                }
            }
            Some(1) => {
                if let Some(model) = prompt_optional("OpenAI model > ")? {
                    state.model = model;
                    state.app_state.model = Some(state.model.clone());
                    save_app_state(&state.app_state)?;
                }
            }
            Some(2) => {
                state.left_label = prompt_optional("Left label (optional) > ")?;
                state.right_label = prompt_optional("Right label (optional) > ")?;
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
                if let Some(out) = prompt_optional("Output dir > ")? {
                    state.out_dir = PathBuf::from(out);
                    state.app_state.out_dir = Some(state.out_dir.clone());
                    save_app_state(&state.app_state)?;
                }
            }
            Some(5) => {
                let items = vec![
                    "Set API key".to_string(),
                    "Clear saved API key".to_string(),
                    "Back".to_string(),
                ];
                match select_menu("OpenAI API Key", &[], &items, 0)? {
                    Some(0) => {
                        if let Some(key) = prompt_optional("OpenAI API key > ")? {
                            state.app_state.openai_api_key = Some(key);
                            save_app_state(&state.app_state)?;
                        }
                    }
                    Some(1) => {
                        state.app_state.openai_api_key = None;
                        save_app_state(&state.app_state)?;
                    }
                    _ => {}
                }
            }
            _ => return Ok(()),
        }
    }
}

fn run_share_menu(state: &mut SessionState) -> Result<()> {
    let Some(result) = state.last_result.as_ref() else {
        println!("No latest result to share. Press Enter to continue.");
        let _ = prompt_line("")?;
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

    loop {
        let mut items = vec![
            "Start New Battle".to_string(),
            "Rematch Last Pair".to_string(),
            "Share Latest Result".to_string(),
            "Open Latest Report".to_string(),
            "Settings".to_string(),
        ];
        if !state.app_state.star_acknowledged {
            items.push("Star BetterThanYou on GitHub".to_string());
        }
        items.push("Quit".to_string());

        let subtitle = render_start_subtitle(&state);
        match select_menu("BetterThanYou", &subtitle, &items, 0)? {
            Some(0) => {
                let left = state.left.clone().or(prompt_optional("Left path/URL/data URL > ")?).ok_or_else(|| anyhow!("Left input required"))?;
                let right = state.right.clone().or(prompt_optional("Right path/URL/data URL > ")?).ok_or_else(|| anyhow!("Right input required"))?;
                state.left = Some(left.clone());
                state.right = Some(right.clone());
                state.last_pair = Some((left.clone(), right.clone()));
                if matches!(state.judge, JudgeCli::Openai) {
                    ensure_openai_ready(&mut state)?;
                }
                let args = BattleArgs {
                    left: Some(left),
                    right: Some(right),
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
                };
                let (result, artifacts) = battle_from_args(&args, Some(&state)).await?;
                state.last_json = Some(PathBuf::from(&artifacts.json_path));
                state.last_html = Some(PathBuf::from(&artifacts.html_path));
                state.last_result = Some(result.clone());
                present_terminal_battle_app(&result, &artifacts, None)?;
            }
            Some(1) => {
                if let Some((left, right)) = state.last_pair.clone() {
                    let args = BattleArgs {
                        left: Some(left),
                        right: Some(right),
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
                    };
                    let (result, artifacts) = battle_from_args(&args, Some(&state)).await?;
                    state.last_json = Some(PathBuf::from(&artifacts.json_path));
                    state.last_html = Some(PathBuf::from(&artifacts.html_path));
                    state.last_result = Some(result.clone());
                    present_terminal_battle_app(&result, &artifacts, None)?;
                } else {
                    println!("No previous pair. Press Enter to continue.");
                    let _ = prompt_line("")?;
                }
            }
            Some(2) => run_share_menu(&mut state)?,
            Some(3) => {
                let path = state.last_html.clone().unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                open_path(&path)?;
                println!("{}", render_open_summary(&path, io::stdout().is_terminal()));
                let _ = prompt_line("Press Enter to continue > ")?;
            }
            Some(4) => run_settings_menu(&mut state)?,
            Some(5) if !state.app_state.star_acknowledged => {
                let star_url = "https://github.com/NomaDamas/BetterThanYou";
                let _ = Command::new("open").arg(star_url).status();
                state.app_state.star_acknowledged = true;
                save_app_state(&state.app_state)?;
            }
            _ => break,
        }
    }

    Ok(())
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
            if !(cli.left.is_none() && cli.right.is_none() && cli.left_label.is_none() && cli.right_label.is_none() && !cli.left_clipboard && !cli.right_clipboard && !cli.json && !cli.open && io::stdin().is_terminal() && io::stdout().is_terminal()) {
                maybe_print_star_reminder(&app_state);
            }
            if cli.left.is_none()
                && cli.right.is_none()
                && cli.left_label.is_none()
                && cli.right_label.is_none()
                && !cli.left_clipboard
                && !cli.right_clipboard
                && !cli.json
                && !cli.open
                && io::stdin().is_terminal()
                && io::stdout().is_terminal()
            {
                return run_interactive_app().await;
            }

            let args = BattleArgs {
                left: cli.left,
                right: cli.right,
                left_label: cli.left_label,
                right_label: cli.right_label,
                out_dir: cli.out_dir,
                left_clipboard: cli.left_clipboard,
                right_clipboard: cli.right_clipboard,
                judge: cli.judge,
                model: cli.model,
                json: cli.json,
                open: cli.open,
                no_app: cli.no_app,
            };
            run_battle(args).await
        }
    }
}
