use assert_cmd::Command;

use linkedin::domain::{
    generate_cron_entry, generate_systemd_service, generate_systemd_timer, Draft, DraftState,
};
use linkedin::infra::{DraftStore, SqliteDraftStore};

#[test]
fn test_draft_store_sqlite_lifecycle_transitions() -> Result<(), Box<dyn std::error::Error>> {
    let store = SqliteDraftStore::in_memory()?;

    // 1. Create a draft without schedule -> State = Draft
    let draft1 = store.create_draft("Unscheduled quick thought on Rust", None, None)?;
    assert_eq!(draft1.state, DraftState::Draft);
    assert!(draft1.scheduled_at.is_none());

    // 2. Create a scheduled draft -> State = Scheduled
    let schedule_time = "2026-09-07T14:30:00Z";
    let draft2 = store.create_draft(
        "Shipping LinkedIn CLI v1.0! 🚀",
        Some("assets/banner.png"),
        Some(schedule_time),
    )?;
    assert_eq!(draft2.state, DraftState::Scheduled);
    assert_eq!(draft2.scheduled_at.as_deref(), Some(schedule_time));
    assert_eq!(draft2.media_path.as_deref(), Some("assets/banner.png"));

    // 3. List drafts
    let all_drafts = store.list_drafts(None)?;
    assert_eq!(all_drafts.len(), 2);

    let scheduled_only = store.list_drafts(Some(DraftState::Scheduled))?;
    assert_eq!(scheduled_only.len(), 1);
    assert_eq!(scheduled_only[0].id, draft2.id);

    // 4. Transition Scheduled -> Published
    let published = store.transition_state(
        &draft2.id,
        DraftState::Published,
        Some("urn:li:share:987654321"),
        None,
    )?;
    assert_eq!(published.state, DraftState::Published);
    assert_eq!(
        published.post_urn.as_deref(),
        Some("urn:li:share:987654321")
    );

    // 5. Invalid transition: Published cannot go back to Scheduled
    let invalid = store.transition_state(&draft2.id, DraftState::Scheduled, None, None);
    assert!(invalid.is_err());

    // 6. Transition Draft -> Scheduled -> Failed
    let scheduled1 = store.transition_state(&draft1.id, DraftState::Scheduled, None, None)?;
    assert_eq!(scheduled1.state, DraftState::Scheduled);

    let failed = store.transition_state(
        &draft1.id,
        DraftState::Failed,
        None,
        Some("Network timeout clearing captcha checkpoint"),
    )?;
    assert_eq!(failed.state, DraftState::Failed);
    assert!(failed.error_message.unwrap().contains("captcha checkpoint"));

    // 7. Delete draft
    let deleted = store.delete_draft(&draft1.id)?;
    assert!(deleted);
    let fetch_deleted = store.get_draft(&draft1.id)?;
    assert!(fetch_deleted.is_none());

    Ok(())
}

#[test]
fn test_draft_cron_syntax_generation() -> Result<(), Box<dyn std::error::Error>> {
    let draft = Draft {
        id: "draft-42a1".to_string(),
        text: "Automated morning post".to_string(),
        media_path: None,
        scheduled_at: Some("2026-09-07T09:15:00Z".to_string()),
        state: DraftState::Scheduled,
        post_urn: None,
        error_message: None,
        created_at: "2026-09-06T20:00:00Z".to_string(),
        updated_at: "2026-09-06T20:00:00Z".to_string(),
    };

    let cron_line = generate_cron_entry(&draft, Some("/usr/local/bin/linkedin"))?;
    // 09:15 UTC on 7th of September
    assert_eq!(
        cron_line,
        "15 09 07 09 * /usr/local/bin/linkedin post publish --draft-id draft-42a1"
    );

    Ok(())
}

#[test]
fn test_draft_systemd_unit_generation() -> Result<(), Box<dyn std::error::Error>> {
    let draft = Draft {
        id: "draft-88bc".to_string(),
        text: "Evening release recap".to_string(),
        media_path: Some("assets/release.png".to_string()),
        scheduled_at: Some("2026-09-07T18:00:00Z".to_string()),
        state: DraftState::Scheduled,
        post_urn: None,
        error_message: None,
        created_at: "2026-09-06T20:00:00Z".to_string(),
        updated_at: "2026-09-06T20:00:00Z".to_string(),
    };

    let service = generate_systemd_service(&draft, Some("/usr/bin/linkedin"));
    assert!(service.contains("[Unit]"));
    assert!(service.contains("Description=LinkedIn Scheduled Post Publisher (Draft draft-88bc)"));
    assert!(service.contains("ExecStart=/usr/bin/linkedin post publish --draft-id draft-88bc"));

    let timer = generate_systemd_timer(&draft)?;
    assert!(timer.contains("[Timer]"));
    assert!(timer.contains("OnCalendar=2026-09-07 18:00:00 UTC"));
    assert!(timer.contains("Persistent=true"));
    assert!(timer.contains("WantedBy=timers.target"));

    Ok(())
}

#[test]
fn test_cli_draft_add_and_export_dry_run() {
    // 1. Test canonical `linkedin post draft` subcommand
    let mut cmd_post_add = Command::cargo_bin("linkedin").expect("linkedin binary exists");
    cmd_post_add
        .args([
            "post",
            "draft",
            "add",
            "--text",
            "Launching new Rust systems tool!",
            "--schedule",
            "2026-09-07T10:00:00Z",
            "--dry-run",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"dry_run\": true"))
        .stdout(predicates::str::contains("\"resource\": \"post\""))
        .stdout(predicates::str::contains("\"action\": \"draft_add\""))
        .stdout(predicates::str::contains("2026-09-07T10:00:00Z"));

    let mut cmd_post_export = Command::cargo_bin("linkedin").expect("linkedin binary exists");
    cmd_post_export
        .args([
            "post",
            "draft",
            "export",
            "--id",
            "draft-1234",
            "--format",
            "cron",
            "--dry-run",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"dry_run\": true"))
        .stdout(predicates::str::contains("\"resource\": \"post\""))
        .stdout(predicates::str::contains("\"action\": \"draft_export\""))
        .stdout(predicates::str::contains("draft-1234"));

    // 2. Test backward-compatible `linkedin draft` alias
    let mut cmd_add = Command::cargo_bin("linkedin").expect("linkedin binary exists");
    cmd_add
        .args([
            "draft",
            "add",
            "--text",
            "Launching new Rust systems tool!",
            "--schedule",
            "2026-09-07T10:00:00Z",
            "--dry-run",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"dry_run\": true"))
        .stdout(predicates::str::contains("\"resource\": \"draft\""))
        .stdout(predicates::str::contains("\"action\": \"add\""))
        .stdout(predicates::str::contains("2026-09-07T10:00:00Z"));

    let mut cmd_export = Command::cargo_bin("linkedin").expect("linkedin binary exists");
    cmd_export
        .args([
            "draft",
            "export",
            "--id",
            "draft-1234",
            "--format",
            "cron",
            "--dry-run",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"dry_run\": true"))
        .stdout(predicates::str::contains("\"action\": \"export\""))
        .stdout(predicates::str::contains("draft-1234"));
}
