use assert_cmd::Command;
use std::str::FromStr;

use linkedin::domain::jobs::{DatePosted, ExperienceLevel, JobSearchFilter, WorkplaceType};
use linkedin::infra::ChromiumJobService;

#[test]
fn test_workplace_type_parsing() {
    assert_eq!(
        WorkplaceType::from_str("onsite").unwrap(),
        WorkplaceType::OnSite
    );
    assert_eq!(
        WorkplaceType::from_str("on-site").unwrap(),
        WorkplaceType::OnSite
    );
    assert_eq!(
        WorkplaceType::from_str("remote").unwrap(),
        WorkplaceType::Remote
    );
    assert_eq!(
        WorkplaceType::from_str("hybrid").unwrap(),
        WorkplaceType::Hybrid
    );
    assert!(WorkplaceType::from_str("invalid").is_err());

    assert_eq!(WorkplaceType::OnSite.linkedin_code(), "1");
    assert_eq!(WorkplaceType::Remote.linkedin_code(), "2");
    assert_eq!(WorkplaceType::Hybrid.linkedin_code(), "3");
}

#[test]
fn test_date_posted_parsing() {
    assert_eq!(
        DatePosted::from_str("past-24h").unwrap(),
        DatePosted::Past24h
    );
    assert_eq!(DatePosted::from_str("24h").unwrap(), DatePosted::Past24h);
    assert_eq!(
        DatePosted::from_str("past-week").unwrap(),
        DatePosted::PastWeek
    );
    assert_eq!(DatePosted::from_str("week").unwrap(), DatePosted::PastWeek);
    assert_eq!(
        DatePosted::from_str("past-month").unwrap(),
        DatePosted::PastMonth
    );
    assert_eq!(DatePosted::from_str("any").unwrap(), DatePosted::Any);
    assert!(DatePosted::from_str("past-year").is_err());

    assert_eq!(DatePosted::Past24h.linkedin_code(), Some("r86400"));
    assert_eq!(DatePosted::PastWeek.linkedin_code(), Some("r604800"));
    assert_eq!(DatePosted::PastMonth.linkedin_code(), Some("r2592000"));
    assert_eq!(DatePosted::Any.linkedin_code(), None);
}

#[test]
fn test_experience_level_parsing() {
    assert_eq!(
        ExperienceLevel::from_str("internship").unwrap(),
        ExperienceLevel::Internship
    );
    assert_eq!(
        ExperienceLevel::from_str("entry").unwrap(),
        ExperienceLevel::Entry
    );
    assert_eq!(
        ExperienceLevel::from_str("associate").unwrap(),
        ExperienceLevel::Associate
    );
    assert_eq!(
        ExperienceLevel::from_str("mid-senior").unwrap(),
        ExperienceLevel::MidSenior
    );
    assert_eq!(
        ExperienceLevel::from_str("director").unwrap(),
        ExperienceLevel::Director
    );
    assert_eq!(
        ExperienceLevel::from_str("executive").unwrap(),
        ExperienceLevel::Executive
    );
    assert!(ExperienceLevel::from_str("founder").is_err());

    assert_eq!(ExperienceLevel::MidSenior.linkedin_code(), "4");
    assert_eq!(ExperienceLevel::Director.linkedin_code(), "5");
}

#[test]
fn test_job_search_url_builder() {
    // 1. Default empty
    let empty_filter = JobSearchFilter::default();
    assert_eq!(
        empty_filter.build_url(),
        "https://www.linkedin.com/jobs/search/"
    );

    // 2. Keywords and location
    let filter = JobSearchFilter {
        keywords: Some("Rust Engineer".to_string()),
        location: Some("San Francisco, CA".to_string()),
        ..Default::default()
    };
    let url = filter.build_url();
    assert!(url.contains("keywords=Rust+Engineer"));
    assert!(url.contains("location=San+Francisco%2C+CA"));

    // 3. Faceted workplace, date posted, experience, and easy apply
    let full_filter = JobSearchFilter {
        keywords: Some("Staff AI".to_string()),
        location: Some("Remote".to_string()),
        workplace: vec![WorkplaceType::Remote, WorkplaceType::Hybrid],
        date_posted: Some(DatePosted::PastWeek),
        experience: vec![ExperienceLevel::MidSenior, ExperienceLevel::Director],
        easy_apply: true,
        limit: 25,
        page: 2,
    };
    let full_url = full_filter.build_url();
    assert!(full_url.contains("keywords=Staff+AI"));
    assert!(full_url.contains("f_WT=2%2C3"));
    assert!(full_url.contains("f_TPR=r604800"));
    assert!(full_url.contains("f_E=4%2C5"));
    assert!(full_url.contains("f_AL=true"));
    assert!(full_url.contains("start=25"));
}

#[test]
fn test_normalize_job_id() {
    assert_eq!(
        ChromiumJobService::normalize_job_id("4123456789"),
        "4123456789"
    );
    assert_eq!(
        ChromiumJobService::normalize_job_id("urn:li:jobPosting:4123456789"),
        "4123456789"
    );
    assert_eq!(
        ChromiumJobService::normalize_job_id("https://www.linkedin.com/jobs/view/4123456789/"),
        "4123456789"
    );
    assert_eq!(
        ChromiumJobService::normalize_job_id(
            "https://www.linkedin.com/jobs/view/4123456789?refId=abc&trackingId=xyz"
        ),
        "4123456789"
    );
    assert_eq!(
        ChromiumJobService::normalize_job_id(
            "https://www.linkedin.com/jobs/search/?currentJobId=4123456789&keywords=Rust"
        ),
        "4123456789"
    );
}

#[test]
fn test_cli_help_displays_jobs_subcommand() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("jobs"))
        .stdout(predicates::str::contains(
            "Job search, details extraction, and saved jobs management",
        ));

    let mut jobs_cmd = Command::cargo_bin("linkedin").unwrap();
    jobs_cmd
        .args(["jobs", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("search"))
        .stdout(predicates::str::contains("get"))
        .stdout(predicates::str::contains("save"))
        .stdout(predicates::str::contains("unsave"))
        .stdout(predicates::str::contains("saved"));
}

#[test]
fn test_cli_dry_run_jobs_search() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.args([
        "jobs",
        "search",
        "Senior Rust Engineer",
        "--location",
        "Remote",
        "--workplace",
        "remote,hybrid",
        "--posted",
        "past-week",
        "--experience",
        "mid-senior",
        "--easy-apply",
        "--limit",
        "5",
        "--dry-run",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("4100000001"))
    .stdout(predicates::str::contains("Staff AI Systems Engineer"))
    .stdout(predicates::str::contains("Anthropic"));
}

#[test]
fn test_cli_dry_run_jobs_get() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.args(["jobs", "get", "4123456789", "--dry-run", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("4123456789"))
        .stdout(predicates::str::contains("Senior AI Systems Engineer"))
        .stdout(predicates::str::contains("Sarah Talent"))
        .stdout(predicates::str::contains("Rust"));
}

#[test]
fn test_cli_dry_run_jobs_save() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.args(["jobs", "save", "4123456789", "--dry-run", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"resource\": \"jobs\""))
        .stdout(predicates::str::contains("\"action\": \"save\""))
        .stdout(predicates::str::contains("\"job_id\": \"4123456789\""))
        .stdout(predicates::str::contains("\"dry_run\": true"));
}

#[test]
fn test_cli_dry_run_jobs_unsave() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.args(["jobs", "unsave", "4123456789", "--dry-run", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"resource\": \"jobs\""))
        .stdout(predicates::str::contains("\"action\": \"unsave\""))
        .stdout(predicates::str::contains("\"job_id\": \"4123456789\""))
        .stdout(predicates::str::contains("\"dry_run\": true"));
}

#[test]
fn test_cli_dry_run_jobs_saved_list() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.args(["jobs", "saved", "--limit", "10", "--dry-run", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("4100000001"))
        .stdout(predicates::str::contains("Staff AI Systems Engineer"));
}

#[test]
fn test_cli_dry_run_jobs_ndjson_streaming() {
    let mut cmd = Command::cargo_bin("linkedin").unwrap();
    cmd.args(["jobs", "search", "Rust", "-o", "ndjson", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"id\":\"4100000001\""))
        .stdout(predicates::str::contains("\"company_name\":\"Anthropic\""));
}
