use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use ratatui::{Frame, Terminal};

use crate::domain::{TuiAction, TuiItem, TuiState, TuiTab};
use crate::errors::AppError;

/// Renders the TUI components into any ratatui Frame (TestBackend or Terminal)
pub fn render_tui(f: &mut Frame, state: &TuiState) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Tabs Bar
            Constraint::Min(5),    // Main Split-Pane
            Constraint::Length(3), // Bottom Status / Help Bar
        ])
        .split(size);

    render_tabs(f, chunks[0], state);
    render_split_pane(f, chunks[1], state);
    render_status_bar(f, chunks[2], state);
}

fn render_tabs(f: &mut Frame, area: Rect, state: &TuiState) {
    let titles: Vec<Line> = TuiTab::all()
        .iter()
        .map(|t| {
            let style = if *t == state.active_tab {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(vec![Span::styled(t.title(), style)])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" LinkedIn CLI (linkedin) TUI "),
        )
        .select(match state.active_tab {
            TuiTab::Invitations => 0,
            TuiTab::Messages => 1,
            TuiTab::Drafts => 2,
        })
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

fn render_split_pane(f: &mut Frame, area: Rect, state: &TuiState) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Left List Pane
            Constraint::Percentage(60), // Right Detail Pane
        ])
        .split(area);

    render_item_list(f, main_chunks[0], state);
    render_detail_pane(f, main_chunks[1], state);
}

fn render_item_list(f: &mut Frame, area: Rect, state: &TuiState) {
    let title = match state.active_tab {
        TuiTab::Invitations => " Pending Invitations ",
        TuiTab::Messages => " Conversations ",
        TuiTab::Drafts => " Staged Drafts ",
    };

    let items: Vec<ListItem> = if state.items.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            "  <No items found in this view>",
            Style::default().fg(Color::DarkGray),
        )]))]
    } else {
        state
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let is_selected = idx == state.selected_index;
                let prefix = if is_selected { "▶ " } else { "  " };

                let title_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let badge_style = Style::default().fg(Color::Yellow);

                let line1 = Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(&item.title, title_style),
                    Span::raw(" "),
                    Span::styled(format!("[{}]", item.badge), badge_style),
                ]);

                let line2 = Line::from(vec![
                    Span::raw("    "),
                    Span::styled(&item.subtitle, Style::default().fg(Color::Gray)),
                ]);

                ListItem::new(vec![line1, line2])
            })
            .collect()
    };

    let list_widget = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(list_widget, area);
}

fn render_detail_pane(f: &mut Frame, area: Rect, state: &TuiState) {
    let (title, content) = if let Some(item) = state.selected_item() {
        let title = format!(" Details: {} ", item.title);
        let content = format!(
            "ID: {}\nBadge/Status: {}\nSubtitle: {}\n\n-- Content --\n{}",
            item.id, item.badge, item.subtitle, item.detail_body
        );
        (title, content)
    } else {
        (" Details ".to_string(), "No item selected.".to_string())
    };

    let paragraph = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, state: &TuiState) {
    let p = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(
            &state.status_message,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Status & Keybindings "),
    );

    f.render_widget(p, area);
}

/// Helper to render directly into any ratatui Terminal instance (e.g. TestBackend)
pub fn render_tui_terminal<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &TuiState,
) -> Result<(), std::io::Error> {
    terminal.draw(|f| render_tui(f, state))?;
    Ok(())
}

/// Helper to create mock sample data for the TUI views
pub fn create_mock_tui_items(tab: TuiTab) -> Vec<TuiItem> {
    match tab {
        TuiTab::Invitations => vec![
            TuiItem {
                id: "inv-1".to_string(),
                title: "Dave Recruiter".to_string(),
                subtitle: "Tech Talent Partner @ Apex".to_string(),
                badge: "NOTE".to_string(),
                detail_body: "Hi! Would love to connect regarding Rust and Systems roles.".to_string(),
            },
            TuiItem {
                id: "inv-2".to_string(),
                title: "Eve Engineer".to_string(),
                subtitle: "Distributed Systems Developer".to_string(),
                badge: "DIRECT".to_string(),
                detail_body: "Eve wants to join your network.".to_string(),
            },
        ],
        TuiTab::Messages => vec![
            TuiItem {
                id: "msg-1".to_string(),
                title: "Alice Smith".to_string(),
                subtitle: "Staff Engineer @ InfraCorp".to_string(),
                badge: "UNREAD".to_string(),
                detail_body: "Alice: Thanks for sharing the benchmarks on the new async pipeline!".to_string(),
            },
            TuiItem {
                id: "msg-2".to_string(),
                title: "Bob PM".to_string(),
                subtitle: "Principal PM @ CloudTech".to_string(),
                badge: "READ".to_string(),
                detail_body: "Bob: Let's catch up next week on API integration deadlines.".to_string(),
            },
        ],
        TuiTab::Drafts => vec![
            TuiItem {
                id: "draft-01".to_string(),
                title: "Rust CLI Launch Announcement".to_string(),
                subtitle: "Scheduled for 2026-09-07 14:30 UTC".to_string(),
                badge: "SCHEDULED".to_string(),
                detail_body: "Excited to share our high-performance LinkedIn CLI rewrite! 🚀\nZero unneeded dependencies, full Unix composability.".to_string(),
            },
            TuiItem {
                id: "draft-02".to_string(),
                title: "Drafting Architecture Post".to_string(),
                subtitle: "No schedule date set".to_string(),
                badge: "DRAFT".to_string(),
                detail_body: "Notes on anti-detection browser automation and CDP interception patterns.".to_string(),
            },
        ],
    }
}

/// Converts terminal crossterm KeyCode to domain TuiAction
pub fn map_key_to_tui_action(key: crossterm::event::KeyCode) -> TuiAction {
    use crossterm::event::KeyCode;
    match key {
        KeyCode::Char('q') | KeyCode::Esc => TuiAction::Quit,
        KeyCode::Char('j') | KeyCode::Down => TuiAction::NextItem,
        KeyCode::Char('k') | KeyCode::Up => TuiAction::PrevItem,
        KeyCode::Tab => TuiAction::NextTab,
        KeyCode::BackTab => TuiAction::PrevTab,
        KeyCode::Char('1') => TuiAction::SetTab(TuiTab::Invitations),
        KeyCode::Char('2') => TuiAction::SetTab(TuiTab::Messages),
        KeyCode::Char('3') => TuiAction::SetTab(TuiTab::Drafts),
        KeyCode::Enter | KeyCode::Char(' ') => TuiAction::Select,
        _ => TuiAction::None,
    }
}

/// Interactive TUI loop using crossterm terminal
pub fn run_interactive_tui(mut state: TuiState) -> Result<(), AppError> {
    use crossterm::event::{self, Event, KeyEventKind};
    use crossterm::execute;
    use crossterm::terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    };
    use ratatui::backend::CrosstermBackend;
    use std::io::stdout;

    enable_raw_mode().map_err(AppError::Io)?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).map_err(AppError::Io)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend).map_err(AppError::Io)?;

    let res = (|| {
        loop {
            terminal.draw(|f| render_tui(f, &state))?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let action = map_key_to_tui_action(key.code);
                    state.handle_action(action);
                    if state.should_quit {
                        break;
                    }
                }
            }
        }
        Ok::<(), std::io::Error>(())
    })();

    // Restore terminal safely
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    res.map_err(AppError::Io)
}
