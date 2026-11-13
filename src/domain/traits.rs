use async_trait::async_trait;
use serde_json::Value;

use crate::domain::jobs::{JobDetails, JobSaveResult, JobSearchFilter, JobSearchResult, JobSummary};
use crate::domain::models::{
    AuthStatus, ContactInfo, InboxFilter, InboxResult,
    Invitation, Message, PeopleSearchResult, Profile, SelfProfile, StatusResult,
};
use crate::domain::timeline::{
    CommentResult, FeedSort, PostComment, ReactionResult, ReactionType, RepostResult, TimelinePost,
};
use crate::errors::AppError;

#[async_trait]
pub trait LinkedInDriver: Send + Sync {
    async fn check_auth_status(&self) -> Result<AuthStatus, AppError>;
    async fn authenticate(&self, username: Option<&str>, password: Option<&str>) -> Result<SelfProfile, AppError>;
    async fn whoami(&self) -> Result<SelfProfile, AppError>;
    async fn navigate_to(&self, url: &str) -> Result<(), AppError>;
    async fn evaluate_js(&self, script: &str) -> Result<Value, AppError>;
}

#[async_trait]
pub trait ProfileScraper: Send + Sync {
    async fn scrape_profile(&self, handle_or_url: &str, raw: bool) -> Result<(Profile, Option<Value>), AppError>;
}

#[async_trait]
pub trait ContactInfoFetcher: Send + Sync {
    async fn get_contact_info(&self, handle_or_url: &str, raw: bool) -> Result<(ContactInfo, Option<Value>), AppError>;
}

#[async_trait]
pub trait ConnectionManager: Send + Sync {
    async fn get_connection_status(&self, handle_or_url: &str) -> Result<StatusResult, AppError>;
    async fn send_connection_request(&self, handle_or_url: &str, note: Option<&str>) -> Result<StatusResult, AppError>;
}

#[async_trait]
pub trait InvitationManager: Send + Sync {
    async fn list_sent_invitations(&self, scroll: u32, limit: Option<usize>) -> Result<Vec<Invitation>, AppError>;
    async fn list_received_invitations(&self, scroll: u32, limit: Option<usize>) -> Result<Vec<Invitation>, AppError>;
    async fn withdraw_invitation(&self, handle_or_url: &str) -> Result<bool, AppError>;
    async fn accept_invitation(&self, handle_or_url: &str) -> Result<bool, AppError>;
    async fn ignore_invitation(&self, handle_or_url: &str) -> Result<bool, AppError>;
}

#[async_trait]
pub trait MessagingService: Send + Sync {
    async fn send_direct_message(&self, handle_or_url: &str, text: &str) -> Result<bool, AppError>;
    async fn send_direct_message_with_attachment(
        &self,
        handle_or_url: &str,
        text: Option<&str>,
        attachment: &std::path::Path,
    ) -> Result<bool, AppError>;
    async fn get_thread(&self, handle_or_url: &str, limit: Option<usize>) -> Result<Vec<Message>, AppError>;
    async fn list_inbox(&self, filter: &InboxFilter, raw: bool) -> Result<(InboxResult, Option<Value>), AppError>;
}

#[async_trait]
pub trait PeopleSearchService: Send + Sync {
    async fn search_people(
        &self,
        keywords: &str,
        networks: &[String],
        locations: &[String],
        hiring_for: &[String],
        actively_hiring: bool,
        page: u32,
    ) -> Result<PeopleSearchResult, AppError>;
}

#[async_trait]
pub trait TimelineService: Send + Sync {
    async fn list_feed(&self, limit: usize, sort: FeedSort) -> Result<Vec<TimelinePost>, AppError>;
    async fn list_member_feed(&self, handle: &str, limit: usize) -> Result<Vec<TimelinePost>, AppError>;
    async fn get_post(&self, urn: &str) -> Result<TimelinePost, AppError>;
    async fn list_comments(&self, post_urn: &str, limit: usize) -> Result<Vec<PostComment>, AppError>;
    async fn react(&self, post_urn: &str, reaction: ReactionType) -> Result<ReactionResult, AppError>;
    async fn unreact(&self, post_urn: &str) -> Result<ReactionResult, AppError>;
    async fn add_comment(&self, post_urn: &str, text: &str) -> Result<CommentResult, AppError>;
    async fn repost(&self, post_urn: &str, commentary: Option<&str>) -> Result<RepostResult, AppError>;
}

#[async_trait]
pub trait JobService: Send + Sync {
    async fn search_jobs(&self, filter: &JobSearchFilter) -> Result<JobSearchResult, AppError>;
    async fn get_job(&self, job_id_or_url: &str) -> Result<JobDetails, AppError>;
    async fn save_job(&self, job_id_or_url: &str) -> Result<JobSaveResult, AppError>;
    async fn unsave_job(&self, job_id_or_url: &str) -> Result<JobSaveResult, AppError>;
    async fn list_saved_jobs(&self, limit: usize) -> Result<Vec<JobSummary>, AppError>;
}
