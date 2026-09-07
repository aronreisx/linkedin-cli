use std::io::Write;
use assert_cmd::Command;
use serde_json::json;
use tempfile::NamedTempFile;

use linkedin::domain::{
    calculate_dispatch_jitter, check_content_variance, render_batch_templates,
    validate_batch_limits,
};
use linkedin::errors::AppError;

// =========================================================================
// Safe Batch Messaging Tests
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
            attachment: None,
        },
        linkedin::domain::RenderedMessage {
            recipient: "bob".to_string(),
            text: "Greetings Bob, would love to connect and discuss Kubernetes scaling.".to_string(),
            utf16_len: 68,
            attachment: None,
        },
        linkedin::domain::RenderedMessage {
            recipient: "carol".to_string(),
            text: "Hey Carol! Noticed your recent publication on distributed transactions.".to_string(),
            utf16_len: 71,
            attachment: None,
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
            attachment: None,
        },
        linkedin::domain::RenderedMessage {
            recipient: "user2".to_string(),
            text: "Hello, check out our new exciting crypto platform!".to_string(),
            utf16_len: 51,
            attachment: None,
        },
        linkedin::domain::RenderedMessage {
            recipient: "user3".to_string(),
            text: "Hello, check out our new exciting crypto platform!".to_string(),
            utf16_len: 51,
            attachment: None,
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

#[test]
fn test_cli_message_send_batch_attachments_dry_run() -> Result<(), Box<dyn std::error::Error>> {
    let mut tmp_recipients = NamedTempFile::new()?;
    let recipients_json = json!([
        { "recipient": "alice", "name": "Alice", "attachment": "assets/alice_resume.pdf" },
        { "recipient": "bob", "name": "Bob" }
    ]);
    tmp_recipients.write_all(recipients_json.to_string().as_bytes())?;

    let mut cmd = Command::cargo_bin("linkedin")?;
    cmd.args([
        "message", "send-batch",
        "--template", "Hi {{ name }}!",
        "--recipients", tmp_recipients.path().to_str().unwrap(),
        "--attachment", "assets/default_deck.pdf",
        "--dry-run",
        "-o", "json",
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("assets/alice_resume.pdf"))
    .stdout(predicates::str::contains("assets/default_deck.pdf"));

    Ok(())
}
