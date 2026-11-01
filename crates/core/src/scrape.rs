//! Scraping orchestration: index walk and detail-page enrichment.

use std::collections::HashMap;
use std::time::Duration;

use futures::stream::{self, StreamExt};

use crate::cache::HtmlCache;
use crate::error::Result;
use crate::flight::FlightDoc;
use crate::http::HttpClient;
use crate::model::{Event, Project};
use crate::parse;

pub const BASE_URL: &str = "https://ethglobal.com";

/// Options for an index scrape.
#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub start_page: u32,
    pub max_pages: Option<u32>,
    /// If true, bypass the cache for listing pages.
    pub fresh: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            start_page: 1,
            max_pages: None,
            fresh: false,
        }
    }
}

/// Result of an index scrape.
#[derive(Debug, Default)]
pub struct IndexResult {
    pub projects: Vec<Project>,
    pub events: Vec<Event>,
    pub pages: u32,
}

/// Shared scraping state.
pub struct Scraper {
    http: HttpClient,
    cache: HtmlCache,
}

impl Scraper {
    pub fn new(http: HttpClient, cache: HtmlCache) -> Self {
        Self { http, cache }
    }

    async fn listing_page(&self, page: u32, fresh: bool) -> Result<String> {
        let key = format!("page-{page}");
        if !fresh && let Some(html) = self.cache.get(&key) {
            return Ok(html);
        }
        let url = format!("{BASE_URL}/showcase?page={page}");
        let html = self.http.get_text(&url).await?;
        self.cache.put(&key, &html)?;
        Ok(html)
    }

    async fn project_page(&self, slug: &str, id: &str, fresh: bool) -> Result<String> {
        if !fresh && let Some(html) = self.cache.get(id) {
            return Ok(html);
        }
        let url = format!("{BASE_URL}/showcase/{slug}-{id}");
        let html = self.http.get_text(&url).await?;
        self.cache.put(id, &html)?;
        Ok(html)
    }

    /// Walk showcase listing pages until they stop yielding projects.
    pub async fn scrape_index(&self, opts: &IndexOptions) -> Result<IndexResult> {
        let mut projects: HashMap<String, Project> = HashMap::new();
        let mut events: HashMap<String, Event> = HashMap::new();
        let mut page = opts.start_page.max(1);
        let mut pages_fetched = 0u32;

        loop {
            if let Some(max) = opts.max_pages
                && pages_fetched >= max
            {
                break;
            }
            let html = self.listing_page(page, opts.fresh).await?;
            let now = chrono::Utc::now().to_rfc3339();
            let (page_projects, page_events) = parse::parse_page(&html, &now)?;

            if page_projects.is_empty() {
                tracing::info!(page, "no projects on page — index complete");
                break;
            }

            let new = page_projects
                .into_iter()
                .filter(|p| projects.insert(p.id.clone(), p.clone()).is_none())
                .count();
            for e in page_events {
                events.insert(e.slug.clone(), e.clone());
            }
            // Enrich project events (listing pages embed only the event name).
            for p in projects.values_mut() {
                if let Some(ev) = &mut p.event {
                    if let Some(canonical) = events.values().find(|e| e.name == ev.name) {
                        ev.slug = canonical.slug.clone();
                        if ev.start_time.is_none() {
                            ev.start_time = canonical.start_time.clone();
                        }
                    } else if ev.slug.is_empty() {
                        ev.slug = slugify(&ev.name);
                    }
                }
            }
            pages_fetched += 1;
            tracing::info!(page, total = projects.len(), new, "indexed page");
            page += 1;
        }

        Ok(IndexResult {
            projects: projects.into_values().collect(),
            events: events.into_values().collect(),
            pages: pages_fetched,
        })
    }

    /// Fetch detail pages for projects missing description/how-it's-made.
    ///
    /// Merges richer detail-page fields into `projects` in place.
    /// Returns the number of detail pages fetched.
    pub async fn scrape_details(
        &self,
        projects: &mut [Project],
        force: bool,
        fresh: bool,
    ) -> Result<usize> {
        let pending: Vec<(usize, String, String)> = projects
            .iter()
            .enumerate()
            .filter(|(_, p)| force || !p.detail_scraped)
            .map(|(i, p)| (i, p.slug.clone(), p.id.clone()))
            .collect();

        let total = pending.len();
        if total == 0 {
            return Ok(0);
        }
        tracing::info!(total, "fetching detail pages");

        let now = chrono::Utc::now().to_rfc3339();
        let results = stream::iter(pending)
            .map(|(idx, slug, id)| async move {
                let html = self.project_page(&slug, &id, fresh).await?;
                Ok::<_, crate::error::Error>((idx, html))
            })
            .buffer_unordered(32)
            .collect::<Vec<_>>()
            .await;

        let mut fetched = 0usize;
        let mut failed = 0usize;
        for result in results {
            let (idx, html) = match result {
                Ok(r) => r,
                Err(e) => {
                    failed += 1;
                    tracing::warn!(error = %e, "detail fetch failed (project kept without details)");
                    continue;
                }
            };
            let doc = FlightDoc::parse(&crate::flight::extract_stream(&html))?;
            let found = parse::extract_projects(&doc, &now);
            if let Some(detail) = found.iter().find(|p| p.id == projects[idx].id) {
                merge_detail(&mut projects[idx], detail.clone());
                fetched += 1;
            } else {
                tracing::warn!(slug = %projects[idx].slug, "detail page yielded no matching project object");
                failed += 1;
            }
        }
        tracing::info!(fetched, failed, "detail pass complete");
        Ok(fetched)
    }
}

/// Overlay detail-page data onto an index record.
pub fn merge_detail(base: &mut Project, detail: Project) {
    if detail.description.is_some() {
        base.description = detail.description;
    }
    if detail.how_its_made.is_some() {
        base.how_its_made = detail.how_its_made;
    }
    if !detail.prizes.is_empty() {
        base.prizes = detail.prizes;
        base.finalist = detail.finalist || base.finalist;
    }
    if detail.demo_url.is_some() {
        base.demo_url = detail.demo_url;
    }
    if detail.source_url.is_some() {
        base.source_url = detail.source_url;
    }
    if detail.video_url.is_some() {
        base.video_url = detail.video_url;
    }
    if detail.youtube_id.is_some() {
        base.youtube_id = detail.youtube_id;
    }
    if !detail.screenshots.is_empty() {
        base.screenshots = detail.screenshots;
    }
    base.detail_scraped = true;
    base.scraped_at = detail.scraped_at;
}

/// Slugify an event name as a last-resort fallback when the canonical slug is
/// not available (e.g. "ETH Online 2025" -> "ethonline2025").
pub fn slugify(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Convenience constructor used by the CLI.
pub fn default_scraper(
    concurrency: usize,
    interval_ms: u64,
    cache_root: impl Into<std::path::PathBuf>,
) -> Result<Scraper> {
    let http = HttpClient::new(concurrency, Duration::from_millis(interval_ms))?;
    Ok(Scraper::new(http, HtmlCache::new(cache_root)))
}
