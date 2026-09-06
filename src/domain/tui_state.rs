use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TuiTab {
    Invitations,
    Messages,
    Drafts,
}

impl TuiTab {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Invitations => "Invitations [1]",
            Self::Messages => "Messages [2]",
            Self::Drafts => "Drafts [3]",
        }
    }

    pub fn all() -> [TuiTab; 3] {
        [Self::Invitations, Self::Messages, Self::Drafts]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TuiItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub badge: String,
    pub detail_body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiAction {
    NextItem,
    PrevItem,
    NextTab,
    PrevTab,
    SetTab(TuiTab),
    Select,
    Quit,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiState {
    pub active_tab: TuiTab,
    pub selected_index: usize,
    pub items: Vec<TuiItem>,
    pub status_message: String,
    pub should_quit: bool,
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            active_tab: TuiTab::Invitations,
            selected_index: 0,
            items: Vec::new(),
            status_message: "Ready | [j/k] Navigate | [Tab/1-3] Switch Tab | [q] Quit".to_string(),
            should_quit: false,
        }
    }

    pub fn with_items(active_tab: TuiTab, items: Vec<TuiItem>) -> Self {
        Self {
            active_tab,
            selected_index: 0,
            items,
            status_message: "Ready | [j/k] Navigate | [Tab/1-3] Switch Tab | [q] Quit".to_string(),
            should_quit: false,
        }
    }

    pub fn handle_action(&mut self, action: TuiAction) {
        match action {
            TuiAction::NextItem => self.next_item(),
            TuiAction::PrevItem => self.prev_item(),
            TuiAction::NextTab => self.next_tab(),
            TuiAction::PrevTab => self.prev_tab(),
            TuiAction::SetTab(tab) => {
                self.active_tab = tab;
                self.selected_index = 0;
            }
            TuiAction::Select => {
                if let Some(item) = self.selected_item() {
                    self.status_message = format!("Selected: {}", item.title);
                }
            }
            TuiAction::Quit => {
                self.should_quit = true;
            }
            TuiAction::None => {}
        }
    }

    pub fn next_item(&mut self) {
        if !self.items.is_empty() {
            if self.selected_index + 1 < self.items.len() {
                self.selected_index += 1;
            } else {
                self.selected_index = 0; // Wrap around
            }
        }
    }

    pub fn prev_item(&mut self) {
        if !self.items.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                self.selected_index = self.items.len().saturating_sub(1); // Wrap around
            }
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            TuiTab::Invitations => TuiTab::Messages,
            TuiTab::Messages => TuiTab::Drafts,
            TuiTab::Drafts => TuiTab::Invitations,
        };
        self.selected_index = 0;
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = match self.active_tab {
            TuiTab::Invitations => TuiTab::Drafts,
            TuiTab::Messages => TuiTab::Invitations,
            TuiTab::Drafts => TuiTab::Messages,
        };
        self.selected_index = 0;
    }

    pub fn selected_item(&self) -> Option<&TuiItem> {
        self.items.get(self.selected_index)
    }
}
