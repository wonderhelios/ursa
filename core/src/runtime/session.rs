//! Session - conversation history persistence

use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;
use ursa_llm::provider::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: chrono::DateTime<Utc>,
    pub messages: Vec<Message>,
}

impl Session {
    pub fn new() -> Self {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        Self {
            id,
            created_at: Utc::now(),
            messages: vec![],
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SessionManager {
    sessions_dir: PathBuf,
}

impl SessionManager {
    pub fn new(sessions_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&sessions_dir).ok();
        Self { sessions_dir }
    }

    // save session messages to a JSONL file (one Message per line)
    pub fn save(&self, session: &Session) -> anyhow::Result<()> {
        let filename = format!(
            "{}_{}.jsonl",
            session.created_at.format("%Y%m%d_%H%M%S"),
            session.id
        );
        let path = self.sessions_dir.join(&filename);

        let lines: Vec<String> = session
            .messages
            .iter()
            .filter_map(|m| serde_json::to_string(m).ok())
            .collect();

        std::fs::write(&path, lines.join("\n"))?;
        info!(
            "Session saved: {} ({} messages)",
            filename,
            session.messages.len()
        );

        Ok(())
    }

    // load most recent session, or one matching an id prefix
    pub fn load(&self, id_prefix: Option<&str>) -> anyhow::Result<Option<Session>> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.sessions_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
            .collect();

        // sort newest first
        entries.sort_by_key(|e| e.file_name());
        entries.reverse();

        let target = match id_prefix {
            Some(prefix) => entries
                .into_iter()
                .find(|e| e.file_name().to_string_lossy().contains(prefix)),
            None => entries.into_iter().next(),
        };

        let entry = match target {
            Some(e) => e,
            None => return Ok(None),
        };

        let content = std::fs::read_to_string(entry.path())?;
        let messages: Vec<Message> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();

        // extract id from filename: YYYYMMDD_HHMMSS_<id>.jsonl
        let stem = entry
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let id = stem.split('_').next_back().unwrap_or("unknown").to_string();

        info!(
            "Session loaded: {} ({} messages)",
            entry.file_name().to_string_lossy(),
            messages.len()
        );

        Ok(Some(Session {
            id,
            created_at: Utc::now(),
            messages,
        }))
    }

    // list session filenames, newest first
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.sessions_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();

        names.sort();
        names.reverse();
        names
    }
}
