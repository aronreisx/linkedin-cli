use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::errors::AppError;

/// Workplace flexibility policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkplaceType {
    OnSite,
    Remote,
    Hybrid,
}

impl WorkplaceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkplaceType::OnSite => "on-site",
            WorkplaceType::Remote => "remote",
            WorkplaceType::Hybrid => "hybrid",
        }
    }

    /// LinkedIn query param value for f_WT
    pub fn linkedin_code(&self) -> &'static str {
        match self {
            WorkplaceType::OnSite => "1",
            WorkplaceType::Remote => "2",
            WorkplaceType::Hybrid => "3",
        }
    }
}

impl FromStr for WorkplaceType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().trim() {
            "onsite" | "on-site" | "1" => Ok(WorkplaceType::OnSite),
            "remote" | "2" => Ok(WorkplaceType::Remote),
            "hybrid" | "3" => Ok(WorkplaceType::Hybrid),
            _ => Err(AppError::InvalidInput(format!(
                "Invalid workplace type: '{s}'. Expected: onsite, remote, hybrid"
            ))),
        }
    }
}

impl fmt::Display for WorkplaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Date posted timeframe filter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatePosted {
    Past24h,
    PastWeek,
    PastMonth,
    Any,
}

impl DatePosted {
    pub fn as_str(&self) -> &'static str {
        match self {
            DatePosted::Past24h => "past-24h",
            DatePosted::PastWeek => "past-week",
            DatePosted::PastMonth => "past-month",
            DatePosted::Any => "any",
        }
    }

    /// LinkedIn query param value for f_TPR (seconds)
    pub fn linkedin_code(&self) -> Option<&'static str> {
        match self {
            DatePosted::Past24h => Some("r86400"),
            DatePosted::PastWeek => Some("r604800"),
            DatePosted::PastMonth => Some("r2592000"),
            DatePosted::Any => None,
        }
    }
}

impl FromStr for DatePosted {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_ascii_lowercase().replace('_', "-");
        match normalized.trim() {
            "past-24h" | "24h" | "day" => Ok(DatePosted::Past24h),
            "past-week" | "week" | "7d" => Ok(DatePosted::PastWeek),
            "past-month" | "month" | "30d" => Ok(DatePosted::PastMonth),
            "any" | "all" => Ok(DatePosted::Any),
            _ => Err(AppError::InvalidInput(format!(
                "Invalid date posted filter: '{s}'. Expected: past-24h, past-week, past-month, any"
            ))),
        }
    }
}

impl fmt::Display for DatePosted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Experience level filter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperienceLevel {
    Internship,
    Entry,
    Associate,
    MidSenior,
    Director,
    Executive,
}

impl ExperienceLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExperienceLevel::Internship => "internship",
            ExperienceLevel::Entry => "entry",
            ExperienceLevel::Associate => "associate",
            ExperienceLevel::MidSenior => "mid-senior",
            ExperienceLevel::Director => "director",
            ExperienceLevel::Executive => "executive",
        }
    }

    /// LinkedIn query param value for f_E
    pub fn linkedin_code(&self) -> &'static str {
        match self {
            ExperienceLevel::Internship => "1",
            ExperienceLevel::Entry => "2",
            ExperienceLevel::Associate => "3",
            ExperienceLevel::MidSenior => "4",
            ExperienceLevel::Director => "5",
            ExperienceLevel::Executive => "6",
        }
    }
}

impl FromStr for ExperienceLevel {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_ascii_lowercase().replace('_', "-");
        match normalized.trim() {
            "internship" | "1" => Ok(ExperienceLevel::Internship),
            "entry" | "entry-level" | "2" => Ok(ExperienceLevel::Entry),
            "associate" | "3" => Ok(ExperienceLevel::Associate),
            "mid-senior" | "midsenior" | "mid" | "senior" | "4" => Ok(ExperienceLevel::MidSenior),
            "director" | "5" => Ok(ExperienceLevel::Director),
            "executive" | "exec" | "6" => Ok(ExperienceLevel::Executive),
            _ => Err(AppError::InvalidInput(format!(
                "Invalid experience level: '{s}'. Expected: internship, entry, associate, mid-senior, director, executive"
            ))),
        }
    }
}

impl fmt::Display for ExperienceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Structured search criteria for LinkedIn job listings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobSearchFilter {
    pub keywords: Option<String>,
    pub location: Option<String>,
    pub workplace: Vec<WorkplaceType>,
    pub date_posted: Option<DatePosted>,
    pub experience: Vec<ExperienceLevel>,
    pub easy_apply: bool,
    pub limit: usize,
    pub page: u32,
}

impl JobSearchFilter {
    /// Builds the LinkedIn jobs search URL query string
    pub fn build_url(&self) -> String {
        let mut params = Vec::new();

        if let Some(ref kw) = self.keywords {
            if !kw.trim().is_empty() {
                params.push(format!("keywords={}", urlencode_param(kw.trim())));
            }
        }

        if let Some(ref loc) = self.location {
            if !loc.trim().is_empty() {
                params.push(format!("location={}", urlencode_param(loc.trim())));
            }
        }

        if !self.workplace.is_empty() {
            let codes: Vec<&str> = self.workplace.iter().map(|w| w.linkedin_code()).collect();
            params.push(format!("f_WT={}", codes.join("%2C")));
        }

        if let Some(dp) = self.date_posted {
            if let Some(code) = dp.linkedin_code() {
                params.push(format!("f_TPR={code}"));
            }
        }

        if !self.experience.is_empty() {
            let codes: Vec<&str> = self.experience.iter().map(|e| e.linkedin_code()).collect();
            params.push(format!("f_E={}", codes.join("%2C")));
        }

        if self.easy_apply {
            params.push("f_AL=true".to_string());
        }

        if self.page > 1 {
            let start = (self.page - 1) * 25;
            params.push(format!("start={start}"));
        }

        if params.is_empty() {
            "https://www.linkedin.com/jobs/search/".to_string()
        } else {
            format!("https://www.linkedin.com/jobs/search/?{}", params.join("&"))
        }
    }
}

pub fn urlencode_param(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

/// Condensed job summary representation for search and lists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    pub id: String,
    pub urn: String,
    pub title: String,
    pub company_name: String,
    pub company_handle: Option<String>,
    pub location: String,
    pub workplace_type: Option<String>,
    pub posted_at: Option<String>,
    pub is_easy_apply: bool,
    pub job_url: String,
}

/// Result of a paginated job search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSearchResult {
    pub jobs: Vec<JobSummary>,
    pub total: usize,
    pub page: u32,
    pub query: Option<String>,
}

/// Organization summary attached to a job posting
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompanySummary {
    pub name: String,
    pub handle: Option<String>,
    pub url: Option<String>,
}

/// Hiring team recruiter / manager profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiringMember {
    pub name: String,
    pub handle: Option<String>,
    pub headline: Option<String>,
    pub profile_url: Option<String>,
}

/// Detailed job posting specification for semantic evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDetails {
    pub id: String,
    pub urn: String,
    pub title: String,
    pub company: CompanySummary,
    pub location: String,
    pub workplace_type: Option<String>,
    pub posted_at: Option<String>,
    pub applicant_count: Option<u32>,
    pub is_easy_apply: bool,
    pub salary_range: Option<String>,
    pub employment_type: Option<String>,
    pub experience_level: Option<String>,
    pub required_skills: Vec<String>,
    pub hiring_team: Vec<HiringMember>,
    pub description_markdown: String,
    pub job_url: String,
}

/// Result of a job save / bookmarking mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSaveResult {
    pub job_id: String,
    pub saved: bool,
    pub success: bool,
    pub message: String,
}
