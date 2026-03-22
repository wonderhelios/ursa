//! DeliveryQueue - write-ahead persistent queue
//!
//! Each item is a separate JSON file in `.ursa/queue/`.
//! On failure, items are moved to `.ursa/queue/failed/`.
//! Design: one file per item with unique UUID name → no concurrent write conflicts.

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

use super::backoff::BackoffStrategy;

// a queued delivery iterm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedDelivery {
    pub id: String,
    /// Delivery channel: "terminal", "file", "slack", etc.
    pub channel: String,
    /// Recipient (channel-specific: file path, slack handle, etc.)
    pub to: String,
    /// Message content
    pub text: String,
    pub retry_count: usize,
    pub last_error: Option<String>,
    /// Unix timestamp (secs) when enqueued
    pub enqueued_at: f64,
    /// Unix timestamp (secs) when next retry is due (0 = ready now)
    pub next_retry_at: f64,
}

pub struct DeliveryQueue {
    queue_dir: PathBuf,
    failed_dir: PathBuf,
}

impl DeliveryQueue {
    pub fn new(base_dir: PathBuf) -> anyhow::Result<Self> {
        let queue_dir = base_dir.join("queue");
        let failed_dir = base_dir.join("queue").join("failed");

        std::fs::create_dir_all(&queue_dir)?;
        std::fs::create_dir_all(&failed_dir)?;
        Ok(Self {
            queue_dir,
            failed_dir,
        })
    }

    // enqueue a delivery — writes to disk first (write-ahead)
    pub fn enqueue(&self, channel: &str, to: &str, text: &str) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = unix_now();
        let item = QueuedDelivery {
            id: id.clone(),
            channel: channel.to_string(),
            to: to.to_string(),
            text: text.to_string(),
            retry_count: 0,
            last_error: None,
            enqueued_at: now,
            next_retry_at: 0.0, // ready immediately
        };
        let path = self.queue_dir.join(format!("{}.json", id));
        std::fs::write(&path, serde_json::to_string(&item)?)?;
        info!("Enqueued delivery {} to channel '{}'", &id[..8], channel);
        Ok(id)
    }

    // return all items that are due for delivery (next_retry_at <= now)
    pub fn dequeue(&self) -> Vec<QueuedDelivery> {
        let now = unix_now();
        let Ok(entries) = std::fs::read_dir(&self.queue_dir) else {
            return vec![];
        };

        let mut items: Vec<QueuedDelivery> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().map(|x| x == "json").unwrap_or(false)
                    && e.file_name() != "failed"
            })
            .filter_map(|e| {
                let content = std::fs::read_to_string(e.path()).ok()?;
                serde_json::from_str::<QueuedDelivery>(&content).ok()
            })
            .filter(|item| item.next_retry_at <= now)
            .collect();

        // FIFO: sort by enqueue time
        items.sort_by(|a, b| a.enqueued_at.partial_cmp(&b.enqueued_at).unwrap());
        items
    }

    // mark delivery as successfully delivered — delete the file
    pub fn ack(&self, id: &str) -> anyhow::Result<()> {
        let path = self.queue_dir.join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(&path)?;
            info!("Delivery {} acked", &id[..8.min(id.len())]);
        }
        Ok(())
    }

    // mark delivery as failed — update retry time or move to failed dir
    pub fn fail(&self, id: &str, error: &str) -> anyhow::Result<()> {
        let path = self.queue_dir.join(format!("{}.json", id));
        if !path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&path)?;
        let mut item: QueuedDelivery = serde_json::from_str(&content)?;
        item.retry_count += 1;
        item.last_error = Some(error.to_string());

        let backoff = BackoffStrategy::default();
        match backoff.next_delay_ms(item.retry_count) {
            Some(delay_ms) => {
                item.next_retry_at = unix_now() + delay_ms as f64 / 1000.0;
                std::fs::write(&path, serde_json::to_string(&item)?)?;
                warn!(
                    "Delivery {} failed (attempt {}), retry in {}s",
                    &id[..8.min(id.len())],
                    item.retry_count,
                    delay_ms / 1000
                );
            }
            None => {
                // Max retries exceeded — move to failed dir
                let failed_path = self.failed_dir.join(format!("{}.json", id));
                std::fs::write(&failed_path, serde_json::to_string(&item)?)?;
                std::fs::remove_file(&path)?;
                warn!(
                    "Delivery {} permanently failed after {} retries",
                    &id[..8.min(id.len())],
                    item.retry_count
                );
            }
        }
        Ok(())
    }

    // list permanently failed deliveries (for inspection)
    pub fn list_failed(&self) -> Vec<QueuedDelivery> {
        let Ok(entries) = std::fs::read_dir(&self.failed_dir) else {
            return vec![];
        };
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
            .filter_map(|e| {
                let content = std::fs::read_to_string(e.path()).ok()?;
                serde_json::from_str(&content).ok()
            })
            .collect()
    }
}

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
