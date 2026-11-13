use assert_cmd::Command;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use linkedin::domain::{TuiAction, TuiItem, TuiState, TuiTab};
use linkedin::infra::{create_mock_tui_items, map_key_to_tui_action, render_tui_terminal};

#[test]
fn test_tui_split_pane_rendering_without_panics() -> Result<(), Box<dyn std::error::Error>> {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend)?;

    let items = create_mock_tui_items(TuiTab::Invitations);
    let state = TuiState::with_items(TuiTab::Invitations, items);

    // Render into TestBackend
    render_tui_terminal(&mut terminal, &state)?;

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    // Verify key UI elements are rendered
    assert!(content.contains("LinkedIn CLI (linkedin) TUI"));
    assert!(content.contains("Pending Invitations"));
    assert!(content.contains("Dave Recruiter"));
    assert!(content.contains("Eve Engineer"));
    assert!(content.contains("Details"));
    assert!(content.contains("Status & Keybindings"));

    Ok(())
}

#[test]
fn test_tui_navigation_and_scrolling_state_transitions() {
    let items = vec![
        TuiItem {
            id: "item-1".to_string(),
            title: "Item One".to_string(),
            subtitle: "Sub 1".to_string(),
            badge: "A".to_string(),
            detail_body: "Body 1".to_string(),
        },
        TuiItem {
            id: "item-2".to_string(),
            title: "Item Two".to_string(),
            subtitle: "Sub 2".to_string(),
            badge: "B".to_string(),
            detail_body: "Body 2".to_string(),
        },
        TuiItem {
            id: "item-3".to_string(),
            title: "Item Three".to_string(),
            subtitle: "Sub 3".to_string(),
            badge: "C".to_string(),
            detail_body: "Body 3".to_string(),
        },
    ];

    let mut state = TuiState::with_items(TuiTab::Invitations, items);
    assert_eq!(state.selected_index, 0);
    assert_eq!(state.selected_item().unwrap().id, "item-1");

    // Next item (j / down)
    state.handle_action(TuiAction::NextItem);
    assert_eq!(state.selected_index, 1);
    assert_eq!(state.selected_item().unwrap().id, "item-2");

    // Next item (j / down)
    state.handle_action(TuiAction::NextItem);
    assert_eq!(state.selected_index, 2);
    assert_eq!(state.selected_item().unwrap().id, "item-3");

    // Wrap-around on next item
    state.handle_action(TuiAction::NextItem);
    assert_eq!(state.selected_index, 0);

    // Prev item (k / up) wrap-around
    state.handle_action(TuiAction::PrevItem);
    assert_eq!(state.selected_index, 2);

    // Prev item backwards
    state.handle_action(TuiAction::PrevItem);
    assert_eq!(state.selected_index, 1);

    // Tab switching: Invitations -> Messages -> Drafts -> Invitations
    assert_eq!(state.active_tab, TuiTab::Invitations);
    state.handle_action(TuiAction::NextTab);
    assert_eq!(state.active_tab, TuiTab::Messages);
    assert_eq!(state.selected_index, 0);

    state.handle_action(TuiAction::NextTab);
    assert_eq!(state.active_tab, TuiTab::Drafts);

    state.handle_action(TuiAction::NextTab);
    assert_eq!(state.active_tab, TuiTab::Invitations);

    // Quit action
    assert!(!state.should_quit);
    state.handle_action(TuiAction::Quit);
    assert!(state.should_quit);
}

#[test]
fn test_tui_key_mapping() {
    use crossterm::event::KeyCode;

    assert_eq!(map_key_to_tui_action(KeyCode::Char('q')), TuiAction::Quit);
    assert_eq!(map_key_to_tui_action(KeyCode::Esc), TuiAction::Quit);
    assert_eq!(
        map_key_to_tui_action(KeyCode::Char('j')),
        TuiAction::NextItem
    );
    assert_eq!(map_key_to_tui_action(KeyCode::Down), TuiAction::NextItem);
    assert_eq!(
        map_key_to_tui_action(KeyCode::Char('k')),
        TuiAction::PrevItem
    );
    assert_eq!(map_key_to_tui_action(KeyCode::Up), TuiAction::PrevItem);
    assert_eq!(map_key_to_tui_action(KeyCode::Tab), TuiAction::NextTab);
    assert_eq!(
        map_key_to_tui_action(KeyCode::Char('1')),
        TuiAction::SetTab(TuiTab::Invitations)
    );
    assert_eq!(
        map_key_to_tui_action(KeyCode::Char('2')),
        TuiAction::SetTab(TuiTab::Messages)
    );
    assert_eq!(
        map_key_to_tui_action(KeyCode::Char('3')),
        TuiAction::SetTab(TuiTab::Drafts)
    );
    assert_eq!(map_key_to_tui_action(KeyCode::Enter), TuiAction::Select);
}

#[test]
fn test_cli_tui_launch_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").expect("linkedin binary exists");
    cmd.args(["tui", "--tab", "drafts", "--dry-run", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"dry_run\": true"))
        .stdout(predicates::str::contains("\"resource\": \"tui\""))
        .stdout(predicates::str::contains("\"action\": \"launch\""))
        .stdout(predicates::str::contains("\"initial_tab\": \"Drafts\""));
}
