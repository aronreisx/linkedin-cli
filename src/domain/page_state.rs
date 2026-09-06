use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PageState {
    Checkpoint,
    Login,
    Authwall,
    Feed,
    Profile,
    Messaging,
    NotFound,
    Unknown,
}

impl PageState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Checkpoint => "checkpoint",
            Self::Login => "login",
            Self::Authwall => "authwall",
            Self::Feed => "feed",
            Self::Profile => "profile",
            Self::Messaging => "messaging",
            Self::NotFound => "not_found",
            Self::Unknown => "unknown",
        }
    }
}

/// Classify the LinkedIn page by URL *path* only, avoiding query parameter traps.
pub fn classify_page_url(raw_url: &str) -> PageState {
    let path = if let Ok(parsed) = Url::parse(raw_url) {
        parsed.path().to_string()
    } else {
        // Fallback for relative paths or malformed URLs
        raw_url.split('?').next().unwrap_or(raw_url).to_string()
    };

    let routes: &[(&str, PageState)] = &[
        ("/checkpoint", PageState::Checkpoint),
        ("/login", PageState::Login),
        ("/uas/login", PageState::Login),
        ("/authwall", PageState::Authwall),
        ("/feed", PageState::Feed),
        ("/in/", PageState::Profile),
        ("/messaging", PageState::Messaging),
        ("/404", PageState::NotFound),
    ];

    for (prefix, state) in routes {
        if path.starts_with(prefix) {
            return *state;
        }
    }

    PageState::Unknown
}
