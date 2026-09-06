use std::path::PathBuf;
use crate::infra::{resolve_base_dir, resolve_credential_store};

#[derive(Debug, Clone)]
pub struct BrowserConfig {
    pub headed: bool,
    pub user_data_dir: Option<PathBuf>,
    pub cdp_url: Option<String>,
    pub session_name: String,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headed: false,
            user_data_dir: None,
            cdp_url: None,
            session_name: "default".to_string(),
        }
    }
}

impl BrowserConfig {
    pub fn resolve_user_data_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.user_data_dir {
            return dir.clone();
        }

        let base_dir = resolve_base_dir();
        let target_session = if self.session_name == "default" {
            let store = resolve_credential_store();
            store.get_active_profile().unwrap_or_else(|_| "default".to_string())
        } else {
            self.session_name.clone()
        };

        base_dir.join("profiles").join(target_session)
    }
}
