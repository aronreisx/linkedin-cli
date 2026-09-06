use serde::{Deserialize, Serialize};
use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProfileCredentials {
    pub username: Option<String>,
    pub password: Option<String>,
    pub li_at: Option<String>,
    pub jsessionid: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSummary {
    pub name: String,
    pub is_active: bool,
    pub has_username: bool,
    pub has_password: bool,
    pub has_cookies: bool,
    pub user_data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub active_profile: String,
    pub profiles: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_profile: "default".to_string(),
            profiles: vec!["default".to_string()],
        }
    }
}

pub trait CredentialStore: Send + Sync {
    fn get_credentials(&self, profile: &str) -> Result<Option<ProfileCredentials>, AppError>;
    fn set_credentials(&self, profile: &str, creds: &ProfileCredentials) -> Result<(), AppError>;
    fn delete_credentials(&self, profile: &str) -> Result<bool, AppError>;
    fn list_profiles(&self) -> Result<Vec<ProfileSummary>, AppError>;
    fn get_active_profile(&self) -> Result<String, AppError>;
    fn set_active_profile(&self, profile: &str) -> Result<(), AppError>;
}
