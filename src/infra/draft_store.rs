use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::domain::{Draft, DraftState};
use crate::errors::AppError;
use crate::infra::resolve_base_dir;

pub trait DraftStore: Send + Sync {
    fn create_draft(
        &self,
        text: &str,
        media_path: Option<&str>,
        scheduled_at: Option<&str>,
    ) -> Result<Draft, AppError>;
    fn get_draft(&self, id: &str) -> Result<Option<Draft>, AppError>;
    fn list_drafts(&self, filter_state: Option<DraftState>) -> Result<Vec<Draft>, AppError>;
    fn transition_state(
        &self,
        id: &str,
        next_state: DraftState,
        post_urn: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<Draft, AppError>;
    fn delete_draft(&self, id: &str) -> Result<bool, AppError>;
}

pub struct SqliteDraftStore {
    conn: Mutex<Connection>,
}

impl SqliteDraftStore {
    pub fn new(conn: Connection) -> Result<Self, AppError> {
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::Database(format!("Failed to open in-memory SQLite: {e}")))?;
        Self::new(conn)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(AppError::Io)?;
        }
        let conn = Connection::open(path)
            .map_err(|e| AppError::Database(format!("Failed to open SQLite database: {e}")))?;
        Self::new(conn)
    }

    pub fn default_path() -> PathBuf {
        resolve_base_dir().join("drafts.db")
    }

    pub fn open_default() -> Result<Self, AppError> {
        Self::open(Self::default_path())
    }

    fn initialize_schema(&self) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Database("Database mutex poisoned".to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS drafts (
                id TEXT PRIMARY KEY,
                text TEXT NOT NULL,
                media_path TEXT,
                scheduled_at TEXT,
                state TEXT NOT NULL,
                post_urn TEXT,
                error_message TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_drafts_state ON drafts(state);",
        )
        .map_err(|e| AppError::Database(format!("Failed to initialize database schema: {e}")))?;

        Ok(())
    }
}

impl DraftStore for SqliteDraftStore {
    fn create_draft(
        &self,
        text: &str,
        media_path: Option<&str>,
        scheduled_at: Option<&str>,
    ) -> Result<Draft, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Database("Database mutex poisoned".to_string()))?;
        let id = format!("draft-{}", &format!("{:08x}", rand::random::<u32>())[..6]);
        let now = Utc::now().to_rfc3339();
        let state = if scheduled_at.is_some() {
            DraftState::Scheduled
        } else {
            DraftState::Draft
        };

        conn.execute(
            "INSERT INTO drafts (id, text, media_path, scheduled_at, state, post_urn, error_message, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                text,
                media_path,
                scheduled_at,
                state.as_str(),
                None::<String>,
                None::<String>,
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Database(format!("Failed to create draft: {e}")))?;

        Ok(Draft {
            id,
            text: text.to_string(),
            media_path: media_path.map(|s| s.to_string()),
            scheduled_at: scheduled_at.map(|s| s.to_string()),
            state,
            post_urn: None,
            error_message: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn get_draft(&self, id: &str) -> Result<Option<Draft>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Database("Database mutex poisoned".to_string()))?;
        let mut stmt = conn
            .prepare("SELECT id, text, media_path, scheduled_at, state, post_urn, error_message, created_at, updated_at FROM drafts WHERE id = ?1")
            .map_err(|e| AppError::Database(format!("Failed to prepare query: {e}")))?;

        let draft = stmt
            .query_row(params![id], |row| {
                let state_str: String = row.get(4)?;
                let state = DraftState::from_str_loose(&state_str).unwrap_or(DraftState::Draft);
                Ok(Draft {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    media_path: row.get(2)?,
                    scheduled_at: row.get(3)?,
                    state,
                    post_urn: row.get(5)?,
                    error_message: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .optional()
            .map_err(|e| AppError::Database(format!("Failed to fetch draft: {e}")))?;

        Ok(draft)
    }

    fn list_drafts(&self, filter_state: Option<DraftState>) -> Result<Vec<Draft>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Database("Database mutex poisoned".to_string()))?;
        let mut drafts = Vec::new();

        if let Some(state) = filter_state {
            let mut stmt = conn
                .prepare("SELECT id, text, media_path, scheduled_at, state, post_urn, error_message, created_at, updated_at FROM drafts WHERE state = ?1 ORDER BY created_at DESC")
                .map_err(|e| AppError::Database(format!("Failed to prepare query: {e}")))?;
            let rows = stmt
                .query_map(params![state.as_str()], |row| {
                    let state_str: String = row.get(4)?;
                    let state = DraftState::from_str_loose(&state_str).unwrap_or(DraftState::Draft);
                    Ok(Draft {
                        id: row.get(0)?,
                        text: row.get(1)?,
                        media_path: row.get(2)?,
                        scheduled_at: row.get(3)?,
                        state,
                        post_urn: row.get(5)?,
                        error_message: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                })
                .map_err(|e| AppError::Database(format!("Query failed: {e}")))?;

            for r in rows {
                drafts.push(r.map_err(|e| AppError::Database(format!("Row error: {e}")))?);
            }
        } else {
            let mut stmt = conn
                .prepare("SELECT id, text, media_path, scheduled_at, state, post_urn, error_message, created_at, updated_at FROM drafts ORDER BY created_at DESC")
                .map_err(|e| AppError::Database(format!("Failed to prepare query: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    let state_str: String = row.get(4)?;
                    let state = DraftState::from_str_loose(&state_str).unwrap_or(DraftState::Draft);
                    Ok(Draft {
                        id: row.get(0)?,
                        text: row.get(1)?,
                        media_path: row.get(2)?,
                        scheduled_at: row.get(3)?,
                        state,
                        post_urn: row.get(5)?,
                        error_message: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                })
                .map_err(|e| AppError::Database(format!("Query failed: {e}")))?;

            for r in rows {
                drafts.push(r.map_err(|e| AppError::Database(format!("Row error: {e}")))?);
            }
        }

        Ok(drafts)
    }

    fn transition_state(
        &self,
        id: &str,
        next_state: DraftState,
        post_urn: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<Draft, AppError> {
        let existing = self
            .get_draft(id)?
            .ok_or_else(|| AppError::NotFound(format!("Draft with ID '{id}' not found")))?;

        if !existing.state.can_transition_to(next_state) {
            return Err(AppError::InvalidInput(format!(
                "Invalid state transition: Cannot transition draft '{}' from '{}' to '{}'",
                id,
                existing.state.as_str(),
                next_state.as_str()
            )));
        }

        let now = Utc::now().to_rfc3339();
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Database("Database mutex poisoned".to_string()))?;

        conn.execute(
            "UPDATE drafts SET state = ?1, post_urn = ?2, error_message = ?3, updated_at = ?4 WHERE id = ?5",
            params![
                next_state.as_str(),
                post_urn,
                error_message,
                now,
                id
            ],
        )
        .map_err(|e| AppError::Database(format!("Failed to update draft state: {e}")))?;

        Ok(Draft {
            id: existing.id,
            text: existing.text,
            media_path: existing.media_path,
            scheduled_at: existing.scheduled_at,
            state: next_state,
            post_urn: post_urn.map(|s| s.to_string()),
            error_message: error_message.map(|s| s.to_string()),
            created_at: existing.created_at,
            updated_at: now,
        })
    }

    fn delete_draft(&self, id: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Database("Database mutex poisoned".to_string()))?;
        let affected = conn
            .execute("DELETE FROM drafts WHERE id = ?1", params![id])
            .map_err(|e| AppError::Database(format!("Failed to delete draft: {e}")))?;
        Ok(affected > 0)
    }
}
