//! Polite HTTP fetching: bounded concurrency, global pacing, retries.

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

const MAX_RETRIES: u32 = 5;

/// Shared HTTP client with politeness controls.
#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    interval: Duration,
    /// Serializes pacing decisions so the global rate is respected.
    pace: Arc<Mutex<Instant>>,
}

impl HttpClient {
    /// `concurrency`: max in-flight requests. `interval`: minimum spacing
    /// between request *starts* globally.
    pub fn new(concurrency: usize, interval: Duration) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            client,
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
            interval,
            pace: Arc::new(Mutex::new(Instant::now())),
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
            let response = self.client.get(url).send().await;
            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(resp.text().await?);
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

    /// Wait so that request starts are at least `interval` apart.
    async fn pace(&self) {
        let mut last = self.pace.lock().await;
        let now = Instant::now();
        let earliest = *last + self.interval;
        if earliest > now {
            tokio::time::sleep(earliest - now).await;
            *last = earliest;
        } else {
            *last = now;
        }
    }
}

fn is_retryable(status: u16) -> bool {
    status == 429 || status >= 500
}

fn backoff(attempt: u32) -> Duration {
    let base = Duration::from_millis(500u64.saturating_mul(1 << attempt.min(6)));
    let jitter = rand::thread_rng().gen_range(0..250);
    base + Duration::from_millis(jitter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows() {
        assert!(backoff(0) < backoff(2));
        assert!(backoff(2) < backoff(4));
    }
}
