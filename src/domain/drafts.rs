use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DraftState {
    Draft,
    Scheduled,
    Published,
    Failed,
    Archived,
}

impl DraftState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Scheduled => "scheduled",
            Self::Published => "published",
            Self::Failed => "failed",
            Self::Archived => "archived",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "scheduled" => Some(Self::Scheduled),
            "published" => Some(Self::Published),
            "failed" => Some(Self::Failed),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    pub fn can_transition_to(&self, next: DraftState) -> bool {
        match (self, next) {
            // Draft can become Scheduled, Published, or Archived
            (Self::Draft, Self::Scheduled)
            | (Self::Draft, Self::Published)
            | (Self::Draft, Self::Archived) => true,
            // Scheduled can become Published, Failed, Draft, or Archived
            (Self::Scheduled, Self::Published)
            | (Self::Scheduled, Self::Failed)
            | (Self::Scheduled, Self::Draft)
            | (Self::Scheduled, Self::Archived) => true,
            // Failed can retry back to Scheduled or Draft
            (Self::Failed, Self::Scheduled)
            | (Self::Failed, Self::Draft)
            | (Self::Failed, Self::Archived) => true,
            // Published can only be Archived
            (Self::Published, Self::Archived) => true,
            // Archived is terminal or can restore to Draft
            (Self::Archived, Self::Draft) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Draft {
    pub id: String,
    pub text: String,
    pub media_path: Option<String>,
    pub scheduled_at: Option<String>,
    pub state: DraftState,
    pub post_urn: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Generates crontab line syntax for a scheduled draft
pub fn generate_cron_entry(draft: &Draft, bin_path: Option<&str>) -> Result<String, AppError> {
    let sched_str = draft.scheduled_at.as_deref().ok_or_else(|| {
        AppError::InvalidInput(format!(
            "Draft {} does not have a scheduled timestamp",
            draft.id
        ))
    })?;

    let dt = DateTime::parse_from_rfc3339(sched_str)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            AppError::InvalidInput(format!(
                "Invalid RFC3339 scheduled timestamp '{sched_str}': {e}"
            ))
        })?;

    let bin = bin_path.unwrap_or("linkedin");
    use chrono::Datelike;
    use chrono::Timelike;

    // Cron format: minute hour day month day-of-week command
    let cron_line = format!(
        "{:02} {:02} {:02} {:02} * {} post publish --draft-id {}",
        dt.minute(),
        dt.hour(),
        dt.day(),
        dt.month(),
        bin,
        draft.id
    );

    Ok(cron_line)
}

/// Generates systemd .service file content for a scheduled draft
pub fn generate_systemd_service(draft: &Draft, bin_path: Option<&str>) -> String {
    let bin = bin_path.unwrap_or("linkedin");
    format!(
        "[Unit]\n\
Description=LinkedIn Scheduled Post Publisher (Draft {})\n\
After=network-online.target\n\
Wants=network-online.target\n\n\
[Service]\n\
Type=oneshot\n\
ExecStart={} post publish --draft-id {}\n\
StandardOutput=journal\n\
StandardError=journal\n",
        draft.id, bin, draft.id
    )
}

/// Generates systemd .timer file content for a scheduled draft
pub fn generate_systemd_timer(draft: &Draft) -> Result<String, AppError> {
    let sched_str = draft.scheduled_at.as_deref().ok_or_else(|| {
        AppError::InvalidInput(format!(
            "Draft {} does not have a scheduled timestamp",
            draft.id
        ))
    })?;

    let dt = DateTime::parse_from_rfc3339(sched_str)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            AppError::InvalidInput(format!(
                "Invalid RFC3339 scheduled timestamp '{sched_str}': {e}"
            ))
        })?;

    let calendar_format = dt.format("%Y-%m-%d %H:%M:%S UTC").to_string();

    let content = format!(
        "[Unit]\n\
Description=Timer for LinkedIn Scheduled Post (Draft {})\n\n\
[Timer]\n\
OnCalendar={}\n\
Persistent=true\n\n\
[Install]\n\
WantedBy=timers.target\n",
        draft.id, calendar_format
    );

    Ok(content)
}
