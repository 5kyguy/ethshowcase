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
}
