//! Persistent memory store
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub access_count: usize,
}

pub struct MemoryStore {
    memory_file: PathBuf,
    entries: Vec<MemoryEntry>,
}

impl MemoryStore {
    // load from file (creates empty store if file doesn't exist)
    pub fn load(memory_file: PathBuf) -> anyhow::Result<Self> {
        let entries = if memory_file.exists() {
            let content = std::fs::read_to_string(&memory_file)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            vec![]
        };
        info!(
            "MemoryStore loaded {} entries from {:?}",
            entries.len(),
            memory_file
        );
        Ok(Self {
            memory_file,
            entries,
        })
    }

    // write a new memory entry and persist
    pub fn write(&mut self, content: &str, tags: Vec<String>) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.entries.push(MemoryEntry {
            id: id.clone(),
            content: content.to_string(),
            tags,
            created_at: Utc::now(),
            access_count: 0,
        });
        self.persist()?;
        info!("Memory written: {:.60}", content);
        Ok(id)
    }

    // search entries by relevance to query
    pub fn search(&self, query: &str, limit: usize) -> Vec<&MemoryEntry> {
        let q = query.to_lowercase();
        let mut scored: Vec<(&MemoryEntry, f32)> = self
            .entries
            .iter()
            .filter_map(|e| {
                let score = self.score(e, &q);
                if score > 0.0 { Some((e, score)) } else { None }
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scored.into_iter().take(limit).map(|(e, _)| e).collect()
    }

    pub fn all(&self) -> &[MemoryEntry] {
        &self.entries
    }

    fn score(&self, entry: &MemoryEntry, query: &str) -> f32 {
        let content_lower = entry.content.to_lowercase();
        let mut score = 0.0_f32;

        // keyword match in content
        for word in query.split_whitespace() {
            if content_lower.contains(word) {
                score += 1.0;
            }
        }

        // tag match
        for tag in &entry.tags {
            if tag.to_lowercase().contains(query) {
                score += 0.5;
            }
        }

        // recency boost (newer = higher score)
        let days_old = (Utc::now() - entry.created_at).num_days() as f32;
        score -= days_old * 0.01;

        // access frequency boost
        score += entry.access_count as f32 * 0.1;

        score.max(0.0)
    }

    fn persist(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.memory_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.memory_file, contents)?;
        Ok(())
    }
}
