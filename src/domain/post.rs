use serde::{Deserialize, Serialize};
use crate::errors::AppError;

pub const LINKEDIN_POST_MAX_UTF16_CHARS: usize = 3000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PostValidationResult {
    pub utf16_chars: usize,
    pub max_allowed: usize,
    pub is_valid: bool,
    pub excess_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedPost {
    pub body: String,
    pub media_path: Option<String>,
    pub utf16_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaUploadStep {
    pub step: usize,
    pub action: String,
    pub endpoint_or_target: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaUploadPlan {
    pub media_file: String,
    pub asset_urn: String,
    pub upload_url: String,
    pub steps: Vec<MediaUploadStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PostCreationResult {
    pub post_urn: Option<String>,
    pub visibility: String,
    pub text: String,
    pub utf16_length: usize,
    pub media: Option<MediaUploadPlan>,
}

/// Computes the exact UTF-16 code unit count matching LinkedIn's backend validation
pub fn count_utf16_units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Validates post length against the 3,000 UTF-16 character limit
pub fn validate_post_content(text: &str) -> Result<PostValidationResult, AppError> {
    let utf16_chars = count_utf16_units(text);
    let is_valid = utf16_chars <= LINKEDIN_POST_MAX_UTF16_CHARS;
    let excess_chars = utf16_chars.saturating_sub(LINKEDIN_POST_MAX_UTF16_CHARS);

    if !is_valid {
        return Err(AppError::InvalidInput(format!(
            "Post exceeds LinkedIn 3,000 UTF-16 character limit ({utf16_chars} code units, excess: {excess_chars}). Please shorten your draft."
        )));
    }

    Ok(PostValidationResult {
        utf16_chars,
        max_allowed: LINKEDIN_POST_MAX_UTF16_CHARS,
        is_valid,
        excess_chars,
    })
}

/// Parses a Markdown post file, stripping frontmatter and detecting embedded media
pub fn parse_markdown_post(content: &str) -> Result<ParsedPost, AppError> {
    let mut text = content.trim();

    // Strip YAML frontmatter if present (e.g. ---\ntitle: ...\n---)
    if text.starts_with("---") {
        if let Some(end_idx) = text[3..].find("---") {
            text = text[3 + end_idx + 3..].trim();
        }
    }

    // Extract first markdown image link if present: ![alt](path)
    let mut media_path = None;
    if let Some(start_img) = text.find("![") {
        if let Some(mid_img) = text[start_img..].find("](") {
            let path_start = start_img + mid_img + 2;
            if let Some(end_img) = text[path_start..].find(')') {
                let path = &text[path_start..path_start + end_img];
                media_path = Some(path.to_string());
            }
        }
    }

    let utf16_length = count_utf16_units(text);

    Ok(ParsedPost {
        body: text.to_string(),
        media_path,
        utf16_length,
    })
}

/// Constructs a 3-step media upload simulation plan matching LinkedIn's protocol
pub fn plan_media_upload(media_file: &str) -> MediaUploadPlan {
    let dummy_id = rand::random::<u32>();
    let asset_urn = format!("urn:li:digitalmediaAsset:C4D05AQF{dummy_id}");
    let upload_url = format!("https://api.linkedin.com/mediaUpload/temp/{dummy_id}");

    let steps = vec![
        MediaUploadStep {
            step: 1,
            action: "initialize_upload".to_string(),
            endpoint_or_target: "/voyager/api/feed/richMedia?action=initializeUpload".to_string(),
            status: "ready".to_string(),
        },
        MediaUploadStep {
            step: 2,
            action: "upload_binary".to_string(),
            endpoint_or_target: upload_url.clone(),
            status: "ready".to_string(),
        },
        MediaUploadStep {
            step: 3,
            action: "create_share".to_string(),
            endpoint_or_target: "/voyager/api/contentcreation/normShares".to_string(),
            status: "ready".to_string(),
        },
    ];

    MediaUploadPlan {
        media_file: media_file.to_string(),
        asset_urn,
        upload_url,
        steps,
    }
}
