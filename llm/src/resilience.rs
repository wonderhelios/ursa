//! Resilience layer: retry policy, auth rotation, circuit breaker.
//!
//! Usage:
//! ```
//! use ursa_llm::resilience::{Resilience, RetryPolicy, AuthManager, CircuitBreaker};
//!
//! async fn example() -> anyhow::Result<()> {
//!     // Create a simple auth manager with a test key
//!     let auth = AuthManager::new(vec![
//!         ursa_llm::resilience::AuthProfile {
//!             name: "test".to_string(),
//!             api_key: "test-key".to_string(),
//!         }
//!     ]);
//!
//!     let resilience = Resilience::builder()
//!         .retry(RetryPolicy::default())
//!         .auth(auth)
//!         .circuit_breaker(CircuitBreaker::default())
//!         .build();
//!
//!     // Example async operation
//!     let result = resilience
//!         .execute(|api_key| async move {
//!             // Simulate an API call
//!             if api_key == "test-key" {
//!                 Ok("success")
//!             } else {
//!                 Err(anyhow::anyhow!("Invalid API key"))
//!             }
//!         })
//!         .await?;
//!
//!     println!("Result: {}", result);
//!     Ok(())
//! }
//! ```

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

// ====== RetryPolicy =====

const DEFAULT_RETRYABLE: &[&str] = &[
    "429",
    "500",
    "502",
    "503",
    "504",
    "rate limit",
    "timeout",
    "connection",
    "reset",
];

// backoff delays in milliseconds for successive retry attempts
const DEFAULT_BACKOFF_MS: &[u64] = &[1_000, 5_000, 15_000];

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub backoff_ms: Vec<u64>,
    pub retryable: Vec<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_ms: DEFAULT_BACKOFF_MS.to_vec(),
            retryable: DEFAULT_RETRYABLE.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl RetryPolicy {
    pub fn is_retryable(&self, error: &str) -> bool {
        let lower = error.to_lowercase();
        self.retryable.iter().any(|r| lower.contains(r.as_str()))
    }

    pub fn is_auth_error(&self, error: &str) -> bool {
        let lower = error.to_lowercase();
        lower.contains("401")
            || lower.contains("403")
            || lower.contains("rate limit")
            || lower.contains("429")
    }

    pub fn backoff_for(&self, attempt: usize) -> Duration {
        let ms = self
            .backoff_ms
            .get(attempt)
            .copied()
            .unwrap_or_else(|| *self.backoff_ms.last().unwrap_or(&5_000));
        Duration::from_millis(ms)
    }
}

// ===== AuthProfile / AuthManager =====

#[derive(Debug, Clone)]
pub struct AuthProfile {
    pub name: String,
    pub api_key: String,
}

// round-robin auth rotation across multiple API keys.
// thread-safe: index is atomic, failed set is behind a Mutex.
pub struct AuthManager {
    profiles: Vec<AuthProfile>,
    current: AtomicUsize,
    failed: Mutex<HashSet<usize>>,
}

impl AuthManager {
    pub fn new(profiles: Vec<AuthProfile>) -> Self {
        assert!(
            !profiles.is_empty(),
            "AuthManager requires at least one profile"
        );
        Self {
            profiles,
            current: AtomicUsize::new(0),
            failed: Mutex::new(HashSet::new()),
        }
    }

    // load profiles from environment variables:
    //   URSA_LLM_API_KEY        (required)
    //   URSA_LLM_API_KEY_2      (optional backup)
    //   URSA_LLM_API_KEY_3      (optional backup 2)
    pub fn from_env() -> Option<Self> {
        let primary = std::env::var("URSA_LLM_API_KEY").ok()?;
        if primary.is_empty() {
            return None;
        }

        let mut profiles = vec![AuthProfile {
            name: "primary".to_string(),
            api_key: primary,
        }];

        for i in 2..=9 {
            if let Ok(key) = std::env::var(format!("URSA_LLM_API_KEY_{}", i))
                && !key.is_empty() {
                    profiles.push(AuthProfile {
                        name: format!("backup_{}", i),
                        api_key: key,
                    });
                }
        }

        Some(Self::new(profiles))
    }

    // get the current peofile's API key
    pub fn current_key(&self) -> &str {
        let idx = self.current.load(Ordering::Relaxed);
        &self.profiles[idx].api_key
    }

    /// rotate to the next non-failed profile. Returns true if a new profile was found.
    pub fn rotate(&self) -> bool {
        let failed = self.failed.lock().unwrap();
        let n = self.profiles.len();

        for offset in 1..=n {
            let next = (self.current.load(Ordering::Relaxed) + offset) % n;
            if !failed.contains(&next) {
                self.current.store(next, Ordering::Relaxed);
                warn!("Auth rotated to profile '{}'", self.profiles[next].name);
                return true;
            }
        }
        false
    }

    /// mark the current profile as permanently failed
    pub fn mark_current_failed(&self) {
        let idx = self.current.load(Ordering::Relaxed);
        warn!(
            "Marking auth profile '{}' as failed",
            self.profiles[idx].name
        );
        self.failed.lock().unwrap().insert(idx);
    }

    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }
}

// ===== CircuitBreaker =====

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,   // Normal — requests go through
    Open,     // Failing — reject immediately
    HalfOpen, // Testing — one probe request allowed
}

pub struct CircuitBreaker {
    failure_threshold: usize,
    reset_after: Duration,
    // interior mutability: state transitions are infrequent
    state: Mutex<CircuitState>,
    failures: Mutex<usize>,
    opened_at: Mutex<Option<Instant>>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(30))
    }
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, reset_after: Duration) -> Self {
        Self {
            failure_threshold,
            reset_after,
            state: Mutex::new(CircuitState::Closed),
            failures: Mutex::new(0),
            opened_at: Mutex::new(None),
        }
    }

    // returns true if the request should be allowed through
    pub fn allow_request(&self) -> bool {
        let mut state = self.state.lock().unwrap();

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // check if reset period has passed → try half-open
                let opened = self.opened_at.lock().unwrap();
                if opened
                    .map(|t| t.elapsed() >= self.reset_after)
                    .unwrap_or(false)
                {
                    drop(opened);
                    *state = CircuitState::HalfOpen;
                    debug!("Circuit breaker → HalfOpen (testing recovery)");
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    // record a successful request
    pub fn on_success(&self) {
        let mut state = self.state.lock().unwrap();
        if *state == CircuitState::HalfOpen {
            debug!("Circuit breaker → Closed (recovered)");
        }
        *state = CircuitState::Closed;
        *self.failures.lock().unwrap() = 0;
    }

    // record a failed request
    pub fn on_failure(&self) {
        let mut failures = self.failures.lock().unwrap();
        *failures += 1;

        let mut state = self.state.lock().unwrap();
        if *failures >= self.failure_threshold || *state == CircuitState::HalfOpen {
            *state = CircuitState::Open;
            *self.opened_at.lock().unwrap() = Some(Instant::now());
            warn!(
                "Circuit breaker → Open ({} failures, waiting {:?})",
                failures, self.reset_after
            );
        }
    }

    pub fn state(&self) -> CircuitState {
        *self.state.lock().unwrap()
    }
}

// ===== Resilience =====

/// Bundles RetryPolicy + AuthManager + CircuitBreaker into a single composable wrapper.
pub struct Resilience {
    pub policy: RetryPolicy,
    pub auth: AuthManager,
    pub circuit: CircuitBreaker,
}

impl Resilience {
    pub fn builder() -> ResilienceBuilder {
        ResilienceBuilder::default()
    }

    /// execute an async operation with retry, auth rotation, and circuit breaking.
    ///
    /// the closure receives the current API key on each attempt so it can use
    /// the rotated key after an auth failure.
    pub async fn execute<F, Fut, T>(&self, op: F) -> anyhow::Result<T>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<T>>,
    {
        if !self.circuit.allow_request() {
            return Err(anyhow::anyhow!(
                "Circuit breaker is open — service unavailable, retry later"
            ));
        }

        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..self.policy.max_attempts {
            let api_key = self.auth.current_key().to_string();
            match op(api_key).await {
                Ok(result) => {
                    self.circuit.on_success();
                    return Ok(result);
                }
                Err(e) => {
                    let msg = e.to_string();
                    self.circuit.on_failure();

                    if !self.policy.is_retryable(&msg) {
                        warn!("Non-retryable error on attempt {}: {}", attempt + 1, msg);
                        return Err(e);
                    }

                    warn!(
                        "Retryable error on attempt {}/{}: {}",
                        attempt + 1,
                        self.policy.max_attempts,
                        msg
                    );

                    // Rotate auth on rate limit / auth errors
                    if self.policy.is_auth_error(&msg)
                        && !self.auth.rotate() {
                            warn!("All auth profiles exhausted");
                            return Err(e);
                        }

                    last_error = Some(e);
                    // Wait before retrying (skip wait on last attempt)
                    if attempt + 1 < self.policy.max_attempts {
                        let delay = self.policy.backoff_for(attempt);
                        debug!("Waiting {:?} before retry", delay);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All retry attempts exhausted")))
    }
}

// ===== Builder =====

#[derive(Default)]
pub struct ResilienceBuilder {
    policy: Option<RetryPolicy>,
    auth: Option<AuthManager>,
    circuit: Option<CircuitBreaker>,
}

impl ResilienceBuilder {
    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    pub fn auth(mut self, auth: AuthManager) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn circuit_breaker(mut self, cb: CircuitBreaker) -> Self {
        self.circuit = Some(cb);
        self
    }

    /// build with provided config, falling back to defaults.
    /// panics if no AuthManager is provided (auth is required).
    pub fn build(self) -> Resilience {
        Resilience {
            policy: self.policy.unwrap_or_default(),
            auth: self.auth.expect("AuthManager is required"),
            circuit: self.circuit.unwrap_or_default(),
        }
    }
}