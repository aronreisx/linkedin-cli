use std::collections::VecDeque;
use std::io::{self, Write};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::domain::{Invitation, TriageAction, TriageReader};
use crate::errors::AppError;

/// Interactive terminal reader using crossterm raw-mode single keypresses
pub struct TerminalTriageReader {
    raw_active: bool,
}

impl TerminalTriageReader {
    pub fn new() -> Self {
        Self { raw_active: false }
    }

    fn render_prompt(&self, invite: &Invitation) {
        let name = invite.name.as_deref().unwrap_or("Unknown");
        let headline = invite.headline.as_deref().unwrap_or("No headline available");
        let url = &invite.profile_url;
        let note = invite.note.as_deref().unwrap_or("<No note attached>");

        eprintln!("\n╭─────────────────────────────────────────────────────────────╮");
        eprintln!("│ INVITATION TRIAGE                                           │");
        eprintln!("├─────────────────────────────────────────────────────────────┤");
        eprintln!("│ Name:     {:<49} │", name);
        eprintln!("│ Headline: {:<49} │", headline.chars().take(49).collect::<String>());
        eprintln!("│ URL:      {:<49} │", url.chars().take(49).collect::<String>());
        eprintln!("│ Note:     {:<49} │", note.chars().take(49).collect::<String>());
        eprintln!("├─────────────────────────────────────────────────────────────┤");
        eprintln!("│ Actions:  [a]ccept   [i]gnore   [s]kip / space   [q]uit     │");
        eprintln!("╰─────────────────────────────────────────────────────────────╯");
        eprint!("Your decision: ");
        let _ = io::stderr().flush();
    }
}

impl Default for TerminalTriageReader {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TerminalTriageReader {
    fn drop(&mut self) {
        if self.raw_active {
            let _ = disable_raw_mode();
        }
    }
}

impl TriageReader for TerminalTriageReader {
    fn read_action(&mut self, invite: &Invitation) -> Result<TriageAction, AppError> {
        self.render_prompt(invite);

        enable_raw_mode().map_err(AppError::Io)?;
        self.raw_active = true;

        let action = loop {
            if let Ok(Event::Key(key_event)) = event::read() {
                if key_event.kind == KeyEventKind::Press {
                    match key_event.code {
                        KeyCode::Char('a') | KeyCode::Char('A') => break TriageAction::Accept,
                        KeyCode::Char('i') | KeyCode::Char('I') => break TriageAction::Ignore,
                        KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Char(' ') => break TriageAction::Skip,
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break TriageAction::Quit,
                        _ => {}
                    }
                }
            }
        };

        disable_raw_mode().map_err(AppError::Io)?;
        self.raw_active = false;

        let action_str = match action {
            TriageAction::Accept => "ACCEPTED",
            TriageAction::Ignore => "IGNORED",
            TriageAction::Skip => "SKIPPED",
            TriageAction::Quit => "QUIT",
        };
        eprintln!(" {action_str}");

        Ok(action)
    }
}

/// In-memory mock reader for automated testing
pub struct MockTriageReader {
    decisions: VecDeque<TriageAction>,
}

impl MockTriageReader {
    pub fn new(decisions: Vec<TriageAction>) -> Self {
        Self {
            decisions: VecDeque::from(decisions),
        }
    }
}

impl TriageReader for MockTriageReader {
    fn read_action(&mut self, _invite: &Invitation) -> Result<TriageAction, AppError> {
        Ok(self.decisions.pop_front().unwrap_or(TriageAction::Skip))
    }
}
