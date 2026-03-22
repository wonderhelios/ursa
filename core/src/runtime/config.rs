//! Runtime configuration

// Concurrency limits for each named lane
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Serial user request processing (default: 1)
    pub main_concurrency: usize,
    /// Parallel background tasks: subagents, delivery (default: 4)
    pub background_concurrency: usize,
    /// Serial scheduled/cron tasks (default: 1)
    pub cron_concurrency: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            main_concurrency: 1,
            background_concurrency: 4,
            cron_concurrency: 1,
        }
    }
}
