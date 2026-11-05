//! Polite HTTP fetching: bounded concurrency, global pacing, adaptive
//! slowdown on rate limiting, retries.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;
use tokio::sync::{Mutex, Semaphore};

use crate::error::{Error, Result};

/// Identifying user agent (be a good citizen).
pub const USER_AGENT: &str = concat!(
    "ethshowcase/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/5kyguy/ethshowcase)"
);

const MAX_RETRIES: u32 = 8;
/// Per-attempt backoff: `BACKOFF_BASE * 2^attempt` + jitter.
const BACKOFF_BASE: Duration = Duration::from_secs(2);
const BACKOFF_CAP: Duration = Duration::from_secs(120);
/// AIMD slowdown multiplier bounds.
const SLOWDOWN_MAX: f64 = 32.0;
/// Shared quiet period after a 429 before any new request starts.
const COOLDOWN_START: Duration = Duration::from_secs(15);
const COOLDOWN_MAX: Duration = Duration::from_secs(300);

/// Shared pacing state: request spacing, adaptive slowdown, and cool-downs.
#[derive(Debug)]
struct Pacer {
    last_start: Instant,
    /// Multiplier on the base interval. 1.0 when healthy.
    slowdown: f64,
    /// No request may start before this instant after a 429.
    cooldown_until: Instant,
    /// Current cool-down length; doubles on repeat 429s.
    cooldown: Duration,
    /// Consecutive successes since the last slowdown increase.
    consecutive_ok: u32,
}

impl Pacer {
    fn new(base_interval: Duration) -> Self {
        Self {
            last_start: Instant::now()
                .checked_sub(base_interval)
                .unwrap_or_else(Instant::now),
            slowdown: 1.0,
            cooldown_until: Instant::now(),
            cooldown: COOLDOWN_START,
            consecutive_ok: 0,
        }
    }

    /// Effective spacing between request starts.
    fn interval(&self, base: Duration) -> Duration {
        let scaled = base.mul_f64(self.slowdown);
        // Keep the effective interval bounded so slowdown never stalls runs.
        scaled.min(Duration::from_secs(10))
    }

    fn on_throttled(&mut self, base: Duration) {
        self.slowdown = (self.slowdown * 2.0).min(SLOWDOWN_MAX);
        self.cooldown = (self.cooldown * 2).min(COOLDOWN_MAX);
        self.cooldown_until = Instant::now() + self.cooldown;
        self.consecutive_ok = 0;
        tracing::warn!(
            slow_down_factor = self.slowdown as u64,
            cooldown_secs = self.cooldown.as_secs(),
            "rate limited — backing off globally"
        );
        let _ = base;
    }

    fn on_success(&mut self, base: Duration) {
        self.consecutive_ok += 1;
        if self.slowdown > 1.0 && self.consecutive_ok >= 100 {
            self.slowdown = (self.slowdown * 0.7).max(1.0);
            self.consecutive_ok = 0;
            tracing::info!(
                slow_down_factor = format!("{:.2}", self.slowdown).as_str(),
                "recovering pace"
            );
        }
        let _ = base;
    }
}

/// Shared HTTP client with politeness controls.
#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    interval: Duration,
    pace: Arc<Mutex<Pacer>>,
}

impl HttpClient {
    /// `concurrency`: max in-flight requests. `interval`: minimum spacing
    /// between request *starts* globally (before adaptive slowdown).
    pub fn new(concurrency: usize, interval: Duration) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            client,
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
            interval,
            pace: Arc::new(Mutex::new(Pacer::new(interval))),
        })
    }

    /// GET a URL, retrying transient failures with exponential backoff.
    pub async fn get_text(&self, url: &str) -> Result<String> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| Error::Flight("semaphore closed".into()))?;

        let mut attempt = 0u32;
        loop {
            self.pace().await;
            match self.client.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body = resp.text().await?;
                        {
                            let mut p = self.pace.lock().await;
                            p.on_success(self.interval);
                        }
                        return Ok(body);
                    }
                    let retry_after = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(Duration::from_secs);
                    if !is_retryable(status.as_u16()) || attempt >= MAX_RETRIES {
                        return Err(Error::Status {
                            status: status.as_u16(),
                            url: url.to_string(),
                        });
                    }
                    {
                        let mut p = self.pace.lock().await;
                        if status.as_u16() == 429 {
                            p.on_throttled(self.interval);
                            // Honor an explicit Retry-After by extending the
                            // shared cool-down.
                            if let Some(ra) = retry_after {
                                let until = Instant::now() + ra;
                                if until > p.cooldown_until {
                                    p.cooldown_until = until;
                                }
                            }
                        }
                    }
                    let wait = retry_after.unwrap_or_else(|| backoff(attempt));
                    tracing::warn!(
                        status = status.as_u16(),
                        url,
                        wait_ms = wait.as_millis() as u64,
                        "retrying"
                    );
                    tokio::time::sleep(wait).await;
                }
                Err(e) => {
                    if attempt >= MAX_RETRIES {
                        return Err(Error::Http(e));
                    }
                    let wait = backoff(attempt);
                    tracing::warn!(error = %e, url, wait_ms = wait.as_millis() as u64, "retrying");
                    tokio::time::sleep(wait).await;
                }
            }
            attempt += 1;
        }
    }

    /// Wait so request starts respect the effective interval and any
    /// cool-down. Holds the lock across the sleep so all workers serialize.
    async fn pace(&self) {
        let mut p = self.pace.lock().await;
        let now = Instant::now();
        if p.cooldown_until > now {
            tokio::time::sleep(p.cooldown_until - now).await;
        }
        let spacing = p.interval(self.interval);
        let now = Instant::now();
        let earliest = p.last_start + spacing;
        if earliest > now {
            tokio::time::sleep(earliest - now).await;
            p.last_start = earliest;
        } else {
            p.last_start = now;
        }
    }
}

fn is_retryable(status: u16) -> bool {
    status == 429 || status >= 500
}

fn backoff(attempt: u32) -> Duration {
    let exp = BACKOFF_BASE
        .saturating_mul(2u32.saturating_pow(attempt.min(6)))
        .min(BACKOFF_CAP);
    let jitter = rand::thread_rng().gen_range(0..1_000);
    exp + Duration::from_millis(jitter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert!(backoff(0) < backoff(2));
        assert!(backoff(2) < backoff(4));
        assert!(backoff(7) <= BACKOFF_CAP + Duration::from_secs(1));
    }

    #[tokio::test]
    async fn pacer_aims() {
        let mut p = Pacer::new(Duration::from_millis(100));
        assert_eq!(
            p.interval(Duration::from_millis(100)),
            Duration::from_millis(100)
        );
        p.on_throttled(Duration::from_millis(100));
        assert_eq!(p.slowdown, 2.0);
        assert_eq!(
            p.interval(Duration::from_millis(100)),
            Duration::from_millis(200)
        );
        for _ in 0..100 {
            p.on_success(Duration::from_millis(100));
        }
        assert!(
            p.slowdown < 2.0,
            "slowdown should decay after sustained success"
        );
    }
}
