//! Static site generation: a zero-dependency searchable dataset site.

use std::path::Path;

use serde::Serialize;

use crate::error::Result;
use crate::model::{Event, Project, Stats};
use crate::store::Db;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const APP_JS: &str = include_str!("../assets/app.js");
const STYLE_CSS: &str = include_str!("../assets/style.css");
// Without this, Cloudflare Pages' SPA fallback serves index.html with HTTP 200
// for any missing path (e.g. removed projects' data/p/*.json) — GitHub Pages
// 404s correctly, so the mirrors would disagree. A 404.html disables the
// fallback and doubles as GitHub Pages' custom error page.
const NOT_FOUND_HTML: &str = include_str!("../assets/404.html");

/// Compact per-project record for client-side search.
#[derive(Debug, Serialize)]
struct SearchRecord {
    /// id
    i: String,
    /// slug
    s: String,
    /// title
    t: String,
    /// tagline
    g: String,
    /// event slug
    e: String,
    /// year (from event start)
    d: String,
    /// finalist
    f: bool,
    /// prize placements (compact)
    z: Vec<String>,
    /// precomputed lowercase search text
    h: String,
}

/// Generate the static site into `out_dir` from the dataset in `db`.
pub fn generate(db: &Db, out_dir: impl AsRef<Path>) -> Result<()> {
    let out = out_dir.as_ref();
    // Start clean so projects removed from the snapshot (e.g. by past-only
    // filtering) don't linger as stale detail files on the deployed site.
    let _ = std::fs::remove_dir_all(out.join("data/p"));
    std::fs::create_dir_all(out.join("data/p"))?;

    let projects = db.all_projects()?;
    let events = db.list_events()?;
    let stats = db.stats()?;

    std::fs::write(out.join("index.html"), INDEX_HTML)?;
    std::fs::write(out.join("app.js"), APP_JS)?;
    std::fs::write(out.join("style.css"), STYLE_CSS)?;
    std::fs::write(out.join("404.html"), NOT_FOUND_HTML)?;

    let mut records = Vec::with_capacity(projects.len());
    for p in &projects {
        write_pretty_json(out.join("data/p").join(format!("{}.json", p.id)), p)?;
        let year = p
            .event
            .as_ref()
            .and_then(|e| e.start_time.as_deref())
            .and_then(|s| s.get(0..4))
            .unwrap_or("")
            .to_string();
        let hay = format!(
            "{} {} {} {}",
            p.title,
            p.tagline.as_deref().unwrap_or(""),
            p.auto_summary.as_deref().unwrap_or(""),
            p.event.as_ref().map(|e| e.name.as_str()).unwrap_or("")
        )
        .to_lowercase();
        records.push(SearchRecord {
            i: p.id.clone(),
            s: p.slug.clone(),
            t: p.title.clone(),
            g: p.tagline.clone().unwrap_or_default(),
            e: p.event.as_ref().map(|e| e.slug.clone()).unwrap_or_default(),
            d: year,
            f: p.finalist,
            z: p.prizes
                .iter()
                .filter_map(|z| z.placement.clone().or_else(|| z.sponsor.clone()))
                .collect(),
            h: hay,
        });
    }

    write_pretty_json(out.join("data/stats.json"), &stats)?;
    write_pretty_json(out.join("data/events.json"), &events)?;
    write_compact_json(out.join("data/search.json"), &records)?;

    tracing::info!(
        out = %out.display(),
        projects = projects.len(),
        "static site generated"
    );
    Ok(())
}

/// Render the site dataset without a database (from plain vectors).
/// `cutoff` records the past-only build rule applied by the caller, if any.
pub fn generate_from(
    out_dir: impl AsRef<Path>,
    projects: &[Project],
    events: &[Event],
    cutoff: Option<String>,
) -> Result<Stats> {
    let out = out_dir.as_ref();
    // Start clean so projects removed from the snapshot (e.g. by past-only
    // filtering) don't linger as stale detail files on the deployed site.
    let _ = std::fs::remove_dir_all(out.join("data/p"));
    std::fs::create_dir_all(out.join("data/p"))?;
    std::fs::write(out.join("index.html"), INDEX_HTML)?;
    std::fs::write(out.join("app.js"), APP_JS)?;
    std::fs::write(out.join("style.css"), STYLE_CSS)?;
    std::fs::write(out.join("404.html"), NOT_FOUND_HTML)?;

    let finalists = projects.iter().filter(|p| p.finalist).count();
    let with_description = projects.iter().filter(|p| p.description.is_some()).count();
    let as_of = projects
        .iter()
        .map(|p| p.scraped_at.as_str())
        .max()
        .map(str::to_string);
    let stats = Stats {
        projects: projects.len() as u64,
        events: events.len() as u64,
        finalists: finalists as u64,
        with_description: with_description as u64,
        generated_at: chrono::Utc::now().to_rfc3339(),
        as_of,
        cutoff,
    };

    let mut records = Vec::with_capacity(projects.len());
    for p in projects {
        write_pretty_json(out.join("data/p").join(format!("{}.json", p.id)), p)?;
        let year = p
            .event
            .as_ref()
            .and_then(|e| e.start_time.as_deref())
            .and_then(|s| s.get(0..4))
            .unwrap_or("")
            .to_string();
        let hay = format!(
            "{} {} {} {}",
            p.title,
            p.tagline.as_deref().unwrap_or(""),
            p.auto_summary.as_deref().unwrap_or(""),
            p.event.as_ref().map(|e| e.name.as_str()).unwrap_or("")
        )
        .to_lowercase();
        records.push(SearchRecord {
            i: p.id.clone(),
            s: p.slug.clone(),
            t: p.title.clone(),
            g: p.tagline.clone().unwrap_or_default(),
            e: p.event.as_ref().map(|e| e.slug.clone()).unwrap_or_default(),
            d: year,
            f: p.finalist,
            z: p.prizes
                .iter()
                .filter_map(|z| z.placement.clone().or_else(|| z.sponsor.clone()))
                .collect(),
            h: hay,
        });
    }

    write_pretty_json(out.join("data/stats.json"), &stats)?;
    write_pretty_json(out.join("data/events.json"), &events)?;
    write_compact_json(out.join("data/search.json"), &records)?;
    Ok(stats)
}

fn write_pretty_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    serde_json::to_writer_pretty(std::io::BufWriter::new(std::fs::File::create(path)?), value)?;
    Ok(())
}

fn write_compact_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    serde_json::to_writer(std::io::BufWriter::new(std::fs::File::create(path)?), value)?;
    Ok(())
}
