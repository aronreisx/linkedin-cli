use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Browser lifecycle error: {0}")]
    Browser(String),

    #[error("Navigation failed for '{url}': {reason}")]
    NavigationFailed { url: String, reason: String },

    #[error("DOM element not found: {selector}")]
    ElementNotFound { selector: String },

    #[error("Authentication required: {0}")]
    Authentication(String),

    #[error("Security checkpoint / verification challenge: {0}")]
    CheckpointChallenge(String),

    #[error("Profile inaccessible: {0}")]
    ProfileInaccessible(String),

    #[error("Reached weekly or daily connection limit: {0}")]
    ConnectionLimit(String),

    #[error("Skipping profile: {0}")]
    SkipProfile(String),

    #[error("Invalid input argument: {0}")]
    InvalidInput(String),

    #[error("Extraction failed for target '{target}': {reason}")]
    ExtractionFailed { target: String, reason: String },

    #[error("CDP command error: {0}")]
    Cdp(String),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Credential or keyring error: {0}")]
    Credential(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Resource not found: {0}")]
    NotFound(String),
}

impl AppError {
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::CheckpointChallenge(_) => "checkpoint_challenge",
            Self::Authentication(_) => "authentication",
            Self::Credential(_) => "credential_error",
            Self::Database(_) => "database_error",
            Self::NotFound(_) => "not_found",
            Self::ProfileInaccessible(_) => "profile_inaccessible",
            Self::ConnectionLimit(_) => "connection_limit",
            Self::SkipProfile(_) => "skip_profile",
            Self::InvalidInput(_) => "invalid_argument",
            Self::ElementNotFound { .. } => "element_not_found",
            Self::NavigationFailed { .. } => "navigation_failed",
            Self::ExtractionFailed { .. } => "extraction_failed",
            Self::Cdp(_) => "cdp_error",
            Self::Browser(_) => "browser_error",
            Self::Json(_) => "json_error",
            Self::Url(_) => "url_parse_error",
            Self::Io(_) => "io_error",
        }
    }
}
