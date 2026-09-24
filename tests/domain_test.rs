use linkedin::domain::{
    classify_page_url, AuthStatus, ConnectionState, ContactInfo, Date, DateRange, Education,
    InboxResult, InteractionRule, Invitation, Message, PageState, Participant, PeopleSearchResult,
    Position, Profile, ProfileSnippet, SelfProfile, StatusResult,
};
use linkedin::errors::AppError;

#[test]
fn test_profile_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let profile = Profile {
        urn_or_id: "alice-smith".to_string(),
        public_identifier: Some("alice-smith".to_string()),
        urn: Some("urn:li:fsd_profile:12345".to_string()),
        name: "Alice Smith".to_string(),
        first_name: Some("Alice".to_string()),
        last_name: Some("Smith".to_string()),
        headline: Some("Staff Engineer".to_string()),
        summary: Some("Staff engineer with 10+ years of experience".to_string()),
        location: Some("San Francisco, CA".to_string()),
        profile_url: "https://www.linkedin.com/in/alice-smith".to_string(),
        connection_degree: Some(1),
        positions: vec![Position {
            title: "Staff Engineer".to_string(),
            company_name: "Acme Corp".to_string(),
            company_urn: None,
            location: Some("San Francisco".to_string()),
            date_range: Some(DateRange {
                start: Some(Date { year: Some(2021), month: Some(1) }),
                end: None,
            }),
            description: Some("Building distributed systems".to_string()),
        }],
        educations: vec![Education {
            school_name: "Stanford University".to_string(),
            degree_name: Some("B.S.".to_string()),
            field_of_study: Some("Computer Science".to_string()),
            date_range: None,
        }],
    };

    let serialized = serde_json::to_string(&profile)?;
    let deserialized: Profile = serde_json::from_str(&serialized)?;
    assert_eq!(profile, deserialized);
    Ok(())
}

#[test]
fn test_auth_status_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let status = AuthStatus::Authenticated {
        account_name: Some("Alice Smith".to_string()),
        self_profile: Some(SelfProfile {
            public_identifier: Some("alice-smith".to_string()),
            urn: Some("urn:li:fsd_profile:12345".to_string()),
            full_name: Some("Alice Smith".to_string()),
        }),
    };
    let serialized = serde_json::to_string(&status)?;
    let deserialized: AuthStatus = serde_json::from_str(&serialized)?;
    assert_eq!(status, deserialized);
    Ok(())
}

#[test]
fn test_page_state_classification() {
    assert_eq!(classify_page_url("https://www.linkedin.com/feed/"), PageState::Feed);
    assert_eq!(classify_page_url("https://www.linkedin.com/feed/?trk=nav"), PageState::Feed);
    assert_eq!(classify_page_url("https://www.linkedin.com/login?session_redirect=%2Ffeed%2F"), PageState::Login);
    assert_eq!(classify_page_url("https://www.linkedin.com/uas/login"), PageState::Login);
    assert_eq!(classify_page_url("https://www.linkedin.com/checkpoint/challenge/123"), PageState::Checkpoint);
    assert_eq!(classify_page_url("https://www.linkedin.com/authwall?trk=rip"), PageState::Authwall);
    assert_eq!(classify_page_url("https://www.linkedin.com/in/alice-smith/"), PageState::Profile);
    assert_eq!(classify_page_url("https://www.linkedin.com/messaging/thread/123"), PageState::Messaging);
    assert_eq!(classify_page_url("https://www.linkedin.com/random-page"), PageState::Unknown);
}

#[test]
fn test_error_taxonomy_types() {
    let err = AppError::CheckpointChallenge("https://linkedin.com/checkpoint".to_string());
    assert_eq!(err.error_type(), "checkpoint_challenge");

    let err = AppError::Authentication("Invalid creds".to_string());
    assert_eq!(err.error_type(), "authentication");

    let err = AppError::ProfileInaccessible("alice".to_string());
    assert_eq!(err.error_type(), "profile_inaccessible");

    let err = AppError::ConnectionLimit("Limit reached".to_string());
    assert_eq!(err.error_type(), "connection_limit");

    let err = AppError::SkipProfile("Already invited".to_string());
    assert_eq!(err.error_type(), "skip_profile");

    let err = AppError::InvalidInput("Too long".to_string());
    assert_eq!(err.error_type(), "invalid_argument");
}

#[test]
fn test_status_result_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let res = StatusResult {
        public_identifier: "alice-smith".to_string(),
        state: ConnectionState::Connected,
    };
    let serialized = serde_json::to_string(&res)?;
    assert!(serialized.contains("Connected"));
    Ok(())
}

#[test]
fn test_people_search_result_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let result = PeopleSearchResult {
        query: "Rust".to_string(),
        page: 1,
        network: vec!["first".to_string()],
        profiles: vec![ProfileSnippet {
            public_identifier: "alice-smith".to_string(),
            profile_url: "https://www.linkedin.com/in/alice-smith".to_string(),
            name: Some("Alice Smith".to_string()),
            headline: Some("Staff Engineer".to_string()),
            location: Some("San Francisco".to_string()),
        }],
    };
    let json_str = serde_json::to_string(&result)?;
    let deserialized: PeopleSearchResult = serde_json::from_str(&json_str)?;
    assert_eq!(result, deserialized);
    Ok(())
}

#[test]
fn test_contact_info_and_messaging_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let contact = ContactInfo {
        public_identifier: "alice".to_string(),
        email: Some("alice@example.com".to_string()),
        emails: vec!["alice@example.com".to_string()],
        phone_numbers: vec!["+123456789".to_string()],
        websites: vec!["https://alice.dev".to_string()],
        twitter_handles: vec!["@alice".to_string()],
        birthday: Some("April 12".to_string()),
        connected: Some("May 2023".to_string()),
    };
    let json_contact = serde_json::to_string(&contact)?;
    let des_contact: ContactInfo = serde_json::from_str(&json_contact)?;
    assert_eq!(contact, des_contact);

    let invite = Invitation {
        profile_url: "https://linkedin.com/in/alice".to_string(),
        public_identifier: "alice".to_string(),
        name: Some("Alice".to_string()),
        headline: Some("Engineer".to_string()),
        sent_or_received_at: Some("2 days ago".to_string()),
        note: Some("Let's connect!".to_string()),
    };
    let json_invite = serde_json::to_string(&invite)?;
    let des_invite: Invitation = serde_json::from_str(&json_invite)?;
    assert_eq!(invite, des_invite);

    let msg = Message {
        entity_urn: Some("urn:msg:1".to_string()),
        sender_urn: Some("urn:member:1".to_string()),
        sender_name: Some("Alice".to_string()),
        text: Some("Hello!".to_string()),
        attachments: vec![],
        delivered_at: Some(1700000000),
        timestamp: Some("Yesterday".to_string()),
    };
    let participant = Participant {
        urn: Some("urn:member:1".to_string()),
        name: Some("Alice".to_string()),
        headline: Some("Engineer".to_string()),
        profile_url: Some("https://linkedin.com/in/alice".to_string()),
        is_self: false,
    };
    let json_msg = serde_json::to_string(&msg)?;
    let des_msg: Message = serde_json::from_str(&json_msg)?;
    assert_eq!(msg, des_msg);

    let json_part = serde_json::to_string(&participant)?;
    let des_part: Participant = serde_json::from_str(&json_part)?;
    assert_eq!(participant, des_part);

    Ok(())
}

#[test]
fn test_interaction_rule_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let rule = InteractionRule::default();
    assert!(rule.min_delay_ms <= rule.max_delay_ms);
    Ok(())
}

#[test]
fn test_inbox_result_cursor_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let inbox_with_cursor = InboxResult {
        conversations: vec![],
        next_cursor: Some("REVTQ0VORElORyYxNzg4NzkwMjkxNTA4JmNvbnZfMTIz".to_string()),
    };
    let json_with_cursor = serde_json::to_string(&inbox_with_cursor)?;
    assert!(json_with_cursor.contains("next_cursor"));

    let inbox_without_cursor = InboxResult {
        conversations: vec![],
        next_cursor: None,
    };
    let json_without_cursor = serde_json::to_string(&inbox_without_cursor)?;
    assert!(!json_without_cursor.contains("next_cursor"));
    Ok(())
}
