//! Typed records stored in the dataset.

use serde::{Deserialize, Serialize};

/// Event reference attached to a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRef {
    pub slug: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
}

/// Full event metadata (from the showcase filter dropdown payload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// A prize/bounty placement won by a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prize {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sponsor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sponsor_id: Option<String>,
    /// Placement as shown on the project (e.g. "1st place", "Finalist").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    /// The bounty/track title (e.g. "Best Uniswap Stack Contribution").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
}

/// ETHGlobal auto-evaluated hackathon scores (0-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scores {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originality: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub practicality: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technicality: Option<u8>,
}

/// A single showcase project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Internal ETHGlobal id (uuid, e.g. `sk506`).
    pub id: String,
    /// URL slug without the id suffix (e.g. `dedan`).
    pub slug: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<EventRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub youtube_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prizes: Vec<Prize>,
    #[serde(default)]
    pub finalist: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how_its_made: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scores: Option<Scores>,
    /// Canonical showcase URL.
    pub showcase_url: String,
    /// RFC3339 timestamp of the last scrape of this record.
    pub scraped_at: String,
    /// Whether the detail page (description / how-it's-made) has been scraped.
    #[serde(default)]
    pub detail_scraped: bool,
}

impl Project {
    /// Canonical showcase URL for a slug + id pair.
    pub fn showcase_url(slug: &str, id: &str) -> String {
        format!("https://ethglobal.com/showcase/{slug}-{id}")
    }
}

/// Dataset-level stats for the API and static site.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub projects: u64,
    pub events: u64,
    pub finalists: u64,
    pub with_description: u64,
    pub generated_at: String,
    /// Freshness of the underlying data: the latest `scraped_at` across all
    /// projects. Distinct from `generated_at` (build time) — re-deriving from
    /// cache updates the latter but not the former.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    /// Past-only cutoff applied at build time: events that had not finished
    /// before this timestamp are excluded from the dataset. `None` when the
    /// build kept live events (`--include-live`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cutoff: Option<String>,
}

impl Event {
    /// Past-only rule: an event belongs in the dataset when it is not
    /// cancelled/suspended and it ended at or before `now`. A missing status
    /// is tolerated (decide on `end_time` alone); a missing `end_time` keeps
    /// the event only if its status says it finished.
    pub fn is_past(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        let status_ok = match self.status.as_deref() {
            Some(s) => !matches!(
                s.to_ascii_lowercase().as_str(),
                "cancelled" | "canceled" | "suspended" | "postponed"
            ),
            None => true,
        };
        if !status_ok {
            return false;
        }
        match self
            .end_time
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        {
            Some(end) => end <= now,
            None => self
                .status
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("finished")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(status: Option<&str>, end: Option<&str>) -> Event {
        Event {
            id: "1".into(),
            name: "Test".into(),
            slug: "test".into(),
            kind: Some("hackathon".into()),
            start_time: Some("2024-01-01T00:00:00Z".into()),
            end_time: end.map(String::from),
            status: status.map(String::from),
        }
    }

    #[test]
    fn past_event_rule() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-06T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        // Finished long ago.
        assert!(event(Some("finished"), Some("2024-09-13T00:00:00Z")).is_past(now));
        // Cancelled — never included, even though it "ended".
        assert!(!event(Some("cancelled"), Some("2020-05-17T04:00:00Z")).is_past(now));
        // Still running.
        assert!(!event(Some("finished"), Some("2026-12-01T00:00:00Z")).is_past(now));
        // Live event that already claims finished status but ends in future.
        assert!(!event(Some("finished"), Some("2026-12-01T00:00:00Z")).is_past(now));
        // Missing end_time but finished status.
        assert!(event(Some("finished"), None).is_past(now));
        // Missing both — cannot prove it ended.
        assert!(!event(None, None).is_past(now));
        // Missing status, ended long ago.
        assert!(event(None, Some("2024-09-13T00:00:00Z")).is_past(now));
    }
}
