use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use assert_cmd::Command;
use async_trait::async_trait;
use serde_json::json;
use tempfile::NamedTempFile;

use linkedin::domain::{
    calculate_dispatch_jitter, check_content_variance, render_batch_templates,
    triage_invitations, validate_batch_limits, Invitation, InvitationManager, TriageAction,
};
use linkedin::errors::AppError;
use linkedin::infra::MockTriageReader;

// --- Mock Invitation Manager for Triage Tests ---

#[derive(Default, Clone)]
struct MockInvitationManager {
    pub accepted_calls: Arc<AtomicUsize>,
    pub ignored_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl InvitationManager for MockInvitationManager {
    async fn list_sent_invitations(&self, _scroll: u32, _limit: Option<usize>) -> Result<Vec<Invitation>, AppError> {
        Ok(vec![])
    }

    async fn list_received_invitations(&self, _scroll: u32, _limit: Option<usize>) -> Result<Vec<Invitation>, AppError> {
        Ok(vec![])
    }

    async fn withdraw_invitation(&self, _handle_or_url: &str) -> Result<bool, AppError> {
        Ok(true)
    }

    async fn accept_invitation(&self, _handle_or_url: &str) -> Result<bool, AppError> {
        self.accepted_calls.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }

    async fn ignore_invitation(&self, _handle_or_url: &str) -> Result<bool, AppError> {
        self.ignored_calls.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }
}

fn sample_invitations() -> Vec<Invitation> {
    vec![
        Invitation {
            profile_url: "https://www.linkedin.com/in/recruiter-dave/".to_string(),
            public_identifier: "recruiter-dave".to_string(),
            name: Some("Dave Recruiter".to_string()),
            headline: Some("Tech Recruiter".to_string()),
            sent_or_received_at: None,
            note: Some("Great profile, let's connect!".to_string()),
        },
        Invitation {
            profile_url: "https://www.linkedin.com/in/engineer-eve/".to_string(),
            public_identifier: "engineer-eve".to_string(),
            name: Some("Eve Engineer".to_string()),
            headline: Some("Rust Developer".to_string()),
            sent_or_received_at: None,
            note: None,
        },
        Invitation {
            profile_url: "https://www.linkedin.com/in/pm-bob/".to_string(),
            public_identifier: "pm-bob".to_string(),
            name: Some("Bob PM".to_string()),
            headline: Some("Product Manager".to_string()),
            sent_or_received_at: None,
            note: Some("Looking for collaborators".to_string()),
        },
        Invitation {
            profile_url: "https://www.linkedin.com/in/founder-charlie/".to_string(),
            public_identifier: "founder-charlie".to_string(),
            name: Some("Charlie Founder".to_string()),
            headline: Some("Founder @ Startup".to_string()),
            sent_or_received_at: None,
            note: None,
        },
    ]
}

// =========================================================================
// 1. Interactive Triage Tests
// =========================================================================

#[tokio::test]
async fn test_interactive_triage_actions_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    let queue = sample_invitations();
    let manager = MockInvitationManager::default();

    // Decisions: Accept first, Ignore second, Skip third, Quit on fourth
    let decisions = vec![
        TriageAction::Accept,
        TriageAction::Ignore,
        TriageAction::Skip,
        TriageAction::Quit,
    ];
    let mut reader = MockTriageReader::new(decisions);

    let result = triage_invitations(&queue, &mut reader, &manager, false).await?;

    assert_eq!(result.total_reviewed, 3);
    assert_eq!(result.accepted, 1);
    assert_eq!(result.ignored, 1);
    assert_eq!(result.skipped, 1);
    assert!(result.quit_early);

    // Verify manager mutations occurred
    assert_eq!(manager.accepted_calls.load(Ordering::SeqCst), 1);
    assert_eq!(manager.ignored_calls.load(Ordering::SeqCst), 1);

    Ok(())
}

#[tokio::test]
async fn test_interactive_triage_dry_run_zero_side_effects() -> Result<(), Box<dyn std::error::Error>> {
    let queue = sample_invitations();
    let manager = MockInvitationManager::default();

    // Decisions: Accept first, Ignore second
    let decisions = vec![
        TriageAction::Accept,
        TriageAction::Ignore,
    ];
    let mut reader = MockTriageReader::new(decisions);

    // Run in dry-run mode
    let result = triage_invitations(&queue[..2], &mut reader, &manager, true).await?;

    assert_eq!(result.total_reviewed, 2);
    assert_eq!(result.accepted, 1);
    assert_eq!(result.ignored, 1);
    assert!(!result.quit_early);

    // Assert ZERO side effects on manager
    assert_eq!(manager.accepted_calls.load(Ordering::SeqCst), 0);
    assert_eq!(manager.ignored_calls.load(Ordering::SeqCst), 0);

    Ok(())
}

#[test]
fn test_cli_invite_triage_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").expect("binary linkedin exists");
    cmd.args(["invite", "triage", "--dry-run", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"dry_run\": true"))
        .stdout(predicates::str::contains("\"resource\": \"invite\""))
        .stdout(predicates::str::contains("\"action\": \"triage\""));
}

#[test]
fn test_cli_invite_triage_filter_note_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").expect("binary linkedin exists");
    cmd.args(["invite", "triage", "--filter-has-note", "--dry-run", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"filter_has_note\": true"))
        .stdout(predicates::str::contains("\"queue_size\": 1"));
}

// =========================================================================
// 2. Safe Batch Messaging Tests
// =========================================================================

#[test]
fn test_safe_messaging_template_rendering() -> Result<(), Box<dyn std::error::Error>> {
    let template = "Hi {{ name }}, saw your work at {{ company }} on {{ role }}!";
    let recipients = vec![
        json!({ "recipient": "alice", "name": "Alice", "company": "RustCorp", "role": "Systems Engineer" }),
        json!({ "recipient": "bob", "name": "Bob", "company": "CloudInc", "role": "DevOps Architect" }),
    ];

    let rendered = render_batch_templates(template, &recipients)?;
    assert_eq!(rendered.len(), 2);
    assert_eq!(rendered[0].recipient, "alice");
    assert_eq!(rendered[0].text, "Hi Alice, saw your work at RustCorp on Systems Engineer!");
    assert_eq!(rendered[1].recipient, "bob");
    assert_eq!(rendered[1].text, "Hi Bob, saw your work at CloudInc on DevOps Architect!");
    assert_eq!(rendered[0].utf16_len, rendered[0].text.encode_utf16().count());

    Ok(())
}

#[test]
fn test_safe_messaging_content_variance_check() -> Result<(), Box<dyn std::error::Error>> {
    // 1. High variance case (different recipients & varied text)
    let diverse_messages = vec![
        linkedin::domain::RenderedMessage {
            recipient: "alice".to_string(),
            text: "Hi Alice, huge fan of your async Rust work at Acme!".to_string(),
            utf16_len: 52,
        },
        linkedin::domain::RenderedMessage {
            recipient: "bob".to_string(),
            text: "Greetings Bob, would love to connect and discuss Kubernetes scaling.".to_string(),
            utf16_len: 68,
        },
        linkedin::domain::RenderedMessage {
            recipient: "carol".to_string(),
            text: "Hey Carol! Noticed your recent publication on distributed transactions.".to_string(),
            utf16_len: 71,
        },
    ];
    let report_diverse = check_content_variance(&diverse_messages)?;
    assert!(report_diverse.passes_variance_check);
    assert_eq!(report_diverse.unique_messages, 3);
    assert!(report_diverse.average_similarity < 0.50);

    // 2. Low variance spam case (identical messages)
    let spam_messages = vec![
        linkedin::domain::RenderedMessage {
            recipient: "user1".to_string(),
            text: "Hello, check out our new exciting crypto platform!".to_string(),
            utf16_len: 51,
        },
        linkedin::domain::RenderedMessage {
            recipient: "user2".to_string(),
            text: "Hello, check out our new exciting crypto platform!".to_string(),
            utf16_len: 51,
        },
        linkedin::domain::RenderedMessage {
            recipient: "user3".to_string(),
            text: "Hello, check out our new exciting crypto platform!".to_string(),
            utf16_len: 51,
        },
    ];
    let report_spam = check_content_variance(&spam_messages)?;
    assert!(!report_spam.passes_variance_check);
    assert_eq!(report_spam.unique_messages, 1);
    assert!(report_spam.warning.is_some());

    Ok(())
}

#[test]
fn test_safe_messaging_batch_limit_enforcement() {
    // Batch size within ceiling (<= 15)
    assert!(validate_batch_limits(10, 15).is_ok());
    assert!(validate_batch_limits(15, 15).is_ok());

    // Batch size exceeding default ceiling (> 15)
    let err = validate_batch_limits(16, 15);
    assert!(err.is_err());
    if let Err(AppError::ConnectionLimit(msg)) = err {
        assert!(msg.contains("Batch size 16 exceeds safe account protection ceiling of 15"));
    } else {
        panic!("Expected AppError::ConnectionLimit for batch limit exceeded");
    }

    // Zero recipients error
    assert!(validate_batch_limits(0, 15).is_err());
}

#[test]
fn test_safe_messaging_randomized_dispatch_jitter() {
    let min_secs = 45;
    let max_secs = 120;

    let mut samples = Vec::new();
    for _ in 0..100 {
        let secs = calculate_dispatch_jitter(min_secs, max_secs);
        assert!(
            secs >= min_secs && secs <= max_secs,
            "Jitter {secs} outside expected bounds [{min_secs}, {max_secs}]"
        );
        samples.push(secs);
    }

    // Ensure randomization is active (variance between min and max)
    let min_sampled = *samples.iter().min().unwrap();
    let max_sampled = *samples.iter().max().unwrap();
    assert!(min_sampled < max_sampled, "Random jitter must produce variable delays");
}

#[test]
fn test_cli_message_send_batch_dry_run() -> Result<(), Box<dyn std::error::Error>> {
    let mut tmp_recipients = NamedTempFile::new()?;
    let recipients_json = json!([
        { "recipient": "alice", "name": "Alice", "company": "Systems Co" },
        { "recipient": "bob", "name": "Bob", "company": "Infra LLC" }
    ]);
    tmp_recipients.write_all(recipients_json.to_string().as_bytes())?;

    let mut cmd = Command::cargo_bin("linkedin")?;
    cmd.args([
        "message", "send-batch",
        "--template", "Hi {{ name }} at {{ company }}!",
        "--recipients", tmp_recipients.path().to_str().unwrap(),
        "--dry-run",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("\"dry_run\": true"))
    .stdout(predicates::str::contains("\"action\": \"send_batch\""))
    .stdout(predicates::str::contains("Systems Co"));

    Ok(())
}
