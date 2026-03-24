// 命名通道调度器 (claw0 s10)
//! Named lane scheduler — concurrency control via named semaphores.
//!
//! Each lane is a named semaphore with a configurable concurrency limit.
//! Tasks submitted to the same lane are served in FIFO order (tokio semaphore
//! is fair by design) and at most `max_concurrency` tasks run in parallel.
//!
//! Standard lanes:
//! - LANE_MAIN       (concurrency=1): serial user request processing
//! - LANE_BACKGROUND (concurrency=4): parallel background work
//! - LANE_CRON       (concurrency=1): serial scheduled tasks

use std::future::Future;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tracing::debug;

// ===== Standard lane names ======
pub const LANE_MAIN: &str = "main";
pub const LANE_BACKGROUND: &str = "background";
pub const LANE_CRON: &str = "cron";

// ===== LaneScheduler =====
//
// All methods take `&self` and are safe to share across threads via `Arc`.
pub struct LaneScheduler {
    lanes: DashMap<String, Arc<Semaphore>>,
}

impl LaneScheduler {
    pub fn new() -> Self {
        Self {
            lanes: DashMap::new(),
        }
    }

    // create a scheduler pre-configured with the three standard lanes.
    pub fn with_standard_lanes(cfg: &super::config::RuntimeConfig) -> Self {
        let scheduler = Self::new();
        scheduler.create_lane(LANE_MAIN, cfg.main_concurrency);
        scheduler.create_lane(LANE_BACKGROUND, cfg.background_concurrency);
        scheduler.create_lane(LANE_CRON, cfg.cron_concurrency);
        scheduler
    }

    // register a new lane. If the lane already exists, this is a no-op.
    pub fn create_lane(&self, name: &str, max_concurrency: usize) {
        self.lanes
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(max_concurrency)));
        debug!("Lane '{}' created (concurrency={})", name, max_concurrency);
    }

    // returns the concurrency limit for a lane, or None if it doesn't exist.
    pub fn concurrency(&self, name: &str) -> Option<usize> {
        self.lanes.get(name).map(|s| s.available_permits())
    }

    // ===== Core submission methods =====

    /// Acquire a permit for the named lane, waiting if at capacity.
    /// The permit is released when it is dropped.
    pub async fn permit(&self, lane: &str) -> anyhow::Result<OwnedSemaphorePermit> {
        self.acquire(lane).await
    }

    async fn acquire(&self, lane: &str) -> anyhow::Result<OwnedSemaphorePermit> {
        let sem = self
            .lanes
            .get(lane)
            .ok_or_else(|| anyhow::anyhow!("Lane '{}' not found. Call create_lane first.", lane))?
            .clone();

        sem.acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("Lane '{}' semaphore closed", lane))
    }

    // run a task inside the named lane and await its result
    pub async fn run<F, Fut, T>(&self, lane: &str, task: F) -> anyhow::Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let permit = self.acquire(lane).await?;
        let result = task().await;
        drop(permit);
        Ok(result)
    }

    // spawn a task in the named lane as a backgound tokio task
    pub async fn spawn<F, Fut, T>(&self, lane: &str, task: F) -> anyhow::Result<JoinHandle<T>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let permit = self.acquire(lane).await?;
        let handle = tokio::spawn(async move {
            let result = task().await;
            drop(permit);
            result
        });
        Ok(handle)
    }

    // fire-and-forget: spawn without waiting for a lane slot.
    pub fn try_spawn<F, Fut, T>(&self, lane: &str, task: F) -> anyhow::Result<JoinHandle<T>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let sem = self
            .lanes
            .get(lane)
            .ok_or_else(|| anyhow::anyhow!("Lane '{}' not found", lane))?
            .clone();

        let permit = sem
            .try_acquire_owned()
            .map_err(|_| anyhow::anyhow!("Lane '{}' is at capacity", lane))?;

        let handle = tokio::spawn(async move {
            let result = task().await;
            drop(permit);
            result
        });
        Ok(handle)
    }
}

impl Default for LaneScheduler {
    fn default() -> Self {
        Self::with_standard_lanes(&super::config::RuntimeConfig::default())
    }
}