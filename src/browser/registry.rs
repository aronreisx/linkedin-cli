use crate::errors::AppError;
use crate::infra::resolve_base_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// Persistent session entry tracking an active, bound Chromium supervisor daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionEntry {
    pub name: String,
    pub endpoint: String,
    pub pid: u32,
    pub created_at: u64,
}

/// Returns the directory where session registry metadata files are stored (`~/.linkedin-cli/sessions`).
pub fn sessions_dir() -> PathBuf {
    resolve_base_dir().join("sessions")
}

/// Returns the path to a session's registry JSON file.
pub fn session_file(name: &str) -> PathBuf {
    sessions_dir().join(format!("{name}.json"))
}

/// Atomically writes a session entry to disk under `~/.linkedin-cli/sessions/<name>.json`.
pub fn write_session(name: &str, endpoint: &str, pid: u32) -> Result<SessionEntry, AppError> {
    let dir = sessions_dir();
    fs::create_dir_all(&dir).map_err(AppError::Io)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let entry = SessionEntry {
        name: name.to_string(),
        endpoint: endpoint.to_string(),
        pid,
        created_at: now,
    };

    let file_path = session_file(name);
    let tmp_path = file_path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(&entry).map_err(AppError::Json)?;

    fs::write(&tmp_path, content).map_err(AppError::Io)?;
    fs::rename(&tmp_path, &file_path).map_err(AppError::Io)?;

    debug!(target: "browser", "Saved session registry for '{name}' (pid: {pid}) at {endpoint}");
    Ok(entry)
}

/// Reads a session entry from disk if present.
pub fn read_session(name: &str) -> Option<SessionEntry> {
    let path = session_file(name);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Removes a session registry file from disk.
pub fn clear_session(name: &str) -> Result<(), AppError> {
    let path = session_file(name);
    if path.exists() {
        let _ = fs::remove_file(path);
        debug!(target: "browser", "Cleared session registry for '{name}'");
    }
    Ok(())
}

/// Lists all registered browser session entries.
pub fn list_sessions() -> Result<Vec<SessionEntry>, AppError> {
    let dir = sessions_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    let read_dir = fs::read_dir(dir).map_err(AppError::Io)?;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(se) = serde_json::from_str::<SessionEntry>(&content) {
                    entries.push(se);
                }
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Checks whether a process with the given PID is currently alive on the host.
#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_pid_alive(_pid: u32) -> bool {
    true
}

/// Sends a SIGTERM signal to terminate a process by PID.
#[cfg(unix)]
pub fn terminate_pid(pid: u32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid as i32, 15) == 0 }
}

#[cfg(not(unix))]
pub fn terminate_pid(_pid: u32) -> bool {
    false
}

/// Finds an active, alive browser session. Automatically prunes stale dead-process entries.
pub fn find_active_session(name: &str) -> Option<SessionEntry> {
    let entry = read_session(name)?;
    if is_pid_alive(entry.pid) {
        Some(entry)
    } else {
        warn!(target: "browser", "Pruning stale session entry for '{name}' (PID {} is no longer alive)", entry.pid);
        let _ = clear_session(name);
        None
    }
}
