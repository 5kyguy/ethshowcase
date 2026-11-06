use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ethshowcase_core::{Db, IndexOptions, default_scraper};

/// Scrape, store, and serve the ETHGlobal Showcase project database.
#[derive(Parser, Debug)]
#[command(name = "ethshowcase", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Walk showcase listing pages and extract every project's record.
    Index {
        /// Output JSONL path.
        #[arg(long, default_value = "data/projects.index.jsonl")]
        out: PathBuf,
        /// Events output path.
        #[arg(long, default_value = "data/events.json")]
        events_out: PathBuf,
        /// Max concurrent requests.
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
        /// Minimum ms between request starts (global pacing).
        #[arg(long, default_value_t = 350)]
        interval_ms: u64,
        /// Stop after this many listing pages (for testing).
        #[arg(long)]
        max_pages: Option<u32>,
        /// Ignore cached listing pages and refetch.
        #[arg(long)]
        fresh: bool,
        /// Cache directory for raw HTML.
        #[arg(long, default_value = "data/cache")]
        cache: PathBuf,
    },
    /// Fetch detail pages (description / how-it's-made) and merge into the
    /// index snapshot.
    Details {
        /// Index JSONL input (as written by `index`).
        #[arg(long, default_value = "data/projects.index.jsonl")]
        input: PathBuf,
        /// Output JSONL path.
        #[arg(long, default_value = "data/projects.jsonl")]
        out: PathBuf,
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
        #[arg(long, default_value_t = 350)]
        interval_ms: u64,
        /// Refetch detail pages even if cached/marked scraped.
        #[arg(long)]
        force: bool,
        /// Ignore cached detail pages and refetch.
        #[arg(long)]
        fresh: bool,
        #[arg(long, default_value = "data/cache")]
        cache: PathBuf,
    },
    /// Build the SQLite database (and optionally the static site) from a
    /// projects JSONL snapshot.
    Build {
        #[arg(long, default_value = "data/projects.jsonl")]
        projects: PathBuf,
        #[arg(long, default_value = "data/events.json")]
        events: PathBuf,
        /// Output SQLite database path.
        #[arg(long, default_value = "data/ethshowcase.db")]
        db: PathBuf,
        /// Also generate the static site into this directory.
        #[arg(long)]
        site: Option<PathBuf>,
        /// Also write the built (past-only filtered) project snapshot to this
        /// path, and the built event list next to it (events.built.json in the
        /// same directory). Dataset releases ship these files, not the raw
        /// scrape.
        #[arg(long)]
        emit_projects: Option<PathBuf>,
        /// Include events that have not finished yet (default: past
        /// hackathons only — cancelled and still-running events are dropped).
        #[arg(long)]
        include_live: bool,
    },
    /// Serve the queryable HTTP API over a built database.
    Serve {
        #[arg(long, default_value = "data/ethshowcase.db")]
        db: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Dump dataset statistics from a built database.
    Stats {
        #[arg(long, default_value = "data/ethshowcase.db")]
        db: PathBuf,
    },
    /// Debug: parse a saved showcase HTML file and print extracted records.
    Inspect {
        /// Path to a saved HTML file (listing or project page).
        path: PathBuf,
        /// Print resolved project objects (large).
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    match Cli::parse().command {
        Command::Index {
            out,
            events_out,
            concurrency,
            interval_ms,
            max_pages,
            fresh,
            cache,
        } => {
            let scraper = default_scraper(concurrency, interval_ms, &cache)?;
            let opts = IndexOptions {
                start_page: 1,
                max_pages,
                fresh,
            };
            let result = scraper.scrape_index(&opts).await?;
            let dir = out.parent().map(PathBuf::from).unwrap_or_default();
            std::fs::create_dir_all(&dir).ok();
            ethshowcase_core::store::write_jsonl(&out, &result.projects)?;
            ethshowcase_core::store::write_events(&events_out, &result.events)?;
            println!(
                "indexed {} projects across {} listing pages; {} events → {}",
                result.projects.len(),
                result.pages,
                result.events.len(),
                out.display()
            );
        }
        Command::Details {
            input,
            out,
            concurrency,
            interval_ms,
            force,
            fresh,
            cache,
        } => {
            let mut projects = ethshowcase_core::store::read_jsonl(&input)
                .with_context(|| format!("reading {}", input.display()))?;
            println!(
                "loaded {} projects from {}",
                projects.len(),
                input.display()
            );
            let scraper = default_scraper(concurrency, interval_ms, &cache)?;
            let fetched = scraper.scrape_details(&mut projects, force, fresh).await?;
            ethshowcase_core::store::write_jsonl(&out, &projects)?;
            println!("fetched {} detail pages → {}", fetched, out.display());
        }
        Command::Build {
            projects,
            events,
            db,
            site,
            emit_projects,
            include_live,
        } => {
            let mut all = ethshowcase_core::store::read_jsonl(&projects)?;
            all.sort_by(|a, b| a.id.cmp(&b.id));
            let event_list = ethshowcase_core::store::read_events(&events).unwrap_or_default();
            let now = chrono::Utc::now();
            let (all, event_list, cutoff) = if include_live {
                (all, event_list, None)
            } else {
                let cutoff = now.to_rfc3339();
                let known: std::collections::HashSet<&str> =
                    event_list.iter().map(|e| e.slug.as_str()).collect();
                let kept_events: Vec<_> = event_list
                    .iter()
                    .filter(|e| e.is_past(now))
                    .cloned()
                    .collect();
                let excluded: Vec<&str> = event_list
                    .iter()
                    .filter(|e| !e.is_past(now))
                    .map(|e| e.slug.as_str())
                    .collect();
                let allowed: std::collections::HashSet<&str> =
                    kept_events.iter().map(|e| e.slug.as_str()).collect();
                let mut unknown_events: Vec<String> = Vec::new();
                let mut dropped_live = 0usize;
                let mut dropped_unknown = 0usize;
                all.retain(|p| match p.event.as_ref() {
                    Some(ev) if allowed.contains(ev.slug.as_str()) => true,
                    Some(ev) => {
                        if known.contains(ev.slug.as_str()) {
                            dropped_live += 1;
                        } else {
                            if !unknown_events.iter().any(|s| s == &ev.slug) {
                                unknown_events.push(ev.slug.clone());
                            }
                            dropped_unknown += 1;
                        }
                        false
                    }
                    None => false,
                });
                println!("past-only cutoff {cutoff}");
                if !excluded.is_empty() {
                    println!("  excluded non-finished event(s): {}", excluded.join(", "));
                }
                if !unknown_events.is_empty() {
                    // Slugs that only exist on project pages (never in the
                    // events index) — the currently-running hackathon's tracks.
                    println!(
                        "  excluded live track event(s): {} ({} projects)",
                        unknown_events.join(", "),
                        dropped_unknown
                    );
                }
                if dropped_live > 0 {
                    println!("  dropped {dropped_live} project(s) in non-finished events");
                }
                (all, kept_events, Some(cutoff))
            };
            let database = Db::open(&db)?;
            let allowed_slugs: Vec<String> = event_list.iter().map(|e| e.slug.clone()).collect();
            let pruned = database.prune_projects_to_events(&allowed_slugs)?;
            if pruned > 0 {
                println!("pruned {pruned} stale project rows outside the event set");
            }
            let pruned_events = database.prune_events_to_slugs(&allowed_slugs)?;
            if pruned_events > 0 {
                println!("pruned {pruned_events} stale event rows");
            }
            database.upsert_projects(&all)?;
            database.upsert_events(&event_list)?;
            database.set_meta("cutoff", cutoff.as_deref())?;
            if let Some(out) = emit_projects {
                ethshowcase_core::store::write_jsonl(&out, &all)?;
                let events_out = out
                    .parent()
                    .map(|d| d.join("events.built.json"))
                    .unwrap_or_else(|| PathBuf::from("events.built.json"));
                ethshowcase_core::store::write_events(&events_out, &event_list)?;
                println!(
                    "wrote built snapshot → {} (+ {})",
                    out.display(),
                    events_out.display()
                );
            }
            println!(
                "built {} ({}) with {} projects, {} events",
                db.display(),
                fmt_size(&db),
                all.len(),
                event_list.len()
            );
            if let Some(site_dir) = site {
                let stats =
                    ethshowcase_core::site::generate_from(&site_dir, &all, &event_list, cutoff)?;
                println!(
                    "site generated at {} ({} projects, {} finalists)",
                    site_dir.display(),
                    stats.projects,
                    stats.finalists
                );
            }
        }
        Command::Serve { db, host, port } => {
            let database = Db::open(&db)?;
            let addr = std::net::SocketAddr::new(
                host.parse()
                    .context("invalid host (expected an IP address)")?,
                port,
            );
            ethshowcase_core::serve::serve(database, addr).await?;
        }
        Command::Stats { db } => {
            let database = Db::open(&db)?;
            let stats = database.stats()?;
            println!(
                "projects: {}\nevents: {}\nfinalists: {}\nwith description: {}\ngenerated: {}",
                stats.projects,
                stats.events,
                stats.finalists,
                stats.with_description,
                stats.generated_at
            );
            if let Some(as_of) = stats.as_of {
                println!("data as of: {as_of}");
            }
            match stats.cutoff {
                Some(c) => println!("past-only cutoff: {c}"),
                None => println!("past-only cutoff: none (live events included)"),
            }
        }
        Command::Inspect { path, json } => {
            let html = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let doc = ethshowcase_core::flight::parse_html(&html)?;
            let projects = ethshowcase_core::parse::extract_projects(&doc, "inspect");
            let events = ethshowcase_core::parse::extract_events(&doc);
            let texts = ethshowcase_core::parse::text_rows(&doc);
            eprintln!(
                "{} flight rows, {} projects, {} events, {} text rows",
                doc.rows().len(),
                projects.len(),
                events.len(),
                texts.len()
            );
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            for p in &projects {
                if json {
                    serde_json::to_writer_pretty(&mut out, p)?;
                    out.write_all(b"\n")?;
                } else {
                    println!(
                        "{:>8}  {:<40} {:<28} finalist={} desc={} prizes={}",
                        p.id,
                        truncate(&p.title, 40),
                        p.event.as_ref().map(|e| e.name.as_str()).unwrap_or(""),
                        p.finalist,
                        p.description.as_ref().map(|d| d.len()).unwrap_or(0),
                        p.prizes.len(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

fn fmt_size(path: &std::path::Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return "?".into();
    };
    let bytes = meta.len();
    if bytes >= 1 << 20 {
        format!("{:.1} MiB", bytes as f64 / (1 << 20) as f64)
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    }
}
