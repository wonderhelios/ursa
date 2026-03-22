//! Exponential backoff strategy for delivery retries

pub struct BackoffStrategy {
    // delay in ms for each retry: [5s,25,2min,10min]
    interval_ms: Vec<u64>,
    pub max_retries: usize,
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        Self {
            interval_ms: vec![5_000, 25_000, 120_000, 600_000],
            max_retries: 4,
        }
    }
}

impl BackoffStrategy {
    pub fn next_delay_ms(&self, retry_count: usize) -> Option<u64> {
        if retry_count >= self.max_retries {
            return None;
        }
        Some(
            self.interval_ms
                .get(retry_count)
                .copied()
                .unwrap_or(*self.interval_ms.last().unwrap()),
        )
    }
}
