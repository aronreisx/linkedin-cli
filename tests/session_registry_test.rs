use assert_cmd::Command;
use linkedin::browser::{
    clear_session, find_active_session, is_pid_alive, list_sessions, read_session, write_session,
};
use predicates::prelude::*;
use serde_json::Value;

#[test]
fn test_session_registry_lifecycle() {
    let test_name = format!("test_sess_{}", std::process::id());
    let endpoint = "ws://127.0.0.1:9222/devtools/browser/deadbeef";
    let pid = std::process::id();

    // 1. Write session
    let entry = write_session(&test_name, endpoint, pid).expect("write_session should succeed");
    assert_eq!(entry.name, test_name);
    assert_eq!(entry.endpoint, endpoint);
    assert_eq!(entry.pid, pid);

    // 2. Read session
    let read_entry = read_session(&test_name).expect("session entry should exist");
    assert_eq!(read_entry.name, test_name);
    assert_eq!(read_entry.endpoint, endpoint);
    assert_eq!(read_entry.pid, pid);

    // 3. Find active session (current PID is alive)
    let active = find_active_session(&test_name).expect("session should be found active");
    assert_eq!(active.name, test_name);

    // 4. List sessions
    let all = list_sessions().expect("list_sessions should succeed");
    assert!(all.iter().any(|s| s.name == test_name));

    // 5. Clear session
    clear_session(&test_name).expect("clear_session should succeed");

    // Verify gone
    let after_clear = read_session(&test_name);
    assert!(after_clear.is_none());
}

#[test]
fn test_session_pid_liveness_check() {
    let current_pid = std::process::id();
    assert!(is_pid_alive(current_pid), "Current process PID must be alive");

    // PID 99999999 is virtually guaranteed not to be in use
    let non_existent_pid = 99_999_999;
    assert!(
        !is_pid_alive(non_existent_pid),
        "Non-existent PID should not report alive"
    );
}

#[test]
fn test_session_open_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args(["--dry-run", "-o", "json", "session", "open", "--name", "research-bot"])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).expect("valid json output");

    assert_eq!(json["resource"], "session");
    assert_eq!(json["action"], "open");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["parameters"]["name"], "research-bot");
}

#[test]
fn test_session_status_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args(["--dry-run", "-o", "json", "session", "status", "--name", "research-bot"])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).expect("valid json output");

    assert_eq!(json["resource"], "session");
    assert_eq!(json["action"], "status");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["parameters"]["name"], "research-bot");
}

#[test]
fn test_session_close_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args(["--dry-run", "-o", "json", "session", "close", "--name", "research-bot"])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).expect("valid json output");

    assert_eq!(json["resource"], "session");
    assert_eq!(json["action"], "close");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["parameters"]["name"], "research-bot");
}

#[test]
fn test_session_list_dry_run() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args(["--dry-run", "-o", "json", "session", "list"])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).expect("valid json output");

    assert_eq!(json["resource"], "session");
    assert_eq!(json["action"], "list");
    assert_eq!(json["dry_run"], true);
}

#[test]
fn test_cli_help_includes_session_command() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("session"));

    let mut sub_cmd = Command::cargo_bin("linkedin").unwrap();
    sub_cmd
        .args(["session", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("open"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("close"))
        .stdout(predicate::str::contains("list"));
}
