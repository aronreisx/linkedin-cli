use async_trait::async_trait;
use serde_json::Value;

use crate::domain::models::{
    AuthStatus, ContactInfo, InboxFilter, InboxResult,
    Invitation, Message, PeopleSearchResult, Profile, SelfProfile, StatusResult,
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
        page: u32,
    ) -> Result<PeopleSearchResult, AppError>;
}
