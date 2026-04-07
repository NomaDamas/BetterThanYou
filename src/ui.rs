use std::io::{self, Stdout};
use std::path::Path;

use anyhow::Result;
use better_than_you::{open_path, AxisCard, BattleResult, SavedArtifacts};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;

// ── Game color palette ──────────────────────────────────────────────────────
const NEON_GOLD: Color = Color::Rgb(255, 214, 107);
const NEON_CYAN: Color = Color::Rgb(0, 255, 220);
const NEON_MAGENTA: Color = Color::Rgb(255, 60, 200);
const NEON_ORANGE: Color = Color::Rgb(255, 143, 66);
const NEON_BLUE: Color = Color::Rgb(100, 180, 255);
const NEON_GREEN: Color = Color::Rgb(80, 255, 120);
const NEON_RED: Color = Color::Rgb(255, 70, 70);
const NEON_PURPLE: Color = Color::Rgb(180, 120, 255);
const DARK_BG: Color = Color::Rgb(18, 18, 28);
const DIM_TEXT: Color = Color::Rgb(120, 120, 150);
const LIGHT_TEXT: Color = Color::Rgb(220, 220, 240);

// ── ASCII Art Logo ──────────────────────────────────────────────────────────
const LOGO: [&str; 6] = [
    r" ____       _   _           _____ _                __   __",
    r"| __ )  ___| |_| |_ ___ _ _|_   _| |__   __ _ _ _  \ \ / /__  _   _",
    r"|  _ \ / _ \ __| __/ _ \ '__|| | | '_ \ / _` | '_ \  \ V / _ \| | | |",
    r"| |_) |  __/ |_| ||  __/ |  | | | | | | (_| | | | |  | | (_) | |_| |",
    r"|____/ \___|\__|\__\___|_|  |_| |_| |_|\__,_|_| |_|  |_|\___/ \__,_|",
    r"                        C L I   P O R T R A I T   B A T T L E",
];

// ── Menu item icons by label prefix ─────────────────────────────────────────
fn icon_for_item(label: &str) -> &'static str {
    let lower = label.to_lowercase();
    if lower.starts_with("start battle") || lower.starts_with("rematch") { return "\u{2694} "; }
    if lower.starts_with("choose new") { return "\u{1F3AF} "; }
    if lower.starts_with("open") { return "\u{1F4C4} "; }
    if lower.starts_with("share") { return "\u{1F4E4} "; }
    if lower.starts_with("settings") { return "\u{2699} "; }
    if lower.starts_with("star") { return "\u{2B50} "; }
    if lower.starts_with("quit") || lower.starts_with("back") { return "\u{1F6AA} "; }
    if lower.starts_with("judge") { return "\u{2696} "; }
    if lower.starts_with("openai") { return "\u{1F511} "; }
    if lower.starts_with("labels") { return "\u{1F3F7} "; }
    if lower.starts_with("paste") { return "\u{1F4CB} "; }
    if lower.starts_with("output") { return "\u{1F4C2} "; }
    if lower.starts_with("aesthetic") { return "\u{1F3A8} "; }
    if lower.starts_with("enter") || lower.starts_with("set") { return "\u{270F} "; }
    if lower.starts_with("clear") { return "\u{1F5D1} "; }
    if lower.starts_with("switch") { return "\u{1F500} "; }
    if lower.starts_with("cancel") { return "\u{274C} "; }
    if lower.starts_with("reset") { return "\u{1F504} "; }
    if lower.starts_with("auto") { return "\u{26A1} "; }
    if lower.starts_with("heuristic") { return "\u{1F9EA} "; }
    "\u{25B8} "
}

struct TuiSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TuiSession {
    fn new() -> Result<Self> {
        let mut stdout = io::stdout();
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TuiSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen, cursor::Show);
        let _ = self.terminal.show_cursor();
    }
}

// ── Gradient color helpers ──────────────────────────────────────────────────
fn logo_gradient_color(row: usize) -> Color {
    match row {
        0 => Color::Rgb(255, 60, 200),
        1 => Color::Rgb(230, 80, 220),
        2 => Color::Rgb(200, 120, 255),
        3 => Color::Rgb(150, 160, 255),
        4 => Color::Rgb(100, 200, 255),
        _ => Color::Rgb(0, 255, 220),
    }
}

fn stat_bar_color(score: f32) -> Color {
    if score >= 90.0 { NEON_GOLD }
    else if score >= 75.0 { NEON_GREEN }
    else if score >= 60.0 { NEON_CYAN }
    else if score >= 45.0 { NEON_ORANGE }
    else { NEON_RED }
}

fn score_rank(score: f32) -> (&'static str, Color) {
    if score >= 95.0 { ("S+", Color::Rgb(255, 215, 0)) }
    else if score >= 90.0 { ("S", Color::Rgb(255, 200, 60)) }
    else if score >= 80.0 { ("A", NEON_GREEN) }
    else if score >= 70.0 { ("B", NEON_CYAN) }
    else if score >= 60.0 { ("C", NEON_ORANGE) }
    else if score >= 50.0 { ("D", NEON_RED) }
    else { ("F", Color::Rgb(180, 50, 50)) }
}

// ── Game-style block builder ────────────────────────────────────────────────
fn game_block(title: &str, border_color: Color) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(border_color).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(border_color))
}

fn game_block_double(title: &str, border_color: Color) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" \u{25C6} {} \u{25C6} ", title),
            Style::default().fg(border_color).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(border_color))
}

// ── Axis card for battle view ───────────────────────────────────────────────
fn axis_line_game(card: &AxisCard, winner_id: &str, bar_width: u16) -> Vec<Line<'static>> {
    let left_pct = (card.left / 100.0).min(1.0);
    let right_pct = (card.right / 100.0).min(1.0);
    let left_filled = (left_pct * bar_width as f32).round() as usize;
    let right_filled = (right_pct * bar_width as f32).round() as usize;
    let left_empty = (bar_width as usize).saturating_sub(left_filled);
    let right_empty = (bar_width as usize).saturating_sub(right_filled);

    let gap_indicator = if card.leader == "tie" {
        Span::styled(" TIE ", Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
    } else if card.leader == winner_id {
        Span::styled(
            format!(" +{:.1} ", card.diff),
            Style::default().fg(NEON_GREEN).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!(" -{:.1} ", card.diff),
            Style::default().fg(NEON_RED).add_modifier(Modifier::BOLD),
        )
    };

    let (left_rank, left_rank_color) = score_rank(card.left);
    let (right_rank, right_rank_color) = score_rank(card.right);

    vec![
        Line::from(vec![
            Span::styled(
                format!(" {:<20}", card.label),
                Style::default().fg(NEON_GOLD).add_modifier(Modifier::BOLD),
            ),
            gap_indicator,
        ]),
        Line::from(vec![
            Span::styled("  L ", Style::default().fg(NEON_ORANGE)),
            Span::styled(
                "\u{2588}".repeat(left_filled),
                Style::default().fg(stat_bar_color(card.left)),
            ),
            Span::styled(
                "\u{2591}".repeat(left_empty),
                Style::default().fg(Color::Rgb(40, 40, 60)),
            ),
            Span::styled(
                format!(" {:.1} ", card.left),
                Style::default().fg(LIGHT_TEXT),
            ),
            Span::styled(left_rank, Style::default().fg(left_rank_color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  R ", Style::default().fg(NEON_BLUE)),
            Span::styled(
                "\u{2588}".repeat(right_filled),
                Style::default().fg(stat_bar_color(card.right)),
            ),
            Span::styled(
                "\u{2591}".repeat(right_empty),
                Style::default().fg(Color::Rgb(40, 40, 60)),
            ),
            Span::styled(
                format!(" {:.1} ", card.right),
                Style::default().fg(LIGHT_TEXT),
            ),
            Span::styled(right_rank, Style::default().fg(right_rank_color).add_modifier(Modifier::BOLD)),
        ]),
    ]
}

// ── Select menu (game-style) ────────────────────────────────────────────────
pub fn select_menu(title: &str, subtitle: &[String], items: &[String], initial_index: usize) -> Result<Option<usize>> {
    let mut session = TuiSession::new()?;
    let mut selected = initial_index.min(items.len().saturating_sub(1));
    let mut state = ListState::default();
    state.select(Some(selected));

    loop {
        session.terminal.draw(|frame| {
            let area = frame.area();

            // Dark background fill
            frame.render_widget(Clear, area);
            frame.render_widget(
                Block::default().style(Style::default().bg(DARK_BG)),
                area,
            );

            let is_home = title == "BetterThanYou";
            let logo_height = if is_home { 8u16 } else { 0 };
            let subtitle_height = if subtitle.is_empty() { 0 } else { subtitle.len() as u16 + 2 };

            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(logo_height),             // logo or nothing
                    Constraint::Length(if !is_home { 3 } else { 0 }), // sub-page title
                    Constraint::Length(subtitle_height),         // context info
                    Constraint::Min(8),                         // menu items
                    Constraint::Length(3),                       // footer/hotkeys
                ])
                .split(area);

            // ── Logo (home screen only) ────────────────────────────────
            if is_home {
                let logo_lines: Vec<Line> = LOGO.iter().enumerate().map(|(i, line)| {
                    Line::from(Span::styled(*line, Style::default().fg(logo_gradient_color(i))))
                }).collect();
                let logo = Paragraph::new(logo_lines)
                    .alignment(Alignment::Center)
                    .block(Block::default().style(Style::default().bg(DARK_BG)));
                frame.render_widget(logo, sections[0]);
            }

            // ── Sub-page title (non-home screens) ──────────────────────
            if !is_home {
                let page_title = Paragraph::new(Line::from(vec![
                    Span::styled("\u{25C6} ", Style::default().fg(NEON_MAGENTA)),
                    Span::styled(title, Style::default().fg(NEON_GOLD).add_modifier(Modifier::BOLD)),
                    Span::styled(" \u{25C6}", Style::default().fg(NEON_MAGENTA)),
                ]))
                .alignment(Alignment::Center)
                .block(game_block("MENU", NEON_MAGENTA));
                frame.render_widget(page_title, sections[1]);
            }

            // ── Context/subtitle ───────────────────────────────────────
            if !subtitle.is_empty() {
                let subtitle_lines: Vec<Line> = subtitle.iter().map(|line| {
                    Line::from(vec![
                        Span::styled("  \u{25B8} ", Style::default().fg(DIM_TEXT)),
                        Span::styled(line.clone(), Style::default().fg(LIGHT_TEXT)),
                    ])
                }).collect();
                let subtitle_widget = Paragraph::new(subtitle_lines)
                    .block(game_block("STATUS", NEON_CYAN))
                    .wrap(Wrap { trim: true });
                frame.render_widget(subtitle_widget, sections[2]);
            }

            // ── Menu items with icons ──────────────────────────────────
            let items_widget: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let icon = icon_for_item(item);
                    let is_selected = i == selected;
                    let style = if is_selected {
                        Style::default()
                            .fg(DARK_BG)
                            .bg(NEON_GOLD)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(LIGHT_TEXT)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {}", icon), style),
                        Span::styled(item.clone(), style),
                        Span::styled("  ", style),
                    ]))
                })
                .collect();

            let list = List::new(items_widget)
                .block(game_block_double("SELECT ACTION", NEON_ORANGE))
                .highlight_symbol("")
                .highlight_style(Style::default());
            frame.render_stateful_widget(list, sections[3], &mut state);

            // ── Footer/hotkeys ─────────────────────────────────────────
            let footer = Paragraph::new(Line::from(vec![
                Span::styled(" \u{2191}\u{2193}", Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(" Navigate  ", Style::default().fg(DIM_TEXT)),
                Span::styled("\u{23CE}", Style::default().fg(NEON_GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(" Select  ", Style::default().fg(DIM_TEXT)),
                Span::styled("ESC", Style::default().fg(NEON_RED).add_modifier(Modifier::BOLD)),
                Span::styled(" Back", Style::default().fg(DIM_TEXT)),
            ]))
            .alignment(Alignment::Center)
            .block(Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::Rgb(50, 50, 70))));
            frame.render_widget(footer, sections[4]);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = selected.saturating_sub(1);
                    state.select(Some(selected));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected + 1 < items.len() {
                        selected += 1;
                        state.select(Some(selected));
                    }
                }
                KeyCode::Enter => return Ok(Some(selected)),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                _ => {}
            }
        }
    }
}

// ── Draw a game-styled panel ────────────────────────────────────────────────
fn draw_panel(frame: &mut ratatui::Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'static>>, border_color: Color) {
    let widget = Paragraph::new(lines)
        .block(game_block(title, border_color))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

// ── VS Header ───────────────────────────────────────────────────────────────
fn vs_header<'a>(result: &BattleResult) -> Vec<Line<'a>> {
    let winner_side = if result.winner.id == "left" { "L" } else { "R" };
    let (left_rank, left_rank_color) = score_rank(result.scores.left.total);
    let (right_rank, right_rank_color) = score_rank(result.scores.right.total);

    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {} ", result.inputs.left.label.to_uppercase()),
                Style::default().fg(NEON_ORANGE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{:.1}]", result.scores.left.total),
                Style::default().fg(LIGHT_TEXT),
            ),
            Span::styled(
                format!(" {} ", left_rank),
                Style::default().fg(left_rank_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   \u{2694} VS \u{2694}   ",
                Style::default().fg(NEON_MAGENTA).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", right_rank),
                Style::default().fg(right_rank_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{:.1}]", result.scores.right.total),
                Style::default().fg(LIGHT_TEXT),
            ),
            Span::styled(
                format!("  {} ", result.inputs.right.label.to_uppercase()),
                Style::default().fg(NEON_BLUE).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  \u{1F3C6} WINNER: ", Style::default().fg(NEON_GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{} ", result.winner.label.to_uppercase()),
                Style::default().fg(NEON_GOLD).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({})", winner_side),
                Style::default().fg(DIM_TEXT),
            ),
            Span::styled(
                format!("  \u{25B2} +{:.1} margin", result.winner.margin),
                Style::default().fg(NEON_GREEN),
            ),
            if result.winner.decisive {
                Span::styled("  DECISIVE!", Style::default().fg(NEON_RED).add_modifier(Modifier::BOLD))
            } else {
                Span::styled("  close match", Style::default().fg(DIM_TEXT))
            },
        ]),
    ]
}

// ── Battle view (game-style) ────────────────────────────────────────────────
pub fn present_battle_view(result: &BattleResult, artifacts: &SavedArtifacts, _footer_lines: &[String], on_open: Option<fn(&Path) -> Result<()>>) -> Result<()> {
    let mut session = TuiSession::new()?;

    loop {
        session.terminal.draw(|frame| {
            let area = frame.area();

            // Dark background
            frame.render_widget(Clear, area);
            frame.render_widget(
                Block::default().style(Style::default().bg(DARK_BG)),
                area,
            );

            let axis_count = result.axis_cards.len();

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(6),                              // VS header
                    Constraint::Length(5),                              // summary bar
                    Constraint::Length((axis_count as u16) * 3 + 2),   // axis stat bars
                    Constraint::Min(6),                                // analysis
                    Constraint::Length(3),                              // footer/hotkeys
                ])
                .split(area);

            // ── VS Header ──────────────────────────────────────────────
            let hero_lines = vs_header(result);
            let hero = Paragraph::new(hero_lines)
                .block(game_block_double("BATTLE RESULT", NEON_MAGENTA))
                .alignment(Alignment::Center);
            frame.render_widget(hero, layout[0]);

            // ── Summary bar ────────────────────────────────────────────
            let judge_str = result.engine.model.clone().unwrap_or_else(|| result.engine.judge_mode.clone());
            let summary = vec![
                Line::from(vec![
                    Span::styled("  \u{2696} Judge: ", Style::default().fg(DIM_TEXT)),
                    Span::styled(judge_str, Style::default().fg(NEON_PURPLE)),
                    Span::styled("    \u{1F4CA} Margin: ", Style::default().fg(DIM_TEXT)),
                    Span::styled(format!("{:.1}", result.winner.margin), Style::default().fg(NEON_GREEN).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled("  \u{1F7E0} Left: ", Style::default().fg(NEON_ORANGE)),
                    Span::styled(format!("{:.1}", result.scores.left.total), Style::default().fg(LIGHT_TEXT)),
                    Span::styled(format!(" ({})", result.inputs.left.label), Style::default().fg(DIM_TEXT)),
                    Span::styled("    \u{1F535} Right: ", Style::default().fg(NEON_BLUE)),
                    Span::styled(format!("{:.1}", result.scores.right.total), Style::default().fg(LIGHT_TEXT)),
                    Span::styled(format!(" ({})", result.inputs.right.label), Style::default().fg(DIM_TEXT)),
                ]),
            ];
            draw_panel(frame, layout[1], "SCOREBOARD", summary, NEON_CYAN);

            // ── Axis stat bars (RPG style) ─────────────────────────────
            let bar_width = area.width.saturating_sub(40).min(20);
            let mut axis_lines: Vec<Line> = Vec::new();
            for card in &result.axis_cards {
                axis_lines.extend(axis_line_game(card, &result.winner.id, bar_width));
            }
            draw_panel(frame, layout[2], "ABILITY STATS", axis_lines, NEON_ORANGE);

            // ── Analysis ───────────────────────────────────────────────
            let analysis = vec![
                Line::from(vec![
                    Span::styled("  \u{1F4AC} ", Style::default().fg(NEON_CYAN)),
                    Span::styled(result.sections.overall_take.clone(), Style::default().fg(LIGHT_TEXT)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  \u{1F3C6} Why: ", Style::default().fg(NEON_GOLD)),
                    Span::styled(result.sections.why_this_won.clone(), Style::default().fg(LIGHT_TEXT)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  \u{1F4DD} Notes: ", Style::default().fg(NEON_PURPLE)),
                    Span::styled(result.sections.model_jury_notes.clone(), Style::default().fg(DIM_TEXT)),
                ]),
            ];
            draw_panel(frame, layout[3], "JUDGE ANALYSIS", analysis, NEON_PURPLE);

            // ── Footer/hotkeys ─────────────────────────────────────────
            let footer = Paragraph::new(Line::from(vec![
                Span::styled(" \u{23CE}", Style::default().fg(NEON_GREEN).add_modifier(Modifier::BOLD)),
                Span::styled("/", Style::default().fg(DIM_TEXT)),
                Span::styled("q", Style::default().fg(NEON_RED).add_modifier(Modifier::BOLD)),
                Span::styled(" Return  ", Style::default().fg(DIM_TEXT)),
                Span::styled("o", Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(" Open Report", Style::default().fg(DIM_TEXT)),
            ]))
            .alignment(Alignment::Center)
            .block(Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::Rgb(50, 50, 70))));
            frame.render_widget(footer, layout[4]);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('o') => {
                    if let Some(callback) = on_open {
                        callback(Path::new(&artifacts.html_path))?;
                    } else {
                        open_path(Path::new(&artifacts.html_path))?;
                    }
                    return Ok(());
                }
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => return Ok(()),
                _ => {}
            }
        }
    }
}
