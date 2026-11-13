use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::{AppConfig, CredentialStore, ProfileCredentials, ProfileSummary};
use crate::errors::AppError;

const KEYRING_SERVICE: &str = "linkedin";

pub fn resolve_credential_store() -> Box<dyn CredentialStore> {
    let backend = std::env::var("LINKEDIN_CLI_KEYRING_BACKEND")
        .unwrap_or_else(|_| "auto".to_string())
        .to_lowercase();
    match backend.as_str() {
        "mock" => Box::new(MockCredentialStore::new()),
        "file" => Box::new(FileCredentialStore::new()),
        _ => Box::new(KeyringCredentialStore::new()),
    }
}

pub fn resolve_base_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("LINKEDIN_CLI_HOME") {
        PathBuf::from(custom)
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        home.join(".linkedin-cli")
    } else {
        PathBuf::from("/tmp/linkedin-cli")
    }
}

// =========================================================================
// 1. In-Memory Mock Store (for CI and integration tests)
// =========================================================================

#[derive(Clone, Default)]
pub struct MockCredentialStore {
    creds: Arc<Mutex<HashMap<String, ProfileCredentials>>>,
    config: Arc<Mutex<AppConfig>>,
}

impl MockCredentialStore {
    pub fn new() -> Self {
        Self {
            creds: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(AppConfig::default())),
        }
    }
}

impl CredentialStore for MockCredentialStore {
    fn get_credentials(&self, profile: &str) -> Result<Option<ProfileCredentials>, AppError> {
        let lock = self
            .creds
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        Ok(lock.get(profile).cloned())
    }

    fn set_credentials(&self, profile: &str, creds: &ProfileCredentials) -> Result<(), AppError> {
        {
            let mut lock = self
                .creds
                .lock()
                .map_err(|e| AppError::Credential(e.to_string()))?;
            lock.insert(profile.to_string(), creds.clone());
        }
        let mut cfg = self
            .config
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        if !cfg.profiles.contains(&profile.to_string()) {
            cfg.profiles.push(profile.to_string());
        }
        Ok(())
    }

    fn delete_credentials(&self, profile: &str) -> Result<bool, AppError> {
        let mut lock = self
            .creds
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        let removed = lock.remove(profile).is_some();
        let mut cfg = self
            .config
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        cfg.profiles.retain(|p| p != profile);
        if cfg.active_profile == profile {
            cfg.active_profile = cfg
                .profiles
                .first()
                .cloned()
                .unwrap_or_else(|| "default".to_string());
        }
        Ok(removed)
    }

    fn list_profiles(&self) -> Result<Vec<ProfileSummary>, AppError> {
        let cfg = self
            .config
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        let creds_lock = self
            .creds
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        let base_dir = resolve_base_dir();

        let mut summaries = Vec::new();
        for profile in &cfg.profiles {
            let cred = creds_lock.get(profile);
            summaries.push(ProfileSummary {
                name: profile.clone(),
                is_active: profile == &cfg.active_profile,
                has_username: cred.and_then(|c| c.username.as_ref()).is_some(),
                has_password: cred.and_then(|c| c.password.as_ref()).is_some(),
                has_cookies: cred.and_then(|c| c.li_at.as_ref()).is_some(),
                user_data_dir: base_dir
                    .join("profiles")
                    .join(profile)
                    .to_string_lossy()
                    .to_string(),
            });
        }
        Ok(summaries)
    }

    fn get_active_profile(&self) -> Result<String, AppError> {
        let cfg = self
            .config
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        Ok(cfg.active_profile.clone())
    }

    fn set_active_profile(&self, profile: &str) -> Result<(), AppError> {
        let mut cfg = self
            .config
            .lock()
            .map_err(|e| AppError::Credential(e.to_string()))?;
        if !cfg.profiles.contains(&profile.to_string()) {
            cfg.profiles.push(profile.to_string());
        }
        cfg.active_profile = profile.to_string();
        Ok(())
    }
}

// =========================================================================
// 2. Local File Store (Fallback for headless servers)
// =========================================================================

pub struct FileCredentialStore {
    base_dir: PathBuf,
}

impl Default for FileCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FileCredentialStore {
    pub fn new() -> Self {
        let base_dir = resolve_base_dir();
        let _ = fs::create_dir_all(&base_dir);
        Self { base_dir }
    }

    fn config_path(&self) -> PathBuf {
        self.base_dir.join("config.json")
    }

    fn credentials_path(&self) -> PathBuf {
        self.base_dir.join("credentials.json")
    }

    fn read_config(&self) -> AppConfig {
        if let Ok(data) = fs::read_to_string(self.config_path()) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            AppConfig::default()
        }
    }

    fn write_config(&self, cfg: &AppConfig) -> Result<(), AppError> {
        let _ = fs::create_dir_all(&self.base_dir);
        let s = serde_json::to_string_pretty(cfg)?;
        fs::write(self.config_path(), s)?;
        Ok(())
    }

    fn read_all_creds(&self) -> HashMap<String, ProfileCredentials> {
        if let Ok(data) = fs::read_to_string(self.credentials_path()) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    fn write_all_creds(&self, map: &HashMap<String, ProfileCredentials>) -> Result<(), AppError> {
        let _ = fs::create_dir_all(&self.base_dir);
        let cred_file = self.credentials_path();
        let s = serde_json::to_string_pretty(map)?;
        fs::write(&cred_file, s)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&cred_file, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

impl CredentialStore for FileCredentialStore {
    fn get_credentials(&self, profile: &str) -> Result<Option<ProfileCredentials>, AppError> {
        let map = self.read_all_creds();
        Ok(map.get(profile).cloned())
    }

    fn set_credentials(&self, profile: &str, creds: &ProfileCredentials) -> Result<(), AppError> {
        let mut map = self.read_all_creds();
        map.insert(profile.to_string(), creds.clone());
        self.write_all_creds(&map)?;

        let mut cfg = self.read_config();
        if !cfg.profiles.contains(&profile.to_string()) {
            cfg.profiles.push(profile.to_string());
            self.write_config(&cfg)?;
        }
        Ok(())
    }

    fn delete_credentials(&self, profile: &str) -> Result<bool, AppError> {
        let mut map = self.read_all_creds();
        let removed = map.remove(profile).is_some();
        self.write_all_creds(&map)?;

        let mut cfg = self.read_config();
        cfg.profiles.retain(|p| p != profile);
        if cfg.active_profile == profile {
            cfg.active_profile = cfg
                .profiles
                .first()
                .cloned()
                .unwrap_or_else(|| "default".to_string());
        }
        self.write_config(&cfg)?;
        Ok(removed)
    }

    fn list_profiles(&self) -> Result<Vec<ProfileSummary>, AppError> {
        let cfg = self.read_config();
        let creds_map = self.read_all_creds();

        let mut summaries = Vec::new();
        for profile in &cfg.profiles {
            let cred = creds_map.get(profile);
            summaries.push(ProfileSummary {
                name: profile.clone(),
                is_active: profile == &cfg.active_profile,
                has_username: cred.and_then(|c| c.username.as_ref()).is_some(),
                has_password: cred.and_then(|c| c.password.as_ref()).is_some(),
                has_cookies: cred.and_then(|c| c.li_at.as_ref()).is_some(),
                user_data_dir: self
                    .base_dir
                    .join("profiles")
                    .join(profile)
                    .to_string_lossy()
                    .to_string(),
            });
        }
        Ok(summaries)
    }

    fn get_active_profile(&self) -> Result<String, AppError> {
        Ok(self.read_config().active_profile)
    }

    fn set_active_profile(&self, profile: &str) -> Result<(), AppError> {
        let mut cfg = self.read_config();
        if !cfg.profiles.contains(&profile.to_string()) {
            cfg.profiles.push(profile.to_string());
        }
        cfg.active_profile = profile.to_string();
        self.write_config(&cfg)?;
        Ok(())
    }
}

// =========================================================================
// 3. OS Keyring Store (Native Keychain / Secret Service with File Fallback)
// =========================================================================

pub struct KeyringCredentialStore {
    file_fallback: FileCredentialStore,
}

impl Default for KeyringCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringCredentialStore {
    pub fn new() -> Self {
        Self {
            file_fallback: FileCredentialStore::new(),
        }
    }

    fn keyring_entry(&self, profile: &str) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(KEYRING_SERVICE, profile)
            .map_err(|e| AppError::Credential(e.to_string()))
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn get_credentials(&self, profile: &str) -> Result<Option<ProfileCredentials>, AppError> {
        match self.keyring_entry(profile) {
            Ok(entry) => match entry.get_password() {
                Ok(json_secret) => {
                    let creds: ProfileCredentials = serde_json::from_str(&json_secret)?;
                    Ok(Some(creds))
                }
                Err(keyring::Error::NoEntry) => self.file_fallback.get_credentials(profile),
                Err(err) => {
                    tracing::debug!("Keyring retrieval failed ({err}); using file store fallback");
                    self.file_fallback.get_credentials(profile)
                }
            },
            Err(_) => self.file_fallback.get_credentials(profile),
        }
    }

    fn set_credentials(&self, profile: &str, creds: &ProfileCredentials) -> Result<(), AppError> {
        let serialized = serde_json::to_string(creds)?;
        let entry_res = self.keyring_entry(profile);
        let mut saved_keyring = false;

        if let Ok(entry) = entry_res {
            if let Err(err) = entry.set_password(&serialized) {
                tracing::debug!("Keyring set_password failed ({err}); using file store fallback");
            } else {
                saved_keyring = true;
            }
        }

        // Always update profile registry in config file
        self.file_fallback.set_credentials(profile, creds)?;

        if saved_keyring {
            tracing::info!(
                "Credentials securely saved to native OS keyring for profile '{profile}'"
            );
        }
        Ok(())
    }

    fn delete_credentials(&self, profile: &str) -> Result<bool, AppError> {
        if let Ok(entry) = self.keyring_entry(profile) {
            let _ = entry.delete_credential();
        }
        self.file_fallback.delete_credentials(profile)
    }

    fn list_profiles(&self) -> Result<Vec<ProfileSummary>, AppError> {
        self.file_fallback.list_profiles()
    }

    fn get_active_profile(&self) -> Result<String, AppError> {
        self.file_fallback.get_active_profile()
    }

    fn set_active_profile(&self, profile: &str) -> Result<(), AppError> {
        self.file_fallback.set_active_profile(profile)
    }
}

pub fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
