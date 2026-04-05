use std::io::{self, Stdout};
use std::path::Path;

use anyhow::Result;
use better_than_you::{open_path, AxisCard, BattleResult, SavedArtifacts};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;

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

fn title_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default().fg(Color::Rgb(255, 214, 107)).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::Rgb(99, 235, 211)))
}

fn axis_line(card: &AxisCard, winner_id: &str) -> Line<'static> {
    let gap_color = if card.leader == "tie" {
        Color::Cyan
    } else if card.leader == winner_id {
        Color::LightGreen
    } else {
        Color::LightRed
    };
    let gap = if card.leader == "tie" {
        "TIE ".to_string()
    } else if card.leader == winner_id {
        format!("+{:.1}", card.diff)
    } else {
        format!("-{:.1}", card.diff)
    };

    Line::from(vec![
        Span::styled(format!("{:<22}", card.label), Style::default().fg(Color::Rgb(245, 239, 228))),
        Span::raw(format!(" {:>5.1} ", card.left)),
        Span::styled(format!("{:>5.1}", card.right), Style::default().fg(Color::Rgb(141, 183, 255))),
        Span::raw("   "),
        Span::styled(gap, Style::default().fg(gap_color).add_modifier(Modifier::BOLD)),
    ])
}


pub fn select_menu(title: &str, subtitle: &[String], items: &[String], initial_index: usize) -> Result<Option<usize>> {
    let mut session = TuiSession::new()?;
    let mut selected = initial_index.min(items.len().saturating_sub(1));
    let mut state = ListState::default();
    state.select(Some(selected));

    loop {
        session.terminal.draw(|frame| {
            let area = frame.area();
            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Length(subtitle.len() as u16 + 2),
                    Constraint::Min(8),
                    Constraint::Length(2),
                ])
                .split(area);

            let header = Paragraph::new(title)
                .block(title_block("BetterThanYou"))
                .style(Style::default().fg(Color::Rgb(245, 239, 228)).add_modifier(Modifier::BOLD));
            frame.render_widget(header, sections[0]);

            let subtitle_lines = subtitle.iter().map(|line| Line::from(line.clone())).collect::<Vec<_>>();
            let subtitle_widget = Paragraph::new(subtitle_lines)
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)).title("Context"))
                .wrap(Wrap { trim: true });
            frame.render_widget(subtitle_widget, sections[1]);

            let items_widget = items
                .iter()
                .map(|item| ListItem::new(Line::from(item.clone())))
                .collect::<Vec<_>>();
            let list = List::new(items_widget)
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Rgb(255, 143, 66))).title("Actions"))
                .highlight_symbol("› ")
                .highlight_style(Style::default().fg(Color::Rgb(255, 214, 107)).add_modifier(Modifier::BOLD));
            frame.render_stateful_widget(list, sections[2], &mut state);

            let footer = Paragraph::new("↑/↓ move • Enter select • q or Esc back")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(footer, sections[3]);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                    state.select(Some(selected));
                }
                KeyCode::Down => {
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

fn draw_panel(frame: &mut ratatui::Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'static>>, border_color: Color) {
    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(title.to_string(), Style::default().fg(border_color).add_modifier(Modifier::BOLD)))
                .border_style(Style::default().fg(border_color)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

pub fn present_battle_view(result: &BattleResult, artifacts: &SavedArtifacts, footer_lines: &[String], on_open: Option<fn(&Path) -> Result<()>>) -> Result<()> {
    let mut session = TuiSession::new()?;

    loop {
        session.terminal.draw(|frame| {
            let area = frame.area();
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Length(6),
                    Constraint::Length((result.axis_cards.len() as u16) + 4),
                    Constraint::Length(9),
                    Constraint::Length((footer_lines.len() as u16).max(3)),
                ])
                .split(area);

            let hero = Paragraph::new(vec![
                Line::from(Span::styled("BETTERTHANYOU // CLI PORTRAIT BATTLE", Style::default().fg(Color::Rgb(245, 239, 228)).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(format!("WINNER // {}", result.winner.label.to_uppercase()), Style::default().fg(Color::Rgb(255, 214, 107)).add_modifier(Modifier::BOLD))),
            ])
            .block(title_block("Battle"));
            frame.render_widget(hero, layout[0]);

            let summary = vec![
                Line::from(format!("Judge  : {}", result.engine.model.clone().unwrap_or_else(|| result.engine.judge_mode.clone()))),
                Line::from(format!("Left   : {} {:.1}", result.inputs.left.label, result.scores.left.total)),
                Line::from(format!("Right  : {} {:.1}", result.inputs.right.label, result.scores.right.total)),
                Line::from(format!("Margin : {:.1}", result.winner.margin)),
            ];
            draw_panel(frame, layout[1], "Summary", summary, Color::Rgb(99, 235, 211));

            let axis_lines = result.axis_cards.iter().map(|card| axis_line(card, &result.winner.id)).collect::<Vec<_>>();
            draw_panel(frame, layout[2], "Ability Comparison", axis_lines, Color::Rgb(255, 143, 66));

            let analysis = vec![
                Line::from(format!("Overall: {}", result.sections.overall_take)),
                Line::from(""),
                Line::from(format!("Why    : {}", result.sections.why_this_won)),
                Line::from(""),
                Line::from(format!("Notes  : {}", result.sections.model_jury_notes)),
            ];
            draw_panel(frame, layout[3], "Analysis", analysis, Color::Rgb(141, 183, 255));

            let footer = footer_lines.iter().map(|line| Line::from(line.clone())).collect::<Vec<_>>();
            draw_panel(frame, layout[4], "Next", footer, Color::DarkGray);
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
