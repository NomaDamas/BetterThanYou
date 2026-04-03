use std::io::{self, IsTerminal};
use std::path::PathBuf;

use anyhow::{bail, Result};
use better_than_you::{
    analyze_portrait_battle, default_reports_dir, open_path, present_terminal_battle_app,
    read_clipboard_text, regenerate_battle_report, render_open_summary, render_report_summary,
    render_terminal_battle, save_battle_artifacts, AnalyzeOptions, JudgeMode,
};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Clone, Debug, ValueEnum)]
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

#[derive(Debug, Clone)]
struct SessionState {
    left: Option<String>,
    right: Option<String>,
    left_label: Option<String>,
    right_label: Option<String>,
    judge: JudgeCli,
    model: String,
    out_dir: PathBuf,
    last_json: Option<PathBuf>,
    last_html: Option<PathBuf>,
}

impl SessionState {
    fn new() -> Self {
        Self {
            left: None,
            right: None,
            left_label: None,
            right_label: None,
            judge: JudgeCli::Auto,
            model: "gpt-4.1-mini".to_string(),
            out_dir: default_reports_dir(),
            last_json: None,
            last_html: None,
        }
    }
}

fn normalize_input(value: String) -> String {
    value.trim().to_string()
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::Write::flush(&mut io::stdout())?;
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

fn clear_screen() {
    print!("\u{1b}[2J\u{1b}[H");
    let _ = io::Write::flush(&mut io::stdout());
}

fn judge_label(judge: &JudgeCli) -> &'static str {
    match judge {
        JudgeCli::Auto => "auto",
        JudgeCli::Heuristic => "heuristic",
        JudgeCli::Openai => "openai",
    }
}

fn cycle_judge(judge: &JudgeCli) -> JudgeCli {
    match judge {
        JudgeCli::Auto => JudgeCli::Heuristic,
        JudgeCli::Heuristic => JudgeCli::Openai,
        JudgeCli::Openai => JudgeCli::Auto,
    }
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

async fn battle_from_args(args: BattleArgs) -> Result<(better_than_you::BattleResult, better_than_you::SavedArtifacts)> {
    let (left_source, right_source) = resolve_sources(args.left, args.right, args.left_clipboard, args.right_clipboard)?;
    let output_dir = args.out_dir.unwrap_or_else(default_reports_dir);

    let mut options = AnalyzeOptions::new(left_source, right_source);
    options.left_label = args.left_label;
    options.right_label = args.right_label;
    options.judge_mode = args.judge.clone().into();
    options.openai_model = args.model;

    let result = analyze_portrait_battle(options).await?;
    let artifacts = save_battle_artifacts(&result, &output_dir)?;
    Ok((result, artifacts))
}

async fn run_battle(args: BattleArgs) -> Result<()> {
    let (result, artifacts) = battle_from_args(args.clone()).await?;

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

fn render_start_screen(state: &SessionState) {
    clear_screen();
    println!("BetterThanYou // CLI Portrait Battle");
    println!();
    println!("Current inputs");
    println!("  left   : {}", state.left.as_deref().unwrap_or("(empty)"));
    println!("  right  : {}", state.right.as_deref().unwrap_or("(empty)"));
    println!("  judge  : {}", judge_label(&state.judge));
    println!("  model  : {}", state.model);
    println!("  out    : {}", state.out_dir.display());
    println!();
    println!("Actions");
    println!("  1  set left input");
    println!("  2  set right input");
    println!("  3  set labels");
    println!("  4  toggle judge mode");
    println!("  5  set OpenAI model");
    println!("  6  paste both from clipboard");
    println!("  7  run battle");
    println!("  8  rematch last pair");
    println!("  9  open latest report");
    println!("  q  quit");
    println!();
}

async fn run_interactive_app() -> Result<()> {
    let mut state = SessionState::new();
    let mut last_pair: Option<(String, String)> = None;

    loop {
        render_start_screen(&state);
        let action = prompt_line("Select action > ")?;
        match action.as_str() {
            "1" => state.left = prompt_optional("Left path/URL/data URL > ")?,
            "2" => state.right = prompt_optional("Right path/URL/data URL > ")?,
            "3" => {
                state.left_label = prompt_optional("Left label (optional) > ")?;
                state.right_label = prompt_optional("Right label (optional) > ")?;
            }
            "4" => state.judge = cycle_judge(&state.judge),
            "5" => {
                if let Some(model) = prompt_optional("OpenAI model > ")? {
                    state.model = model;
                }
            }
            "6" => {
                let clip = read_clipboard_text()?;
                let parts: Vec<String> = clip.replace('\r', "").split('\n').map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect();
                if parts.len() >= 2 {
                    state.left = Some(parts[0].clone());
                    state.right = Some(parts[1].clone());
                } else {
                    println!("Clipboard needs two non-empty lines. Press Enter to continue.");
                    let _ = prompt_line("")?;
                }
            }
            "7" => {
                let left = state.left.clone().or(prompt_optional("Left path/URL/data URL > ")?).ok_or_else(|| anyhow::anyhow!("Left input required"))?;
                let right = state.right.clone().or(prompt_optional("Right path/URL/data URL > ")?).ok_or_else(|| anyhow::anyhow!("Right input required"))?;
                state.left = Some(left.clone());
                state.right = Some(right.clone());
                last_pair = Some((left.clone(), right.clone()));
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
                let (result, artifacts) = battle_from_args(args).await?;
                state.last_json = Some(PathBuf::from(&artifacts.json_path));
                state.last_html = Some(PathBuf::from(&artifacts.html_path));
                present_terminal_battle_app(&result, &artifacts, None)?;
            }
            "8" => {
                if let Some((left, right)) = last_pair.clone() {
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
                    let (result, artifacts) = battle_from_args(args).await?;
                    state.last_json = Some(PathBuf::from(&artifacts.json_path));
                    state.last_html = Some(PathBuf::from(&artifacts.html_path));
                    present_terminal_battle_app(&result, &artifacts, None)?;
                } else {
                    println!("No previous pair. Press Enter to continue.");
                    let _ = prompt_line("")?;
                }
            }
            "9" => {
                let path = state.last_html.clone().unwrap_or_else(|| state.out_dir.join("latest-battle.html"));
                open_path(&path)?;
                println!("{}", render_open_summary(&path, io::stdout().is_terminal()));
                let _ = prompt_line("Press Enter to continue > ")?;
            }
            "q" | "quit" | "exit" => break,
            _ => {
                println!("Unknown action. Press Enter to continue.");
                let _ = prompt_line("")?;
            }
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
