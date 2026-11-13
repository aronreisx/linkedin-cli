use assert_cmd::Command;
use serde_json::json;
use std::io::Write;
use tempfile::NamedTempFile;

use linkedin::domain::{
    count_utf16_units, parse_markdown_post, parse_voyager_analytics, plan_media_upload,
    validate_post_content, PostAnalytics, LINKEDIN_POST_MAX_UTF16_CHARS,
};
use linkedin::errors::AppError;

// =========================================================================
// 1. UTF-16 Character Accounting & Post Ingestion Tests
// =========================================================================

#[test]
fn test_utf16_character_limit_calculations() {
    // Standard ASCII: 1 byte = 1 char = 1 UTF-16 code unit
    let ascii = "Hello World";
    assert_eq!(ascii.len(), 11);
    assert_eq!(ascii.chars().count(), 11);
    assert_eq!(count_utf16_units(ascii), 11);

    // Accented / 2-byte UTF-8: 1 char = 1 UTF-16 code unit
    let accented = "Café au lait";
    assert_eq!(accented.len(), 13); // 'é' is 2 bytes in UTF-8
    assert_eq!(accented.chars().count(), 12);
    assert_eq!(count_utf16_units(accented), 12);

    // Surrogate pair emoji: 4 bytes in UTF-8, 1 unicode char, but 2 UTF-16 code units!
    let rocket = "🚀";
    assert_eq!(rocket.len(), 4);
    assert_eq!(rocket.chars().count(), 1);
    assert_eq!(count_utf16_units(rocket), 2);

    // Multi-emoji string with crab & rocket
    let emoji_str = "Rust 🦀 & Rocket 🚀";
    assert_eq!(count_utf16_units(emoji_str), 19);

    // Zero-Width Joiner (ZWJ) composite emoji sequence (Family: 👨‍👩‍👧‍👦)
    let family = "👨‍👩‍👧‍👦";
    assert_eq!(count_utf16_units(family), 11);
}

#[test]
fn test_post_validation_boundary_limits() {
    // Exactly at the 3,000 UTF-16 limit
    let exact_post = "a".repeat(LINKEDIN_POST_MAX_UTF16_CHARS);
    let val_result = validate_post_content(&exact_post).expect("3,000 chars is valid");
    assert!(val_result.is_valid);
    assert_eq!(val_result.utf16_chars, LINKEDIN_POST_MAX_UTF16_CHARS);
    assert_eq!(val_result.excess_chars, 0);

    // Exceeding the limit with emojis
    let rocket_post = "🚀".repeat(1501);
    assert_eq!(count_utf16_units(&rocket_post), 3002);
    let err = validate_post_content(&rocket_post);
    assert!(err.is_err());
    if let Err(AppError::InvalidInput(msg)) = err {
        assert!(msg.contains("Post exceeds LinkedIn 3,000 UTF-16 character limit"));
        assert!(msg.contains("excess: 2"));
    } else {
        panic!("Expected AppError::InvalidInput for post exceeding limit");
    }
}

#[test]
fn test_post_markdown_parsing_and_frontmatter() -> Result<(), Box<dyn std::error::Error>> {
    let raw_md = r#"---
title: "Shipping High-Performance CLI Tools in Rust"
visibility: PUBLIC
tags: ["rust", "cli", "systems"]
---

Building modern CLI tooling requires deep attention to ergonomics and speed.

Check out our architecture:
![Architecture Diagram](assets/diagram.png)

Stay tuned for more updates!
"#;

    let parsed = parse_markdown_post(raw_md)?;
    assert_eq!(parsed.media_path.as_deref(), Some("assets/diagram.png"));
    assert!(parsed
        .body
        .contains("Building modern CLI tooling requires deep attention"));
    assert!(!parsed.body.contains("---")); // Frontmatter must be stripped
    assert!(parsed.utf16_length > 0);

    Ok(())
}

#[test]
fn test_post_media_upload_multi_step_protocol() {
    let plan = plan_media_upload("assets/architecture.png");
    assert_eq!(plan.media_file, "assets/architecture.png");
    assert!(plan.asset_urn.starts_with("urn:li:digitalmediaAsset:"));
    assert!(plan
        .upload_url
        .starts_with("https://api.linkedin.com/mediaUpload/temp/"));
    assert_eq!(plan.steps.len(), 3);

    // Verify 3 distinct steps
    assert_eq!(plan.steps[0].step, 1);
    assert_eq!(plan.steps[0].action, "initialize_upload");
    assert!(plan.steps[0].endpoint_or_target.contains("richMedia"));
    assert_eq!(plan.steps[0].status, "ready");

    assert_eq!(plan.steps[1].step, 2);
    assert_eq!(plan.steps[1].action, "upload_binary");
    assert_eq!(plan.steps[1].endpoint_or_target, plan.upload_url);
    assert_eq!(plan.steps[1].status, "ready");

    assert_eq!(plan.steps[2].step, 3);
    assert_eq!(plan.steps[2].action, "create_share");
    assert!(plan.steps[2].endpoint_or_target.contains("normShares"));
    assert_eq!(plan.steps[2].status, "ready");
}

#[test]
fn test_cli_post_create_dry_run() -> Result<(), Box<dyn std::error::Error>> {
    let mut tmp_post = NamedTempFile::new()?;
    let markdown_content = r#"---
visibility: CONNECTIONS
---
Excited to announce our new open source release! 🚀

![Screenshot](assets/preview.png)
"#;
    tmp_post.write_all(markdown_content.as_bytes())?;

    let mut cmd = Command::cargo_bin("linkedin")?;
    cmd.args([
        "post",
        "create",
        "--file",
        tmp_post.path().to_str().unwrap(),
        "--dry-run",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("\"dry_run\": true"))
    .stdout(predicates::str::contains("\"resource\": \"post\""))
    .stdout(predicates::str::contains("\"action\": \"create\""))
    .stdout(predicates::str::contains(
        "\"media_file\": \"assets/preview.png\"",
    ))
    .stdout(predicates::str::contains(
        "Excited to announce our new open source release! 🚀",
    ));

    Ok(())
}

// =========================================================================
// 2. Post Analytics & Metrics Ingestion Tests
// =========================================================================

#[test]
fn test_metrics_schema_mapping_and_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let voyager_payload = json!({
        "summary": {
            "impressionCount": 5000,
            "uniqueImpressionsCount": 4200,
            "clickCount": 150,
            "reactionCount": 250,
            "commentCount": 35,
            "repostCount": 15
        },
        "demographics": [
            {
                "category": "Job Title",
                "label": "Software Engineer",
                "percentage": 42.5,
                "count": 2125
            },
            {
                "category": "Company",
                "label": "Acme Systems",
                "percentage": 15.0,
                "count": 750
            },
            {
                "category": "Location",
                "label": "London, UK",
                "percentage": 25.0,
                "count": 1250
            }
        ]
    });

    let urn = "urn:li:activity:7123456789012345678";
    let analytics = parse_voyager_analytics(&voyager_payload, urn)?;

    assert_eq!(analytics.post_urn, urn);
    assert_eq!(analytics.impressions, 5000);
    assert_eq!(analytics.unique_impressions, 4200);
    assert_eq!(analytics.clicks, 150);
    assert_eq!(analytics.reactions, 250);
    assert_eq!(analytics.comments, 35);
    assert_eq!(analytics.reposts, 15);

    // Total engagements = 250 + 35 + 15 + 150 = 450. Rate = (450 / 5000) * 100 = 9.0%
    assert_eq!(analytics.engagement_rate, 9.0);
    // CTR = (150 / 5000) * 100 = 3.0%
    assert_eq!(analytics.click_through_rate, 3.0);

    assert_eq!(analytics.top_demographics.len(), 3);
    assert_eq!(analytics.top_demographics[0].category, "Job Title");
    assert_eq!(analytics.top_demographics[0].label, "Software Engineer");
    assert_eq!(analytics.top_demographics[0].percentage, 42.5);
    assert_eq!(analytics.top_demographics[0].count, Some(2125));

    // Verify round-trip JSON serialization
    let serialized = serde_json::to_string(&analytics)?;
    let deserialized: PostAnalytics = serde_json::from_str(&serialized)?;
    assert_eq!(analytics, deserialized);

    Ok(())
}

#[test]
fn test_metrics_zero_impressions_division_safety() -> Result<(), Box<dyn std::error::Error>> {
    let empty_payload = json!({
        "summary": {
            "impressionCount": 0,
            "reactionCount": 0
        }
    });

    let analytics = parse_voyager_analytics(&empty_payload, "urn:li:activity:0")?;
    assert_eq!(analytics.impressions, 0);
    assert_eq!(analytics.engagement_rate, 0.0);
    assert_eq!(analytics.click_through_rate, 0.0);
    assert!(analytics.top_demographics.is_empty());

    Ok(())
}

#[test]
fn test_cli_post_stats_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").expect("linkedin binary exists");
    cmd.args([
        "post",
        "stats",
        "urn:li:activity:7123456789",
        "--dry-run",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("\"dry_run\": true"))
    .stdout(predicates::str::contains("\"resource\": \"post\""))
    .stdout(predicates::str::contains("\"action\": \"stats\""))
    .stdout(predicates::str::contains("urn:li:activity:7123456789"))
    .stdout(predicates::str::contains("\"impressions\": 1420"));
}
