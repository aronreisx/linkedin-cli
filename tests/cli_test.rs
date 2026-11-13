use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

// =========================================================================
// 1. CLI Interface Tests
// =========================================================================

#[test]
fn test_cli_help_displays_resource_verb_groups() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("profile"))
        .stdout(predicate::str::contains("connection"))
        .stdout(predicate::str::contains("message"))
        .stdout(predicate::str::contains("inbox"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("invite"))
        .stdout(predicate::str::contains("debug"))
        .stdout(predicate::str::contains("completions"))
        .stdout(predicate::str::contains("manpage"));
}

#[test]
fn test_resource_subcommand_help_menus() {
    // Auth subcommands
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["auth", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("login"))
        .stdout(predicate::str::contains("whoami"));

    // Profile subcommands
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["profile", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("contact"))
        .stdout(predicate::str::contains("status"));

    // Connection subcommands
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["connection", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("request"));

    // Message subcommands
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["message", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("send"))
        .stdout(predicate::str::contains("thread"));

    // Invite subcommands
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["invite", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("accept"))
        .stdout(predicate::str::contains("withdraw"))
        .stdout(predicate::str::contains("ignore"));
}

#[test]
fn test_required_argument_validation() {
    // Missing handle on `profile get` -> exit code 2
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["profile", "get"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("<HANDLE>"));

    // Missing --text on `message send` -> exit code 2
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["message", "send", "alice"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--text"));

    // Missing keywords on `search people` -> exit code 2
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["search", "people"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("<KEYWORDS>"));

    // Missing --shell on `completions` -> exit code 2
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["completions"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--shell"));

    // Missing --out-dir on `manpage` -> exit code 2
    Command::cargo_bin("linkedin")
        .unwrap()
        .args(["manpage"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--out-dir"));
}

// =========================================================================
// 2. Output & Dry-Run Tests
// =========================================================================

#[test]
fn test_dry_run_zero_side_effects_connection_request() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "connection",
            "request",
            "alice-test",
            "--note",
            "Hi Alice, let's connect!",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["resource"], "connection");
    assert_eq!(val["action"], "request");
    assert_eq!(val["target"], "alice-test");
    assert_eq!(val["dry_run"], true);
    assert_eq!(val["parameters"]["note"], "Hi Alice, let's connect!");
}

#[test]
fn test_dry_run_zero_side_effects_message_send() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "message",
            "send",
            "bob-recruiter",
            "--text",
            "Thanks for reaching out.",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["resource"], "message");
    assert_eq!(val["action"], "send");
    assert_eq!(val["target"], "bob-recruiter");
    assert_eq!(val["dry_run"], true);
    assert_eq!(val["parameters"]["text"], "Thanks for reaching out.");
}

#[test]
fn test_dry_run_message_send_with_attachment() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args([
            "message",
            "send",
            "john-doe-sample",
            "--attachment",
            "assets/test_attachment.png",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stdout_bytes = assert.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
    assert_eq!(val["resource"], "message");
    assert_eq!(val["action"], "send");
    assert_eq!(val["target"], "john-doe-sample");
    assert_eq!(val["dry_run"], true);
    assert_eq!(
        val["parameters"]["attachment"],
        "assets/test_attachment.png"
    );
    assert_eq!(
        val["parameters"]["action"],
        "send_direct_message_with_attachment"
    );
}

#[test]
fn test_dry_run_invite_mutations() {
    for (action_name, verb) in [
        ("accept", "accept"),
        ("withdraw", "withdraw"),
        ("ignore", "ignore"),
    ] {
        let mut cmd = Command::cargo_bin("linkedin").unwrap();
        let assert = cmd
            .args(["invite", verb, "charlie-user", "--dry-run", "-o", "json"])
            .assert()
            .success();

        let stdout_bytes = assert.get_output().stdout.as_slice();
        let val: Value = serde_json::from_slice(stdout_bytes).expect("Valid JSON");
        assert_eq!(val["resource"], "invite");
        assert_eq!(val["action"], action_name);
        assert_eq!(val["target"], "charlie-user");
        assert_eq!(val["dry_run"], true);
    }
}

#[test]
fn test_output_formats_ndjson_tsv_table_json() {
    // 1. JSON
    let mut cmd_json = Command::cargo_bin("linkedin").unwrap();
    let assert_json = cmd_json
        .args(["connection", "request", "alice", "--dry-run", "-o", "json"])
        .assert()
        .success();
    let _: Value = serde_json::from_slice(assert_json.get_output().stdout.as_slice())
        .expect("Must parse as JSON");

    // 2. NDJSON
    let mut cmd_ndjson = Command::cargo_bin("linkedin").unwrap();
    let assert_ndjson = cmd_ndjson
        .args([
            "connection",
            "request",
            "alice",
            "--dry-run",
            "-o",
            "ndjson",
        ])
        .assert()
        .success();
    let stdout_ndjson = String::from_utf8(assert_ndjson.get_output().stdout.clone()).unwrap();
    for line in stdout_ndjson.lines().filter(|l| !l.trim().is_empty()) {
        let _: Value = serde_json::from_str(line).expect("Each NDJSON line must be valid JSON");
    }

    // 3. TSV
    let mut cmd_tsv = Command::cargo_bin("linkedin").unwrap();
    let assert_tsv = cmd_tsv
        .args(["connection", "request", "alice", "--dry-run", "-o", "tsv"])
        .assert()
        .success();
    let stdout_tsv = String::from_utf8(assert_tsv.get_output().stdout.clone()).unwrap();
    assert!(
        stdout_tsv.contains('\t'),
        "TSV output must contain tab delimiters"
    );

    // 4. Table / Summary
    let mut cmd_table = Command::cargo_bin("linkedin").unwrap();
    cmd_table
        .args(["connection", "request", "alice", "--dry-run", "-o", "table"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[DRY-RUN]"));

    // 5. Backward-compatible --json flag
    let mut cmd_legacy_json = Command::cargo_bin("linkedin").unwrap();
    let assert_legacy = cmd_legacy_json
        .args(["connection", "request", "alice", "--dry-run", "--json"])
        .assert()
        .success();
    let _: Value = serde_json::from_slice(assert_legacy.get_output().stdout.as_slice())
        .expect("Must parse as JSON");
}

// =========================================================================
// 3. Unix I/O Tests
// =========================================================================

#[test]
fn test_unix_io_stdout_purity_and_quiet_flag() {
    // When -q is enabled, stderr must be completely empty
    let mut cmd_quiet = Command::cargo_bin("linkedin").unwrap();
    let assert_quiet = cmd_quiet
        .args([
            "-q",
            "connection",
            "request",
            "alice",
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success();

    let stderr = assert_quiet.get_output().stderr.as_slice();
    assert!(
        stderr.is_empty(),
        "stderr must be empty when -q / --quiet is specified, but found: {}",
        String::from_utf8_lossy(stderr)
    );

    // stdout must be valid parseable data with zero log contamination
    let stdout = assert_quiet.get_output().stdout.as_slice();
    let val: Value = serde_json::from_slice(stdout).expect("stdout must be clean parseable JSON");
    assert_eq!(val["resource"], "connection");
}

#[test]
fn test_shell_completions_syntax() {
    // Bash completion
    let mut cmd_bash = Command::cargo_bin("linkedin").unwrap();
    cmd_bash
        .args(["completions", "--shell", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_linkedin"));

    // Zsh completion
    let mut cmd_zsh = Command::cargo_bin("linkedin").unwrap();
    cmd_zsh
        .args(["completions", "--shell", "zsh"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("#compdef linkedin").or(predicate::str::contains("_linkedin")),
        );

    // Fish completion
    let mut cmd_fish = Command::cargo_bin("linkedin").unwrap();
    cmd_fish
        .args(["completions", "--shell", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -c linkedin"));
}

#[test]
fn test_manpage_generation_creates_valid_troff() {
    let dir = tempdir().expect("Failed to create tempdir");
    let out_path = dir.path().to_path_buf();

    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.args(["manpage", "--out-dir", out_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Manpage generated at:"));

    let man_file = out_path.join("linkedin.1");
    assert!(man_file.exists(), "linkedin.1 must exist");

    let content = std::fs::read_to_string(&man_file).expect("Must read manpage");
    assert!(
        content.contains(".TH linkedin"),
        "Manpage must contain standard troff header .TH linkedin"
    );
    assert!(
        content.contains("linkedin"),
        "Manpage must document binary name"
    );
}

// =========================================================================
// 4. Composability, Keyring & Sandbox Integration Tests
// =========================================================================

#[test]
fn test_ndjson_streaming_and_sigpipe_resilience() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command as StdCommand, Stdio};

    // 1. Verify line-by-line valid JSON emission
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    let assert = cmd
        .args(["search", "people", "Rust", "--dry-run", "-o", "ndjson"])
        .assert()
        .success();

    let stdout_str = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        lines.len() >= 3,
        "Expected at least 3 streamed snippets, got {}",
        lines.len()
    );

    for line in lines {
        let val: Value =
            serde_json::from_str(line).expect("Each line must be standalone valid JSON");
        assert!(val.get("public_identifier").is_some());
    }

    // 2. SIGPIPE Resilience Test: Child process piped to reader that closes early
    let bin_path = assert_cmd::cargo::cargo_bin("linkedin");
    let mut child = StdCommand::new(bin_path)
        .args(["search", "people", "Rust", "--dry-run", "-o", "ndjson"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("Spawn linkedin process");

    let stdout = child.stdout.take().expect("Child stdout");
    let mut reader = BufReader::new(stdout);
    let mut first_line = String::new();
    let _ = reader.read_line(&mut first_line);

    // Drop the reader to close the pipe immediately (simulating `head -n 1`)
    drop(reader);

    // Wait for child to exit; must exit with code 0 (or standard Unix SIGPIPE exit), never panic
    let status = child.wait().expect("Wait for child");
    assert!(
        status.success() || status.code() == Some(0) || status.code() == Some(141),
        "Process must exit gracefully on SIGPIPE, got status: {:?}",
        status
    );
}

#[test]
fn test_keyring_storage_add_list_switch_remove_in_mock_backend() {
    let temp_home = tempdir().expect("Create temp home");
    let home_path = temp_home.path().to_str().unwrap();

    // 1. Add profile 'work'
    let mut cmd_add = Command::cargo_bin("linkedin").unwrap();
    cmd_add
        .env("LINKEDIN_CLI_HOME", home_path)
        .env("LINKEDIN_CLI_KEYRING_BACKEND", "file")
        .args([
            "auth",
            "add",
            "work",
            "--username",
            "alice@work.com",
            "--password",
            "supersecret123",
            "--li-at",
            "token_cookie_work",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("work"))
        .stdout(predicate::str::contains("saved"));

    // 2. Add profile 'personal'
    let mut cmd_add2 = Command::cargo_bin("linkedin").unwrap();
    cmd_add2
        .env("LINKEDIN_CLI_HOME", home_path)
        .env("LINKEDIN_CLI_KEYRING_BACKEND", "file")
        .args([
            "auth",
            "add",
            "personal",
            "--username",
            "alice@personal.org",
            "--li-at",
            "token_cookie_personal",
        ])
        .assert()
        .success();

    // 3. List profiles
    let mut cmd_list = Command::cargo_bin("linkedin").unwrap();
    let assert_list = cmd_list
        .env("LINKEDIN_CLI_HOME", home_path)
        .env("LINKEDIN_CLI_KEYRING_BACKEND", "file")
        .args(["auth", "list", "-o", "json"])
        .assert()
        .success();

    let list_val: Value = serde_json::from_slice(&assert_list.get_output().stdout).unwrap();
    let profiles_arr = list_val.as_array().expect("List must be JSON array");
    assert_eq!(profiles_arr.len(), 3); // "default", "work", "personal"
    assert!(profiles_arr.iter().any(|p| p["name"] == "work"));
    assert!(profiles_arr.iter().any(|p| p["name"] == "personal"));

    // 4. Switch active profile to 'personal'
    let mut cmd_switch = Command::cargo_bin("linkedin").unwrap();
    cmd_switch
        .env("LINKEDIN_CLI_HOME", home_path)
        .env("LINKEDIN_CLI_KEYRING_BACKEND", "file")
        .args(["auth", "switch", "personal"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Switched active profile to 'personal'",
        ));

    // 5. Verify active profile is 'personal'
    let mut cmd_verify_list = Command::cargo_bin("linkedin").unwrap();
    let assert_verify = cmd_verify_list
        .env("LINKEDIN_CLI_HOME", home_path)
        .env("LINKEDIN_CLI_KEYRING_BACKEND", "file")
        .args(["auth", "list", "-o", "json"])
        .assert()
        .success();

    let verify_val: Value = serde_json::from_slice(&assert_verify.get_output().stdout).unwrap();
    let personal_profile = verify_val
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "personal")
        .expect("Personal profile must exist");
    assert_eq!(personal_profile["is_active"], true);

    let work_profile = verify_val
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "work")
        .expect("Work profile must exist");
    assert_eq!(work_profile["is_active"], false);

    // 6. Remove profile 'work'
    let mut cmd_rm = Command::cargo_bin("linkedin").unwrap();
    cmd_rm
        .env("LINKEDIN_CLI_HOME", home_path)
        .env("LINKEDIN_CLI_KEYRING_BACKEND", "file")
        .args(["auth", "remove", "work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed profile 'work'"));
}

#[test]
fn test_profile_sandbox_directory_isolation() {
    use linkedin::browser::BrowserConfig;
    use linkedin::infra::resolve_credential_store;

    let temp_home = tempdir().expect("Create temp home");
    let home_path = temp_home.path().to_path_buf();
    std::env::set_var("LINKEDIN_CLI_HOME", &home_path);
    std::env::set_var("LINKEDIN_CLI_KEYRING_BACKEND", "file");

    // 1. Check isolation of explicit session configs
    let config_work = BrowserConfig {
        session_name: "work".to_string(),
        ..Default::default()
    };
    let config_personal = BrowserConfig {
        session_name: "personal".to_string(),
        ..Default::default()
    };

    let dir_work = config_work.resolve_user_data_dir();
    let dir_personal = config_personal.resolve_user_data_dir();

    assert_ne!(
        dir_work, dir_personal,
        "Session directories must be completely distinct"
    );
    assert!(dir_work.ends_with("profiles/work"));
    assert!(dir_personal.ends_with("profiles/personal"));

    // 2. Verify filesystem sandbox isolation (cookies/cache cannot leak)
    let work_cookies = dir_work.join("Default").join("Network");
    let personal_cookies = dir_personal.join("Default").join("Network");

    std::fs::create_dir_all(&work_cookies).unwrap();
    std::fs::create_dir_all(&personal_cookies).unwrap();

    let work_cookie_file = work_cookies.join("Cookies");
    let personal_cookie_file = personal_cookies.join("Cookies");

    std::fs::write(&work_cookie_file, "session=work_token; secure; HttpOnly").unwrap();
    std::fs::write(
        &personal_cookie_file,
        "session=personal_token; secure; HttpOnly",
    )
    .unwrap();

    let read_work = std::fs::read_to_string(&work_cookie_file).unwrap();
    let read_personal = std::fs::read_to_string(&personal_cookie_file).unwrap();

    assert_eq!(read_work, "session=work_token; secure; HttpOnly");
    assert_eq!(read_personal, "session=personal_token; secure; HttpOnly");
    assert_ne!(read_work, read_personal);

    // 3. Verify active profile switching dynamically redirects default session
    let store = resolve_credential_store();
    store.set_active_profile("work").unwrap();

    let config_default = BrowserConfig {
        session_name: "default".to_string(),
        ..Default::default()
    };
    let resolved_active = config_default.resolve_user_data_dir();
    assert!(
        resolved_active.ends_with("profiles/work"),
        "Default session should resolve to active profile 'work'"
    );

    store.set_active_profile("personal").unwrap();
    let resolved_switched = config_default.resolve_user_data_dir();
    assert!(
        resolved_switched.ends_with("profiles/personal"),
        "Default session should resolve to switched profile 'personal'"
    );
}
