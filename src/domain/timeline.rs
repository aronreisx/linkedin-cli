use crate::errors::AppError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub const LINKEDIN_COMMENT_MAX_UTF16_CHARS: usize = 1250;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReactionType {
    Like,
    Celebrate,
    Support,
    Love,
    Insightful,
    Funny,
}

impl ReactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReactionType::Like => "like",
            ReactionType::Celebrate => "celebrate",
            ReactionType::Support => "support",
            ReactionType::Love => "love",
            ReactionType::Insightful => "insightful",
            ReactionType::Funny => "funny",
        }
    }

    pub fn display_label(&self) -> &'static str {
        match self {
            ReactionType::Like => "Like 👍",
            ReactionType::Celebrate => "Celebrate 👏",
            ReactionType::Support => "Support 🤝",
            ReactionType::Love => "Love ❤️",
            ReactionType::Insightful => "Insightful 💡",
            ReactionType::Funny => "Funny 😂",
        }
    }
}

impl fmt::Display for ReactionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ReactionType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "like" | "thumbsup" => Ok(ReactionType::Like),
            "celebrate" | "applause" | "clap" => Ok(ReactionType::Celebrate),
            "support" | "handshake" => Ok(ReactionType::Support),
            "love" | "heart" => Ok(ReactionType::Love),
            "insightful" | "idea" | "bulb" => Ok(ReactionType::Insightful),
            "funny" | "laugh" => Ok(ReactionType::Funny),
            other => Err(AppError::InvalidInput(format!(
                "Invalid reaction type '{other}'. Valid options: like, celebrate, support, love, insightful, funny"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FeedSort {
    #[default]
    Recent,
    Top,
}

impl FeedSort {
    pub fn as_str(&self) -> &'static str {
        match self {
            FeedSort::Recent => "recent",
            FeedSort::Top => "top",
        }
    }
}

impl fmt::Display for FeedSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for FeedSort {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "recent" | "chronological" | "latest" => Ok(FeedSort::Recent),
            "top" | "popular" | "algorithmic" => Ok(FeedSort::Top),
            other => Err(AppError::InvalidInput(format!(
                "Invalid feed sort order '{other}'. Valid options: recent, top"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelinePost {
    pub urn: String,
    pub author_name: String,
    pub author_handle: Option<String>,
    pub author_headline: Option<String>,
    pub text: String,
    pub created_at: Option<String>,
    pub reaction_count: u64,
    pub comment_count: u64,
    pub repost_count: u64,
    pub my_reaction: Option<ReactionType>,
    pub media_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PostComment {
    pub urn: String,
    pub author_name: String,
    pub author_handle: Option<String>,
    pub text: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineResult {
    pub posts: Vec<TimelinePost>,
    pub total: usize,
    pub sort: FeedSort,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReactionResult {
    pub post_urn: String,
    pub action: String,
    pub reaction: Option<ReactionType>,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommentResult {
    pub post_urn: String,
    pub comment_urn: Option<String>,
    pub text: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepostResult {
    pub post_urn: String,
    pub commentary: Option<String>,
    pub repost_urn: Option<String>,
    pub success: bool,
}

pub fn validate_comment_content(text: &str) -> Result<(), AppError> {
    let utf16_chars = text.encode_utf16().count();
    if text.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Comment text cannot be empty.".to_string(),
        ));
    }
    if utf16_chars > LINKEDIN_COMMENT_MAX_UTF16_CHARS {
        return Err(AppError::InvalidInput(format!(
            "Comment exceeds LinkedIn 1,250 UTF-16 character limit ({utf16_chars} code units). Please shorten."
        )));
    }
    Ok(())
}
