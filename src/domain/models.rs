use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Date {
    pub year: Option<i32>,
    pub month: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DateRange {
    pub start: Option<Date>,
    pub end: Option<Date>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub title: String,
    pub company_name: String,
    pub company_urn: Option<String>,
    pub location: Option<String>,
    pub date_range: Option<DateRange>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Education {
    pub school_name: String,
    pub degree_name: Option<String>,
    pub field_of_study: Option<String>,
    pub date_range: Option<DateRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub urn_or_id: String,
    pub public_identifier: Option<String>,
    pub urn: Option<String>,
    pub name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub headline: Option<String>,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub profile_url: String,
    pub connection_degree: Option<u8>,
    #[serde(default)]
    pub positions: Vec<Position>,
    #[serde(default)]
    pub educations: Vec<Education>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfProfile {
    pub public_identifier: Option<String>,
    pub urn: Option<String>,
    pub full_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthStatus {
    Authenticated {
        account_name: Option<String>,
        self_profile: Option<SelfProfile>,
    },
    Unauthenticated,
    ChallengeCheckpoint {
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactInfo {
    pub public_identifier: String,
    pub email: Option<String>,
    #[serde(default)]
    pub emails: Vec<String>,
    #[serde(default)]
    pub phone_numbers: Vec<String>,
    #[serde(default)]
    pub websites: Vec<String>,
    #[serde(default)]
    pub twitter_handles: Vec<String>,
    #[serde(default)]
    pub birthday: Option<String>,
    #[serde(default)]
    pub connected: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Pending,
    Qualified,
    OutOfNetwork,
}

impl ConnectionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Pending => "Pending",
            Self::Qualified => "Qualified",
            Self::OutOfNetwork => "OutOfNetwork",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusResult {
    pub public_identifier: String,
    pub state: ConnectionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Invitation {
    pub profile_url: String,
    pub public_identifier: String,
    pub name: Option<String>,
    pub headline: Option<String>,
    pub sent_or_received_at: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageAttachment {
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub entity_urn: Option<String>,
    pub sender_urn: Option<String>,
    pub sender_name: Option<String>,
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
    pub delivered_at: Option<i64>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Participant {
    pub urn: Option<String>,
    pub name: Option<String>,
    pub headline: Option<String>,
    pub profile_url: Option<String>,
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Conversation {
    pub entity_urn: String,
    pub conversation_url: Option<String>,
    pub read: Option<bool>,
    pub unread_count: Option<u32>,
    pub participants: Vec<Participant>,
    pub latest_message: Option<Message>,
    pub last_activity_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct InboxFilter {
    pub all: bool,
    pub cursor: Option<String>,
    pub unread: bool,
    pub read: bool,
    pub participant: Option<String>,
    pub sender: Option<String>,
    pub contains: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxResult {
    pub conversations: Vec<Conversation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSnippet {
    pub public_identifier: String,
    pub profile_url: String,
    pub name: Option<String>,
    pub headline: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeopleSearchResult {
    pub query: String,
    pub page: u32,
    pub network: Vec<String>,
    pub profiles: Vec<ProfileSnippet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRule {
    pub min_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for InteractionRule {
    fn default() -> Self {
        Self {
            min_delay_ms: 350,
            max_delay_ms: 1200,
        }
    }
}
