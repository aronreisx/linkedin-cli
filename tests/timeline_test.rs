use assert_cmd::Command;
use serde_json::Value;
use std::str::FromStr;

use linkedin::domain::timeline::{validate_comment_content, FeedSort, ReactionType};
use linkedin::infra::ChromiumTimelineService;

#[test]
fn test_reaction_type_parsing_and_display() {
    assert_eq!(ReactionType::from_str("like").unwrap(), ReactionType::Like);
    assert_eq!(
        ReactionType::from_str("celebrate").unwrap(),
        ReactionType::Celebrate
    );
    assert_eq!(
        ReactionType::from_str("applause").unwrap(),
        ReactionType::Celebrate
    );
    assert_eq!(
        ReactionType::from_str("support").unwrap(),
        ReactionType::Support
    );
    assert_eq!(ReactionType::from_str("love").unwrap(), ReactionType::Love);
    assert_eq!(ReactionType::from_str("heart").unwrap(), ReactionType::Love);
    assert_eq!(
        ReactionType::from_str("insightful").unwrap(),
        ReactionType::Insightful
    );
    assert_eq!(
        ReactionType::from_str("funny").unwrap(),
        ReactionType::Funny
    );

    assert!(ReactionType::from_str("invalid_reaction").is_err());
    assert_eq!(ReactionType::Like.as_str(), "like");
    assert_eq!(ReactionType::Celebrate.as_str(), "celebrate");
}

#[test]
fn test_feed_sort_parsing() {
    assert_eq!(FeedSort::from_str("recent").unwrap(), FeedSort::Recent);
    assert_eq!(FeedSort::from_str("latest").unwrap(), FeedSort::Recent);
    assert_eq!(FeedSort::from_str("top").unwrap(), FeedSort::Top);
    assert_eq!(FeedSort::from_str("algorithmic").unwrap(), FeedSort::Top);
    assert!(FeedSort::from_str("random_sort").is_err());
}

#[test]
fn test_comment_validation_boundaries() {
    assert!(validate_comment_content("").is_err());
    assert!(validate_comment_content("   ").is_err());
    assert!(validate_comment_content("Valid comment!").is_ok());

    let exact_limit = "a".repeat(1250);
    assert!(validate_comment_content(&exact_limit).is_ok());

    let over_limit = "a".repeat(1251);
    assert!(validate_comment_content(&over_limit).is_err());
}

#[test]
fn test_activity_urn_normalization() {
    assert_eq!(
        ChromiumTimelineService::normalize_activity_urn("urn:li:activity:1234567890"),
        "urn:li:activity:1234567890"
    );
    assert_eq!(
        ChromiumTimelineService::normalize_activity_urn("1234567890"),
        "urn:li:activity:1234567890"
    );
    assert_eq!(
        ChromiumTimelineService::normalize_activity_urn(
            "https://www.linkedin.com/feed/update/urn:li:activity:9876543210/"
        ),
        "urn:li:activity:9876543210"
    );
}

#[test]
fn test_cli_dry_run_feed_list() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args(["feed", "list", "--limit", "2", "--dry-run", "-o", "json"])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert!(val["posts"].is_array());
    assert_eq!(val["posts"].as_array().unwrap().len(), 2);
    assert_eq!(val["sort"], "recent");
}

#[test]
fn test_cli_dry_run_feed_member() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "feed",
            "member",
            "tech-lead-sample",
            "--limit",
            "1",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert!(val["posts"].is_array());
    assert_eq!(val["posts"].as_array().unwrap().len(), 1);
    assert_eq!(val["posts"][0]["author_name"], "tech-lead-sample");
}

#[test]
fn test_cli_dry_run_feed_get() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "feed",
            "get",
            "urn:li:activity:7100000000000000001",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["urn"], "urn:li:activity:7100000000000000001");
}

#[test]
fn test_cli_dry_run_feed_comments() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "feed",
            "comments",
            "urn:li:activity:7100000000000000001",
            "--limit",
            "1",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert!(val.is_array());
    assert_eq!(val.as_array().unwrap().len(), 1);
}

#[test]
fn test_cli_dry_run_feed_react() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "feed",
            "react",
            "urn:li:activity:7100000000000000001",
            "--type",
            "celebrate",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["resource"], "feed");
    assert_eq!(val["action"], "react");
    assert_eq!(val["target"], "urn:li:activity:7100000000000000001");
    assert_eq!(val["parameters"]["reaction"], "celebrate");
    assert_eq!(val["dry_run"], true);
}

#[test]
fn test_cli_dry_run_feed_unreact() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "feed",
            "unreact",
            "urn:li:activity:7100000000000000001",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["resource"], "feed");
    assert_eq!(val["action"], "unreact");
    assert_eq!(val["dry_run"], true);
}

#[test]
fn test_cli_dry_run_feed_comment() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "feed",
            "comment",
            "urn:li:activity:7100000000000000001",
            "--text",
            "Super insightful, thanks!",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["resource"], "feed");
    assert_eq!(val["action"], "comment");
    assert_eq!(val["parameters"]["text"], "Super insightful, thanks!");
    assert_eq!(val["dry_run"], true);
}

#[test]
fn test_cli_dry_run_feed_repost() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "feed",
            "repost",
            "urn:li:activity:7100000000000000001",
            "--note",
            "Worth reading!",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["resource"], "feed");
    assert_eq!(val["action"], "repost");
    assert_eq!(val["parameters"]["commentary"], "Worth reading!");
    assert_eq!(val["dry_run"], true);
}

#[test]
fn test_cli_help_displays_feed_subcommand() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd.arg("feed").arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("list"));
    assert!(stdout.contains("member"));
    assert!(stdout.contains("get"));
    assert!(stdout.contains("comments"));
    assert!(stdout.contains("react"));
    assert!(stdout.contains("unreact"));
    assert!(stdout.contains("comment"));
    assert!(stdout.contains("repost"));
}
