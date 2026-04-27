use std::io::{self, Stdout};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use better_than_you::{
    localized_axis_short, open_path, AxisCard, BattleResult, Language, PublishedShareBundle,
    SavedArtifacts,
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

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
// All lines padded to equal width (77 chars) so Alignment::Center works
const LOGO: [&str; 6] = [
    " ____       _   _           _____ _                __   __               ",
    "| __ )  ___| |_| |_ ___ _ _|_   _| |__   __ _ _ _  \\ \\ / /__  _   _    ",
    "|  _ \\ / _ \\ __| __/ _ \\ '__|| | | '_ \\ / _` | '_ \\  \\ V / _ \\| | | |  ",
    "| |_) |  __/ |_| ||  __/ |  | | | | | | (_| | | | |  | | (_) | |_| |  ",
    "|____/ \\___|\\__|\\__\\___|_|  |_| |_| |_|\\__,_|_| |_|  |_|\\___/ \\__,_|  ",
    "                     C L I   P O R T R A I T   B A T T L E              ",
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
        execute!(stdout, EnterAlternateScreen, cursor::Hide, crossterm::event::EnableBracketedPaste)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TuiSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), crossterm::event::DisableBracketedPaste, LeaveAlternateScreen, cursor::Show);
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

    // Drain any buffered input events from prior screens
    while event::poll(Duration::from_millis(1))? {
        let _ = event::read()?;
    }

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

    // Drain any buffered input events from prior screens (e.g. loading animation)
    while event::poll(Duration::from_millis(1))? {
        let _ = event::read()?;
    }

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
            let has_dual = result.dual_scores.as_ref().and_then(|d| d.vlm.as_ref()).is_some();
            let dual_height: u16 = if has_dual { 4 } else { 0 };
            // Compact: 1 line per axis + 2 for borders
            let stat_height = (axis_count as u16) + 2;

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(6),                    // VS header
                    Constraint::Length(dual_height),          // dual-score bar (optional)
                    Constraint::Length(stat_height),          // side-by-side stats
                    Constraint::Min(5),                       // analysis
                    Constraint::Length(3),                    // footer/hotkeys
                ])
                .split(area);

            // ── VS Header with scores ──────────────────────────────────
            let hero_lines = vs_header(result);
            let hero = Paragraph::new(hero_lines)
                .block(game_block_double("BATTLE RESULT", NEON_MAGENTA))
                .alignment(Alignment::Center);
            frame.render_widget(hero, layout[0]);

            // ── Dual Score Dashboard (if VLM + heuristic both available) ──
            if has_dual {
                if let Some(dual) = result.dual_scores.as_ref() {
                    let mut dual_lines: Vec<Line> = Vec::new();
                    dual_lines.push(Line::from(vec![
                        Span::styled("  \u{2699} HEURISTIC  ", Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD)),
                        Span::styled(
                            format!("{} {:.1}", result.inputs.left.label, dual.heuristic.left.total),
                            Style::default().fg(NEON_ORANGE),
                        ),
                        Span::styled("  vs  ", Style::default().fg(DIM_TEXT)),
                        Span::styled(
                            format!("{} {:.1}", result.inputs.right.label, dual.heuristic.right.total),
                            Style::default().fg(NEON_BLUE),
                        ),
                    ]));
                    if let Some(vlm) = dual.vlm.as_ref() {
                        dual_lines.push(Line::from(vec![
                            Span::styled("  \u{2726} AI JUDGE   ", Style::default().fg(NEON_PURPLE).add_modifier(Modifier::BOLD)),
                            Span::styled(
                                format!("{} {:.1}", result.inputs.left.label, vlm.left.total),
                                Style::default().fg(NEON_ORANGE),
                            ),
                            Span::styled("  vs  ", Style::default().fg(DIM_TEXT)),
                            Span::styled(
                                format!("{} {:.1}", result.inputs.right.label, vlm.right.total),
                                Style::default().fg(NEON_BLUE),
                            ),
                        ]));
                    }
                    let dual_panel = Paragraph::new(dual_lines)
                        .block(Block::default().borders(Borders::ALL)
                            .title(Span::styled(" DUAL SCORE ", Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD)))
                            .border_style(Style::default().fg(NEON_CYAN)));
                    frame.render_widget(dual_panel, layout[1]);
                }
            }

            // ── Side-by-side stat comparison (compact 1-line-per-axis) ─
            let stat_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(10), Constraint::Percentage(45)])
                .split(layout[2]);

            // Resolve UI language from the battle result (falls back to English)
            let ui_lang = match result.language.as_deref() {
                Some("ko") => Language::Korean,
                Some("ja") => Language::Japanese,
                _ => Language::English,
            };

            // Left stats
            let left_is_winner = result.winner.id == "left";
            let left_title = format!(" {} {:.1} ", result.inputs.left.label, result.scores.left.total);
            let left_border = if left_is_winner { NEON_GOLD } else { NEON_ORANGE };
            let mut left_stat_lines: Vec<Line> = Vec::new();
            for card in &result.axis_cards {
                let bar_w = stat_cols[0].width.saturating_sub(22) as usize;
                let filled = ((card.left / 100.0) * bar_w as f32).round() as usize;
                let empty = bar_w.saturating_sub(filled);
                let won_axis = card.leader == "left";
                let bar_color = if won_axis { NEON_GREEN } else { stat_bar_color(card.left) };
                let indicator = if won_axis { "\u{25B2}" } else if card.leader == "tie" { "=" } else { " " };
                // Use short localized label (e.g. "SYM", "SKIN", "대칭", "피부")
                let short = localized_axis_short(ui_lang, &card.key);
                left_stat_lines.push(Line::from(vec![
                    Span::styled(format!(" {:<8}", short), Style::default().fg(NEON_GOLD)),
                    Span::styled("\u{2588}".repeat(filled), Style::default().fg(bar_color)),
                    Span::styled("\u{2591}".repeat(empty), Style::default().fg(Color::Rgb(40, 40, 60))),
                    Span::styled(format!(" {:>5.1} {}", card.left, indicator), Style::default().fg(if won_axis { NEON_GREEN } else { LIGHT_TEXT })),
                ]));
            }
            let left_panel = Paragraph::new(left_stat_lines)
                .block(Block::default().borders(Borders::ALL)
                    .title(Span::styled(left_title, Style::default().fg(left_border).add_modifier(Modifier::BOLD)))
                    .border_style(Style::default().fg(left_border)));
            frame.render_widget(left_panel, stat_cols[0]);

            // VS column (gap indicators)
            let mut gap_lines: Vec<Line> = Vec::new();
            gap_lines.push(Line::from(""));
            for card in &result.axis_cards {
                let (gap_text, gap_color) = if card.leader == "tie" {
                    ("TIE".to_string(), NEON_CYAN)
                } else if card.leader == result.winner.id {
                    (format!("+{:.0}", card.diff), NEON_GREEN)
                } else {
                    (format!("-{:.0}", card.diff), NEON_RED)
                };
                gap_lines.push(Line::from(Span::styled(
                    format!("{:^8}", gap_text),
                    Style::default().fg(gap_color).add_modifier(Modifier::BOLD),
                )));
            }
            let gap_panel = Paragraph::new(gap_lines).alignment(Alignment::Center);
            frame.render_widget(gap_panel, stat_cols[1]);

            // Right stats
            let right_is_winner = result.winner.id == "right";
            let right_title = format!(" {} {:.1} ", result.inputs.right.label, result.scores.right.total);
            let right_border = if right_is_winner { NEON_GOLD } else { NEON_BLUE };
            let mut right_stat_lines: Vec<Line> = Vec::new();
            for card in &result.axis_cards {
                let bar_w = stat_cols[2].width.saturating_sub(22) as usize;
                let filled = ((card.right / 100.0) * bar_w as f32).round() as usize;
                let empty = bar_w.saturating_sub(filled);
                let won_axis = card.leader == "right";
                let bar_color = if won_axis { NEON_GREEN } else { stat_bar_color(card.right) };
                let indicator = if won_axis { "\u{25B2}" } else if card.leader == "tie" { "=" } else { " " };
                let short = localized_axis_short(ui_lang, &card.key);
                right_stat_lines.push(Line::from(vec![
                    Span::styled(format!(" {:<8}", short), Style::default().fg(NEON_GOLD)),
                    Span::styled("\u{2588}".repeat(filled), Style::default().fg(bar_color)),
                    Span::styled("\u{2591}".repeat(empty), Style::default().fg(Color::Rgb(40, 40, 60))),
                    Span::styled(format!(" {:>5.1} {}", card.right, indicator), Style::default().fg(if won_axis { NEON_GREEN } else { LIGHT_TEXT })),
                ]));
            }
            let right_panel = Paragraph::new(right_stat_lines)
                .block(Block::default().borders(Borders::ALL)
                    .title(Span::styled(right_title, Style::default().fg(right_border).add_modifier(Modifier::BOLD)))
                    .border_style(Style::default().fg(right_border)));
            frame.render_widget(right_panel, stat_cols[2]);

            // ── Analysis notes ─────────────────────────────────────────
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
                    let html_path = Path::new(&artifacts.html_path);
                    let published_url = html_path
                        .parent()
                        .map(|dir| dir.join("latest-published.json"))
                        .filter(|p| p.exists())
                        .and_then(|p| std::fs::read_to_string(&p).ok())
                        .and_then(|s| serde_json::from_str::<PublishedShareBundle>(&s).ok())
                        .map(|b| b.share_page_url);

                    if let Some(url) = published_url {
                        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                        let _ = std::process::Command::new(opener).arg(&url).status();
                    } else if let Some(callback) = on_open {
                        callback(html_path)?;
                    } else {
                        open_path(html_path)?;
                    }
                    return Ok(());
                }
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => return Ok(()),
                _ => {}
            }
        }
    }
}

// ── TUI Battle Input Screen (split LEFT/RIGHT) ─────────────────────────────

// ── TUI Text Input ──────────────────────────────────────────────────────────

/// Shows a TUI text input screen. Returns Some(value) on Enter, None on Esc.
/// If the user presses Enter with empty input, returns Some("").
pub fn text_input(title: &str, hint: &str, initial: &str, is_secret: bool) -> Result<Option<String>> {
    let mut session = TuiSession::new()?;
    let mut input = initial.to_string();

    while event::poll(Duration::from_millis(1))? {
        let _ = event::read()?;
    }

    loop {
        session.terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Clear, area);
            frame.render_widget(Block::default().style(Style::default().bg(DARK_BG)), area);

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),      // top spacer
                    Constraint::Length(3),    // title
                    Constraint::Length(3),    // hint
                    Constraint::Length(5),    // input box
                    Constraint::Length(3),    // hotkeys
                    Constraint::Min(3),      // bottom spacer
                ])
                .split(area);

            // Center horizontally
            let center = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(layout[1]);
            let center_input = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(layout[3]);
            let center_hint = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(layout[2]);

            let title_widget = Paragraph::new(Line::from(vec![
                Span::styled("\u{25C6} ", Style::default().fg(NEON_MAGENTA)),
                Span::styled(title, Style::default().fg(NEON_GOLD).add_modifier(Modifier::BOLD)),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(title_widget, center[1]);

            if !hint.is_empty() {
                let hint_widget = Paragraph::new(Line::from(Span::styled(
                    hint, Style::default().fg(DIM_TEXT),
                )))
                .alignment(Alignment::Center);
                frame.render_widget(hint_widget, center_hint[1]);
            }

            let display_text = if is_secret && !input.is_empty() {
                format!("{}\u{2588}", "*".repeat(input.len()))
            } else {
                format!("{}\u{2588}", &input)
            };
            let input_widget = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!(" {}", display_text),
                    Style::default().fg(NEON_GOLD),
                )),
            ])
            .block(game_block("INPUT", NEON_CYAN));
            frame.render_widget(input_widget, center_input[1]);

            let footer = Paragraph::new(Line::from(vec![
                Span::styled(" \u{23CE}", Style::default().fg(NEON_GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(" Confirm  ", Style::default().fg(DIM_TEXT)),
                Span::styled("ESC", Style::default().fg(NEON_RED).add_modifier(Modifier::BOLD)),
                Span::styled(" Cancel", Style::default().fg(DIM_TEXT)),
            ]))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::Rgb(50, 50, 70))));
            frame.render_widget(footer, layout[4]);
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                match key.code {
                    KeyCode::Enter => return Ok(Some(input)),
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Backspace => { input.pop(); }
                    KeyCode::Char(c) => { input.push(c); }
                    _ => {}
                }
            }
            Event::Paste(text) => {
                let cleaned = text.replace('\n', "").replace('\r', "").trim().to_string();
                if cleaned.starts_with('/') || cleaned.starts_with('~') || cleaned.starts_with("http") || cleaned.starts_with("data:") {
                    input = cleaned;
                } else {
                    input.push_str(&cleaned);
                }
            }
            _ => {}
        }
    }
}

fn clean_path(input: &str) -> String {
    let s = input.trim()
        .trim_matches('\'')
        .trim_matches('"')
        .trim();
    // macOS drag-and-drop escapes spaces with backslash: /path/to/my\ file.jpg
    s.replace("\\ ", " ")
        .replace("\\(", "(")
        .replace("\\)", ")")
        .replace("\\[", "[")
        .replace("\\]", "]")
}

fn try_load_preview(path: &str, picker: &Picker) -> Option<StatefulProtocol> {
    let cleaned = clean_path(path);
    if cleaned.is_empty() { return None; }
    let p = std::path::Path::new(&cleaned);
    if !p.exists() { return None; }
    let img = image::open(p).ok()?;
    // Resize to reasonable thumbnail for terminal display
    let thumb = img.resize(400, 400, image::imageops::FilterType::Triangle);
    Some(picker.new_resize_protocol(thumb))
}

pub fn battle_input_screen(existing_left: Option<&str>, existing_right: Option<&str>) -> Result<Option<(String, String)>> {
    // Detect terminal image protocol BEFORE entering alternate screen
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    let mut session = TuiSession::new()?;
    let mut left_input = existing_left.unwrap_or("").to_string();
    let mut right_input = existing_right.unwrap_or("").to_string();
    let mut active_side: usize = 0;
    let mut left_preview: Option<StatefulProtocol> = None;
    let mut right_preview: Option<StatefulProtocol> = None;
    let mut left_prev_path = String::new();
    let mut right_prev_path = String::new();

    // Drain buffered events
    while event::poll(Duration::from_millis(1))? {
        let _ = event::read()?;
    }

    loop {
        // Reload previews when path changes
        let left_cleaned = clean_path(&left_input);
        if left_cleaned != left_prev_path {
            left_preview = try_load_preview(&left_input, &mut picker);
            left_prev_path = left_cleaned;
        }
        let right_cleaned = clean_path(&right_input);
        if right_cleaned != right_prev_path {
            right_preview = try_load_preview(&right_input, &mut picker);
            right_prev_path = right_cleaned;
        }

        session.terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Clear, area);
            frame.render_widget(Block::default().style(Style::default().bg(DARK_BG)), area);

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4),   // title
                    Constraint::Min(10),     // split panels
                    Constraint::Length(3),   // hotkeys
                ])
                .split(area);

            // Title
            let title = Paragraph::new(vec![
                Line::from(Span::styled(
                    "\u{2694}  BATTLE SETUP  \u{2694}",
                    Style::default().fg(NEON_MAGENTA).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "Paste or drag portrait file paths into each side",
                    Style::default().fg(LIGHT_TEXT),
                )),
            ])
            .alignment(Alignment::Center)
            .block(game_block_double("PORTRAIT INPUT", NEON_MAGENTA));
            frame.render_widget(title, layout[0]);

            // Split panels
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[1]);

            // Helper to render a side panel
            for (side, panel_area) in panels.iter().enumerate() {
                let is_left = side == 0;
                let input = if is_left { &left_input } else { &right_input };
                let is_active = active_side == side;
                let accent = if is_left { NEON_ORANGE } else { NEON_BLUE };
                let border_color = if is_active { accent } else { Color::Rgb(60, 60, 80) };
                let cursor_char = if is_active { "\u{2588}" } else { "" };
                let label = if is_left { " \u{1F7E0} LEFT " } else { " \u{1F535} RIGHT " };

                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(label, Style::default().fg(accent).add_modifier(Modifier::BOLD)))
                    .border_style(Style::default().fg(border_color));
                let inner = block.inner(*panel_area);
                frame.render_widget(block, *panel_area);

                // Split inner: preview area + input line
                let inner_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(4), Constraint::Length(3)])
                    .split(inner);

                // Image preview area
                let preview = if is_left { &mut left_preview } else { &mut right_preview };
                if let Some(proto) = preview.as_mut() {
                    let img_widget = StatefulImage::new();
                    frame.render_stateful_widget(img_widget, inner_layout[0], proto);
                } else {
                    let cleaned = clean_path(input);
                    let file_exists = !cleaned.is_empty() && Path::new(&cleaned).exists();
                    let ph_lines = if input.trim().is_empty() {
                        vec![
                            Line::from(""),
                            Line::from(Span::styled("Drag & drop or paste", Style::default().fg(DIM_TEXT))),
                            Line::from(Span::styled("an image file here", Style::default().fg(DIM_TEXT))),
                        ]
                    } else if file_exists {
                        vec![
                            Line::from(Span::styled("\u{2714} File found", Style::default().fg(NEON_GREEN))),
                            Line::from(Span::styled("(preview not supported", Style::default().fg(DIM_TEXT))),
                            Line::from(Span::styled(" in this terminal)", Style::default().fg(DIM_TEXT))),
                        ]
                    } else {
                        vec![
                            Line::from(Span::styled("Path not found:", Style::default().fg(NEON_RED))),
                            Line::from(Span::styled(
                                if cleaned.len() > 30 { format!("...{}", &cleaned[cleaned.len()-28..]) } else { cleaned },
                                Style::default().fg(DIM_TEXT),
                            )),
                        ]
                    };
                    let ph = Paragraph::new(ph_lines)
                    .alignment(Alignment::Center);
                    frame.render_widget(ph, inner_layout[0]);
                }

                // Input line
                let status = if input.trim().is_empty() {
                    Span::styled(" waiting...", Style::default().fg(DIM_TEXT))
                } else {
                    Span::styled(" \u{2714} ready", Style::default().fg(NEON_GREEN))
                };
                let max_w = inner_layout[1].width as usize;
                let display = if input.is_empty() {
                    format!(" {}", cursor_char)
                } else {
                    let truncated = if input.len() > max_w.saturating_sub(4) {
                        format!("...{}", &input[input.len().saturating_sub(max_w.saturating_sub(6))..])
                    } else {
                        input.clone()
                    };
                    format!(" {}{}", truncated, cursor_char)
                };
                let input_widget = Paragraph::new(vec![
                    Line::from(status),
                    Line::from(Span::styled(display, Style::default().fg(if is_active { NEON_GOLD } else { LIGHT_TEXT }))),
                ])
                .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::Rgb(50, 50, 70))));
                frame.render_widget(input_widget, inner_layout[1]);
            }

            // Hotkeys
            let both_ready = !left_input.trim().is_empty() && !right_input.trim().is_empty();
            let enter_color = if both_ready { NEON_GREEN } else { Color::Rgb(50, 50, 50) };
            let footer = Paragraph::new(Line::from(vec![
                Span::styled(" TAB", Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(" Switch  ", Style::default().fg(DIM_TEXT)),
                Span::styled("\u{23CE}", Style::default().fg(enter_color).add_modifier(Modifier::BOLD)),
                Span::styled(if both_ready { " Battle!  " } else { " (fill both)  " }, Style::default().fg(DIM_TEXT)),
                Span::styled("ESC", Style::default().fg(NEON_RED).add_modifier(Modifier::BOLD)),
                Span::styled(" Back", Style::default().fg(DIM_TEXT)),
            ]))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::Rgb(50, 50, 70))));
            frame.render_widget(footer, layout[2]);
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        active_side = 1 - active_side;
                    }
                    KeyCode::Enter => {
                        if !left_input.trim().is_empty() && !right_input.trim().is_empty() {
                            return Ok(Some((clean_path(&left_input), clean_path(&right_input))));
                        }
                        let current = if active_side == 0 { &left_input } else { &right_input };
                        if !current.trim().is_empty() {
                            active_side = 1 - active_side;
                        }
                    }
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Backspace => {
                        if active_side == 0 { left_input.pop(); } else { right_input.pop(); }
                    }
                    KeyCode::Char(c) => {
                        if active_side == 0 { left_input.push(c); } else { right_input.push(c); }
                    }
                    _ => {}
                }
            }
            // Handle paste events (multi-char, e.g. drag-and-drop paths)
            Event::Paste(text) => {
                let cleaned = text.replace('\n', "").replace('\r', "").trim().to_string();
                // If paste looks like a file path or URL, replace instead of append
                let is_path = cleaned.starts_with('/')
                    || cleaned.starts_with('~')
                    || cleaned.starts_with("http")
                    || cleaned.starts_with("data:")
                    || cleaned.starts_with('\'')
                    || cleaned.starts_with('"');
                if active_side == 0 {
                    if is_path { left_input = cleaned; } else { left_input.push_str(&cleaned); }
                } else {
                    if is_path { right_input = cleaned; } else { right_input.push_str(&cleaned); }
                }
            }
            _ => {}
        }
    }
}

// ── Battle Loading Animation (while VLM analyzes) ───────────────────────────

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const SPINNER_FRAMES: [&str; 8] = ["\u{28F7}", "\u{28EF}", "\u{28DF}", "\u{287F}", "\u{28BF}", "\u{28FB}", "\u{28FD}", "\u{28FE}"];

const BATTLE_PHRASES: [&str; 8] = [
    "Analyzing portraits...",
    "Measuring symmetry...",
    "Evaluating lighting...",
    "Judging composition...",
    "Comparing style aura...",
    "Scoring color vitality...",
    "Computing final scores...",
    "Rendering verdict...",
];

/// Shows animated loading screen while VLM analysis runs.
/// Call `done.store(true, Ordering::Relaxed)` from another thread to stop.
pub fn battle_loading_screen(
    _left_path: &str,
    _right_path: &str,
    done: Arc<AtomicBool>,
) -> Result<()> {
    let mut session = TuiSession::new()?;
    let mut frame: u64 = 0;

    while event::poll(Duration::from_millis(1))? {
        let _ = event::read()?;
    }

    while !done.load(Ordering::Relaxed) {
        let f = frame;
        session.terminal.draw(|frame_ref| {
            let area = frame_ref.area();
            frame_ref.render_widget(Clear, area);
            frame_ref.render_widget(Block::default().style(Style::default().bg(DARK_BG)), area);

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),     // top spacer
                    Constraint::Length(3),   // title
                    Constraint::Length(11),  // VS animation
                    Constraint::Length(5),   // spinner + status
                    Constraint::Min(3),     // bottom spacer
                ])
                .split(area);

            // Title with pulsing color
            let title_color = match (f / 3) % 4 {
                0 => NEON_MAGENTA,
                1 => NEON_GOLD,
                2 => NEON_CYAN,
                _ => NEON_PURPLE,
            };
            let title = Paragraph::new(Line::from(Span::styled(
                "\u{2694}  BATTLE IN PROGRESS  \u{2694}",
                Style::default().fg(title_color).add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center)
            .block(game_block("ANALYZING", title_color));
            frame_ref.render_widget(title, layout[1]);

            // VS animation with faces
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(30), Constraint::Percentage(35)])
                .split(layout[2]);

            // Left face
            let left_lines: Vec<Line> = FACE_LEFT.iter().enumerate().map(|(i, line)| {
                let pulse = ((f as u8).wrapping_mul(2).wrapping_add(i as u8 * 3)) % 20;
                let color = Color::Rgb(255, 190 + pulse.min(15), 130 + pulse.min(10));
                Line::from(Span::styled(*line, Style::default().fg(color)))
            }).collect();
            let left_bounce = face_bounce(f, false);
            let mut left_padded = Vec::new();
            for _ in 0..(1 + left_bounce).max(0) as usize { left_padded.push(Line::from("")); }
            left_padded.extend(left_lines);
            frame_ref.render_widget(Paragraph::new(left_padded).alignment(Alignment::Center), cols[0]);

            // VS sparks
            let vs_idx = (f as usize / 2) % VS_FRAMES.len();
            let vs_lines: Vec<Line> = VS_FRAMES[vs_idx].iter().enumerate().map(|(i, line)| {
                if i == 1 {
                    Line::from(Span::styled(*line, Style::default().fg(vs_color(f)).add_modifier(Modifier::BOLD)))
                } else {
                    Line::from(Span::styled(*line, Style::default().fg(spark_color(f + i as u64))))
                }
            }).collect();
            let mut vs_padded = Vec::new();
            for _ in 0..3 { vs_padded.push(Line::from("")); }
            vs_padded.extend(vs_lines);
            frame_ref.render_widget(Paragraph::new(vs_padded).alignment(Alignment::Center), cols[1]);

            // Right face
            let right_lines: Vec<Line> = FACE_RIGHT.iter().enumerate().map(|(i, line)| {
                let pulse = ((f as u8).wrapping_mul(2).wrapping_add(i as u8 * 3)) % 20;
                let color = Color::Rgb(160 + pulse.min(15), 190 + pulse.min(15), 255);
                Line::from(Span::styled(*line, Style::default().fg(color)))
            }).collect();
            let right_bounce = face_bounce(f, true);
            let mut right_padded = Vec::new();
            for _ in 0..(1 + right_bounce).max(0) as usize { right_padded.push(Line::from("")); }
            right_padded.extend(right_lines);
            frame_ref.render_widget(Paragraph::new(right_padded).alignment(Alignment::Center), cols[2]);

            // Spinner + status
            let spinner = SPINNER_FRAMES[(f as usize) % SPINNER_FRAMES.len()];
            let phrase = BATTLE_PHRASES[((f / 8) as usize) % BATTLE_PHRASES.len()];
            let bar_w = 30usize;
            let progress = ((f % (bar_w as u64 * 2)) as usize).min(bar_w);
            let bar = format!("{}{}", "\u{2588}".repeat(progress), "\u{2591}".repeat(bar_w - progress));

            let status = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(format!("  {} ", spinner), Style::default().fg(NEON_CYAN)),
                    Span::styled(phrase, Style::default().fg(LIGHT_TEXT)),
                ]),
                Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(bar, Style::default().fg(NEON_MAGENTA)),
                ]),
            ])
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::Rgb(50, 50, 70))));
            frame_ref.render_widget(status, layout[3]);
        })?;

        frame = frame.wrapping_add(1);

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    break;
                }
            }
        }
    }
    // Explicitly drop session to cleanly restore terminal
    drop(session);
    Ok(())
}

// ── Animated splash screen ──────────────────────────────────────────────────

// Male fighter - block character pixel art
const FACE_LEFT: [&str; 9] = [
    "  \u{2584}\u{2584}\u{2584}\u{2584}\u{2584}\u{2584}\u{2584}  ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{25CF}  \u{25CF}\u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    " \u{2588}\u{2588} \u{2580}\u{2580}\u{2580} \u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "  \u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}  ",
    "   \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   ",
    "  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
];

// Female fighter - block character pixel art with hair frame
const FACE_RIGHT: [&str; 9] = [
    "  \u{2584}\u{2584}\u{2584}\u{2584}\u{2584}\u{2584}\u{2584}  ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{25C9}  \u{25C9}\u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    " \u{2588}\u{2588} \u{2580}\u{2580}\u{2580} \u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "  \u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}  ",
    "   \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   ",
    "  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
];

const VS_FRAMES: [[&str; 5]; 4] = [
    [" \u{2728}        ", "   \u{2694} VS \u{2694}  ", "        \u{2728} ", "  \u{2606}      ", "      \u{2606}  "],
    ["        \u{2605} ", "   \u{2694} VS \u{2694}  ", " \u{2605}        ", "      \u{2734}  ", "  \u{2734}      "],
    ["  \u{2734}      ", "   \u{2694} VS \u{2694}  ", "      \u{2734}  ", "        \u{2728} ", " \u{2728}        "],
    ["      \u{2606}  ", "   \u{2694} VS \u{2694}  ", "  \u{2606}      ", " \u{2605}        ", "        \u{2605} "],
];

fn spark_color(frame: u64) -> Color {
    match frame % 6 {
        0 => NEON_MAGENTA,
        1 => NEON_GOLD,
        2 => NEON_CYAN,
        3 => Color::Rgb(255, 100, 100),
        4 => NEON_PURPLE,
        _ => NEON_GREEN,
    }
}

fn vs_color(frame: u64) -> Color {
    match (frame / 2) % 4 {
        0 => NEON_MAGENTA,
        1 => Color::Rgb(255, 255, 100),
        2 => NEON_CYAN,
        _ => Color::Rgb(255, 140, 50),
    }
}

fn face_bounce(frame: u64, side: bool) -> i16 {
    let phase = if side { frame } else { frame + 2 };
    match phase % 8 {
        0 | 1 => 0,
        2 | 3 => -1,
        4 | 5 => 0,
        _ => 1,
    }
}

/// Returns `true` if the user pressed 's' to star on GitHub.
pub fn splash_screen(star_acknowledged: bool) -> Result<bool> {
    let mut session = TuiSession::new()?;
    let mut frame: u64 = 0;

    // Drain any buffered input events
    while event::poll(Duration::from_millis(1))? {
        let _ = event::read()?;
    }

    loop {
        let f = frame;
        session.terminal.draw(|frame_ref| {
            let area = frame_ref.area();

            // Dark background
            frame_ref.render_widget(Clear, area);
            frame_ref.render_widget(
                Block::default().style(Style::default().bg(DARK_BG)),
                area,
            );

            let star_height = if star_acknowledged { 0u16 } else { 3 };
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(8),       // logo
                    Constraint::Length(1),       // spacer
                    Constraint::Length(11),      // faces + VS
                    Constraint::Length(1),       // spacer
                    Constraint::Length(3),       // labels
                    Constraint::Length(star_height), // star message
                    Constraint::Min(1),         // spacer
                    Constraint::Length(3),       // press any key
                ])
                .split(area);

            // ── Animated gradient logo ────────────────────────────────
            let logo_lines: Vec<Line> = LOGO.iter().enumerate().map(|(i, line)| {
                let color_shift = ((f / 3) as usize + i) % 6;
                Line::from(Span::styled(*line, Style::default().fg(logo_gradient_color(color_shift))))
            }).collect();
            let logo = Paragraph::new(logo_lines)
                .alignment(Alignment::Center)
                .block(Block::default().style(Style::default().bg(DARK_BG)));
            frame_ref.render_widget(logo, layout[0]);

            // ── Faces + VS ───────────────────────────────────────────
            let face_area = layout[2];
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(35),
                    Constraint::Percentage(30),
                    Constraint::Percentage(35),
                ])
                .split(face_area);

            // Left face with bounce + warm amber skin tone
            let left_bounce = face_bounce(f, false);
            let left_lines: Vec<Line> = FACE_LEFT.iter().enumerate().map(|(i, line)| {
                let pulse = ((f as u8).wrapping_mul(2).wrapping_add(i as u8 * 3)) % 20;
                let color = Color::Rgb(255, 190 + (pulse.min(15)) as u8, 130 + (pulse.min(10)) as u8);
                Line::from(Span::styled(*line, Style::default().fg(color)))
            }).collect();
            let mut left_face_lines = Vec::new();
            let pad = (1 + left_bounce).max(0) as usize;
            for _ in 0..pad { left_face_lines.push(Line::from("")); }
            left_face_lines.extend(left_lines);
            let left_face = Paragraph::new(left_face_lines)
                .alignment(Alignment::Center);
            frame_ref.render_widget(left_face, cols[0]);

            // VS animation in center - taller to match face height
            let vs_frame_idx = (f as usize / 2) % VS_FRAMES.len();
            let vs_lines: Vec<Line> = VS_FRAMES[vs_frame_idx].iter().enumerate().map(|(i, line)| {
                if i == 1 {
                    Line::from(Span::styled(
                        *line,
                        Style::default().fg(vs_color(f)).add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(
                        *line,
                        Style::default().fg(spark_color(f + i as u64)),
                    ))
                }
            }).collect();
            let mut vs_padded = Vec::new();
            for _ in 0..3 { vs_padded.push(Line::from("")); }
            vs_padded.extend(vs_lines);
            let vs_widget = Paragraph::new(vs_padded)
                .alignment(Alignment::Center);
            frame_ref.render_widget(vs_widget, cols[1]);

            // Right face with bounce + cool blue-silver tone
            let right_bounce = face_bounce(f, true);
            let right_lines: Vec<Line> = FACE_RIGHT.iter().enumerate().map(|(i, line)| {
                let pulse = ((f as u8).wrapping_mul(2).wrapping_add(i as u8 * 3)) % 20;
                let color = Color::Rgb(160 + (pulse.min(15)) as u8, 190 + (pulse.min(15)) as u8, 255);
                Line::from(Span::styled(*line, Style::default().fg(color)))
            }).collect();
            let mut right_face_lines = Vec::new();
            let pad = (1 + right_bounce).max(0) as usize;
            for _ in 0..pad { right_face_lines.push(Line::from("")); }
            right_face_lines.extend(right_lines);
            let right_face = Paragraph::new(right_face_lines)
                .alignment(Alignment::Center);
            frame_ref.render_widget(right_face, cols[2]);

            // ── Labels ──────────────────────────────────────────────
            let label_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(35),
                    Constraint::Percentage(30),
                    Constraint::Percentage(35),
                ])
                .split(layout[4]);

            let left_label = Paragraph::new(Line::from(Span::styled(
                "CHALLENGER",
                Style::default().fg(NEON_ORANGE).add_modifier(Modifier::BOLD),
            ))).alignment(Alignment::Center);
            frame_ref.render_widget(left_label, label_cols[0]);

            let vs_label = Paragraph::new(Line::from(Span::styled(
                "\u{26A1} FACE OFF \u{26A1}",
                Style::default().fg(vs_color(f)).add_modifier(Modifier::BOLD),
            ))).alignment(Alignment::Center);
            frame_ref.render_widget(vs_label, label_cols[1]);

            let right_label = Paragraph::new(Line::from(Span::styled(
                "DEFENDER",
                Style::default().fg(NEON_BLUE).add_modifier(Modifier::BOLD),
            ))).alignment(Alignment::Center);
            frame_ref.render_widget(right_label, label_cols[2]);

            // ── Star CTA (interactive) ────────────────────────────
            if !star_acknowledged {
                let star_pulse = match (f / 2) % 4 {
                    0 => Color::Rgb(255, 215, 0),
                    1 => Color::Rgb(255, 235, 80),
                    2 => Color::Rgb(255, 200, 50),
                    _ => Color::Rgb(255, 245, 120),
                };
                let star_msg = Paragraph::new(Line::from(vec![
                    Span::styled("  \u{2B50} ", Style::default().fg(star_pulse)),
                    Span::styled(
                        "Press ",
                        Style::default().fg(DIM_TEXT),
                    ),
                    Span::styled(
                        "[S]",
                        Style::default().fg(star_pulse).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " to Star on GitHub \u{2014} it gives the dev a POWER-UP!",
                        Style::default().fg(DIM_TEXT),
                    ),
                    Span::styled(" \u{2B50}  ", Style::default().fg(star_pulse)),
                ]))
                .alignment(Alignment::Center);
                frame_ref.render_widget(star_msg, layout[5]);
            }

            // ── Footer hotkeys ─────────────────────────────────────
            let blink = (f / 3) % 2 == 0;
            let mut footer_spans = vec![
                Span::styled(
                    " \u{23CE} ENTER",
                    Style::default().fg(if blink { NEON_GREEN } else { Color::Rgb(40, 80, 40) }).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Start  ", Style::default().fg(DIM_TEXT)),
            ];
            if !star_acknowledged {
                footer_spans.push(Span::styled(
                    "S",
                    Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD),
                ));
                footer_spans.push(Span::styled(" \u{2B50} Star  ", Style::default().fg(DIM_TEXT)));
            }
            footer_spans.push(Span::styled(
                "Q",
                Style::default().fg(NEON_RED).add_modifier(Modifier::BOLD),
            ));
            footer_spans.push(Span::styled(" Quit", Style::default().fg(DIM_TEXT)));

            let prompt = Paragraph::new(Line::from(footer_spans))
                .alignment(Alignment::Center)
                .block(Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(Color::Rgb(50, 50, 70))));
            frame_ref.render_widget(prompt, layout[7]);
        })?;

        frame = frame.wrapping_add(1);

        // Animation tick: ~5fps, poll for key events
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    return match key.code {
                        KeyCode::Char('s') | KeyCode::Char('S') if !star_acknowledged => Ok(true),
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Ok(false),
                        _ => Ok(false),
                    };
                }
            }
        }
    }
}
