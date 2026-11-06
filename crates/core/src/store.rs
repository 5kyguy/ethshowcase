//! Dataset storage: JSONL snapshots and a SQLite database with FTS5 search.

use std::io::{BufRead, BufWriter, Write};
use std::path::Path;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::{Event, Project, Stats};

// ---------------------------------------------------------------------------
// JSONL snapshots
// ---------------------------------------------------------------------------

/// Write projects as JSON Lines (one object per line).
pub fn write_jsonl(path: impl AsRef<Path>, projects: &[Project]) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    let mut w = BufWriter::new(file);
    for p in projects {
        serde_json::to_writer(&mut w, p)?;
        w.write_all(b"\n")?;
    }
    w.flush()?;
    Ok(())
}

/// Read a JSONL project snapshot.
pub fn read_jsonl(path: impl AsRef<Path>) -> Result<Vec<Project>> {
    let file = std::fs::File::open(path)?;
    let mut out = Vec::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        out.push(serde_json::from_str(&line)?);
    }
    Ok(out)
}

/// Merge `overlay` into `base` by project id, with overlay fields winning when
/// set. Useful to combine an older full snapshot with a newer index pass.
pub fn merge_projects(base: Vec<Project>, overlay: Vec<Project>) -> Vec<Project> {
    let mut by_id: std::collections::HashMap<String, Project> =
        base.into_iter().map(|p| (p.id.clone(), p)).collect();
    for p in overlay {
        match by_id.get_mut(&p.id) {
            Some(existing) => {
                if p.detail_scraped || p.scraped_at >= existing.scraped_at {
                    let description = existing.description.take();
                    let how = existing.how_its_made.take();
                    *existing = p;
                    if existing.description.is_none() {
                        existing.description = description;
                    }
                    if existing.how_its_made.is_none() {
                        existing.how_its_made = how;
                    }
                }
            }
            None => {
                by_id.insert(p.id.clone(), p);
            }
        }
    }
    let mut out: Vec<Project> = by_id.into_values().collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Write events as a JSON array.
pub fn write_events(path: impl AsRef<Path>, events: &[Event]) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut sorted: Vec<&Event> = events.iter().collect();
    sorted.sort_by(|a, b| b.start_time.cmp(&a.start_time));
    serde_json::to_writer_pretty(
        std::io::BufWriter::new(std::fs::File::create(path)?),
        &sorted,
    )?;
    Ok(())
}

/// Read events from a JSON array file.
pub fn read_events(path: impl AsRef<Path>) -> Result<Vec<Event>> {
    let file = std::fs::File::open(path)?;
    Ok(serde_json::from_reader(file)?)
}

// ---------------------------------------------------------------------------
// SQLite + FTS5
// ---------------------------------------------------------------------------

/// A read/write dataset database.
///
/// The SQLite connection is wrapped in a mutex so a single `Db` handle is
/// `Send + Sync` and can be shared as axum state.
pub struct Db {
    conn: std::sync::Mutex<Connection>,
}

/// A search hit with its relevance rank.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub project: Project,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<f64>,
}

/// Query parameters for project search.
#[derive(Debug, Clone, Default)]
pub struct ProjectQuery {
    pub q: Option<String>,
    pub event: Option<String>,
    pub year: Option<i64>,
    pub finalist: Option<bool>,
    pub prize: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

impl Db {
    /// Open (creating) the database and ensure the schema exists.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::with_connection(conn)
    }

    /// In-memory database (tests, throwaway queries).
    pub fn in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    fn with_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch(            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id             TEXT PRIMARY KEY,
                slug           TEXT NOT NULL,
                title          TEXT NOT NULL,
                tagline        TEXT,
                event_slug     TEXT,
                event_name     TEXT,
                event_start    TEXT,
                finalist       INTEGER NOT NULL DEFAULT 0,
                prizes_json    TEXT NOT NULL DEFAULT '[]',
                demo_url       TEXT,
                source_url     TEXT,
                description    TEXT,
                how_its_made   TEXT,
                auto_summary   TEXT,
                data_json      TEXT NOT NULL,
                detail_scraped INTEGER NOT NULL DEFAULT 0,
                scraped_at     TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_projects_event    ON projects(event_slug);
            CREATE INDEX IF NOT EXISTS idx_projects_finalist ON projects(finalist);
            CREATE INDEX IF NOT EXISTS idx_projects_year     ON projects(event_start);

            CREATE TABLE IF NOT EXISTS events (
                slug       TEXT PRIMARY KEY,
                id         TEXT,
                name       TEXT NOT NULL,
                kind       TEXT,
                start_time TEXT,
                end_time   TEXT,
                status     TEXT
            );

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS projects_fts USING fts5(
                title, tagline, description, how_its_made, auto_summary,
                content='projects', content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS projects_ai AFTER INSERT ON projects BEGIN
                INSERT INTO projects_fts(rowid, title, tagline, description, how_its_made, auto_summary)
                VALUES (new.rowid, new.title, new.tagline, new.description, new.how_its_made, new.auto_summary);
            END;
            CREATE TRIGGER IF NOT EXISTS projects_ad AFTER DELETE ON projects BEGIN
                INSERT INTO projects_fts(projects_fts, rowid, title, tagline, description, how_its_made, auto_summary)
                VALUES ('delete', old.rowid, old.title, old.tagline, old.description, old.how_its_made, old.auto_summary);
            END;
            CREATE TRIGGER IF NOT EXISTS projects_au AFTER UPDATE ON projects BEGIN
                INSERT INTO projects_fts(projects_fts, rowid, title, tagline, description, how_its_made, auto_summary)
                VALUES ('delete', old.rowid, old.title, old.tagline, old.description, old.how_its_made, old.auto_summary);
                INSERT INTO projects_fts(rowid, title, tagline, description, how_its_made, auto_summary)
                VALUES (new.rowid, new.title, new.tagline, new.description, new.how_its_made, new.auto_summary);
            END;
            "#,
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    /// Acquire the connection lock. Panics only if a previous panic poisoned
    /// the mutex mid-transaction, which we treat as fatal.
    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("database lock poisoned")
    }

    /// Insert or replace a batch of projects in one transaction.
    pub fn upsert_projects(&self, projects: &[Project]) -> Result<()> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            r#"
            INSERT INTO projects (
                id, slug, title, tagline, event_slug, event_name, event_start,
                finalist, prizes_json, demo_url, source_url,
                description, how_its_made, auto_summary,
                data_json, detail_scraped, scraped_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(id) DO UPDATE SET
                slug = excluded.slug,
                title = excluded.title,
                tagline = excluded.tagline,
                event_slug = excluded.event_slug,
                event_name = excluded.event_name,
                event_start = excluded.event_start,
                finalist = excluded.finalist,
                prizes_json = excluded.prizes_json,
                demo_url = excluded.demo_url,
                source_url = excluded.source_url,
                description = COALESCE(excluded.description, projects.description),
                how_its_made = COALESCE(excluded.how_its_made, projects.how_its_made),
                auto_summary = excluded.auto_summary,
                data_json = excluded.data_json,
                detail_scraped = excluded.detail_scraped,
                scraped_at = excluded.scraped_at
            "#,
        )?;
        conn.execute_batch("BEGIN")?;
        for p in projects {
            let prizes_json = serde_json::to_string(&p.prizes)?;
            stmt.execute(params![
                p.id,
                p.slug,
                p.title,
                p.tagline,
                p.event.as_ref().map(|e| e.slug.clone()),
                p.event.as_ref().map(|e| e.name.clone()),
                p.event.as_ref().and_then(|e| e.start_time.clone()),
                p.finalist as i64,
                prizes_json,
                p.demo_url,
                p.source_url,
                p.description,
                p.how_its_made,
                p.auto_summary,
                serde_json::to_string(p)?,
                p.detail_scraped as i64,
                p.scraped_at,
            ])?;
        }
        conn.execute_batch("COMMIT")?;
        Ok(())
    }

    /// Insert or replace events.
    pub fn upsert_events(&self, events: &[Event]) -> Result<()> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            r#"
            INSERT INTO events (slug, id, name, kind, start_time, end_time, status)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(slug) DO UPDATE SET
                id = excluded.id,
                name = excluded.name,
                kind = excluded.kind,
                start_time = excluded.start_time,
                end_time = excluded.end_time,
                status = excluded.status
            "#,
        )?;
        conn.execute_batch("BEGIN")?;
        for e in events {
            stmt.execute(params![
                e.slug,
                e.id,
                e.name,
                e.kind,
                e.start_time,
                e.end_time,
                e.status
            ])?;
        }
        conn.execute_batch("COMMIT")?;
        Ok(())
    }

    fn row_to_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
        let data_json: String = row.get("data_json")?;
        serde_json::from_str(&data_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    }

    /// Query projects. When `q` is set, FTS5 ranking applies.
    pub fn query_projects(&self, query: &ProjectQuery) -> Result<(Vec<Project>, u64)> {
        let conn = self.lock();
        let mut where_clauses: Vec<String> = Vec::new();
        if query.event.is_some() {
            where_clauses.push("p.event_slug = :event".into());
        }
        if query.year.is_some() {
            where_clauses.push("CAST(strftime('%Y', p.event_start) AS INTEGER) = :year".into());
        }
        if let Some(f) = query.finalist {
            where_clauses.push(format!("p.finalist = {}", f as i64));
        }
        if query.prize.is_some() {
            where_clauses.push(
                "EXISTS (SELECT 1 FROM json_each(p.prizes_json) je
                  WHERE LOWER(COALESCE(json_extract(je.value, '$.placement'), ''))
                     LIKE '%' || LOWER(:prize) || '%'
                     OR LOWER(COALESCE(json_extract(je.value, '$.sponsor'), ''))
                     LIKE '%' || LOWER(:prize) || '%')"
                    .into(),
            );
        }

        let limit = query.limit.clamp(1, 500);
        let offset = query.offset;

        // Dynamic named-parameter binding: only bind names present in each SQL.
        let mut bind: Vec<(&str, &dyn rusqlite::ToSql)> = Vec::new();
        if let Some(e) = query.event.as_ref() {
            bind.push((":event", e));
        }
        if let Some(y) = query.year.as_ref() {
            bind.push((":year", y));
        }
        if let Some(pr) = query.prize.as_ref() {
            bind.push((":prize", pr));
        }

        let suffix = where_clauses
            .iter()
            .map(|c| format!(" AND {c}"))
            .collect::<String>();

        if let Some(q) = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let fts_q = fts_query(q);
            let mut count_bind = bind.clone();
            count_bind.push((":q", &fts_q));
            let mut select_bind = count_bind.clone();
            select_bind.push((":limit", &limit));
            select_bind.push((":offset", &offset));

            let select_sql = format!(
                "SELECT p.* FROM projects_fts f JOIN projects p ON p.rowid = f.rowid \
                 WHERE projects_fts MATCH :q{suffix} \
                 ORDER BY rank, p.id LIMIT :limit OFFSET :offset"
            );
            let count_sql = format!(
                "SELECT COUNT(*) FROM projects_fts f JOIN projects p ON p.rowid = f.rowid \
                 WHERE projects_fts MATCH :q{suffix}"
            );
            let total: u64 = conn
                .prepare(&count_sql)?
                .query_row(count_bind.as_slice(), |r| r.get(0))?;
            let projects = conn
                .prepare(&select_sql)?
                .query_map(select_bind.as_slice(), Self::row_to_project)?
                .filter_map(|r| r.ok())
                .collect();
            return Ok((projects, total));
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };
        let total: u64 = conn
            .prepare(&format!("SELECT COUNT(*) FROM projects p{where_sql}"))?
            .query_row(bind.as_slice(), |r| r.get(0))?;
        let mut select_bind = bind;
        select_bind.push((":limit", &limit));
        select_bind.push((":offset", &offset));
        let projects = conn
            .prepare(&format!(
                "SELECT p.* FROM projects p{where_sql} \
                 ORDER BY p.event_start DESC, p.id LIMIT :limit OFFSET :offset"
            ))?
            .query_map(select_bind.as_slice(), Self::row_to_project)?
            .filter_map(|r| r.ok())
            .collect();
        Ok((projects, total))
    }

    /// Fetch one project by id or slug.
    pub fn get_project(&self, id_or_slug: &str) -> Result<Option<Project>> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT p.* FROM projects p WHERE p.id = ?1 OR p.slug || '-' || p.id = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![id_or_slug])?;
        match rows.next()? {
            Some(row) => Ok(Some(Self::row_to_project(row)?)),
            None => Ok(None),
        }
    }

    /// All events ordered by date (newest first).
    pub fn list_events(&self) -> Result<Vec<Event>> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached("SELECT * FROM events ORDER BY start_time DESC")?;
        let events = stmt
            .query_map([], |r| {
                Ok(Event {
                    id: r.get::<_, Option<String>>("id")?.unwrap_or_default(),
                    name: r.get("name")?,
                    slug: r.get("slug")?,
                    kind: r.get("kind")?,
                    start_time: r.get("start_time")?,
                    end_time: r.get("end_time")?,
                    status: r.get("status")?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(events)
    }

    /// Dataset statistics.
    pub fn stats(&self) -> Result<Stats> {
        let conn = self.lock();
        let one =
            |sql: &str| -> Result<u64> { Ok(conn.prepare(sql)?.query_row([], |r| r.get(0))?) };
        let as_of: Option<String> = conn
            .prepare("SELECT MAX(scraped_at) FROM projects")?
            .query_row([], |r| r.get(0))?;
        let cutoff: Option<String> = conn
            .prepare("SELECT value FROM meta WHERE key = 'cutoff'")?
            .query_row([], |r| r.get(0))
            .ok();
        Ok(Stats {
            projects: one("SELECT COUNT(*) FROM projects")?,
            events: one("SELECT COUNT(*) FROM events")?,
            finalists: one("SELECT COUNT(*) FROM projects WHERE finalist = 1")?,
            with_description: one("SELECT COUNT(*) FROM projects WHERE description IS NOT NULL")?,
            generated_at: chrono::Utc::now().to_rfc3339(),
            as_of,
            cutoff,
        })
    }

    /// Record a build-time metadata value (e.g. the past-only cutoff).
    pub fn set_meta(&self, key: &str, value: Option<&str>) -> Result<()> {
        let conn = self.lock();
        match value {
            Some(v) => {
                conn.execute(
                    "INSERT INTO meta(key, value) VALUES(?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [key, v],
                )?;
            }
            None => {
                conn.execute("DELETE FROM meta WHERE key = ?1", [key])?;
            }
        }
        Ok(())
    }

    /// Delete events whose slug is not in `keep` (stale rows from earlier
    /// builds). Returns rows removed.
    pub fn prune_events_to_slugs(&self, keep: &[String]) -> Result<u64> {
        if keep.is_empty() {
            return Ok(0);
        }
        let conn = self.lock();
        let slugs: Vec<String> = keep.iter().map(|s| format!("'{s}'")).collect();
        let n = conn.execute(
            &format!("DELETE FROM events WHERE slug NOT IN ({})", slugs.join(",")),
            [],
        )?;
        Ok(n as u64)
    }

    /// Delete projects whose event slug is not in `allowed` (stale rows from
    /// builds that once included now-excluded events). Returns rows removed.
    pub fn prune_projects_to_events(&self, allowed: &[String]) -> Result<u64> {
        if allowed.is_empty() {
            // Never wipe the table on an empty allowlist (bad events file).
            return Ok(0);
        }
        let conn = self.lock();
        let slugs: Vec<String> = allowed.iter().map(|s| format!("'{s}'")).collect();
        let n = conn.execute(
            &format!(
                "DELETE FROM projects WHERE event_slug NOT IN ({})",
                slugs.join(",")
            ),
            [],
        )?;
        Ok(n as u64)
    }

    /// Every project (used by the static site generator).
    pub fn all_projects(&self) -> Result<Vec<Project>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare_cached("SELECT p.* FROM projects p ORDER BY p.event_start DESC, p.id")?;
        let projects = stmt
            .query_map([], Self::row_to_project)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(projects)
    }
}

// Named-parameter helpers ---------------------------------------------------

/// Build an FTS5 MATCH expression: quote each term so user input can't inject
/// query syntax, and AND the terms together.
fn fts_query(q: &str) -> String {
    q.split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EventRef;

    fn sample(id: &str, title: &str, desc: Option<&str>) -> Project {
        Project {
            id: id.into(),
            slug: title.to_lowercase().replace(' ', "-"),
            title: title.into(),
            tagline: Some("a tagline".into()),
            event: Some(EventRef {
                slug: "ethonline2025".into(),
                name: "ETHOnline 2025".into(),
                start_time: Some("2025-09-01T00:00:00Z".into()),
            }),
            banner_url: None,
            logo_url: None,
            video_url: None,
            youtube_id: None,
            screenshots: vec![],
            demo_url: None,
            source_url: None,
            prizes: vec![],
            finalist: false,
            description: desc.map(String::from),
            how_its_made: None,
            auto_summary: None,
            scores: None,
            showcase_url: Project::showcase_url(&title.to_lowercase().replace(' ', "-"), id),
            scraped_at: "2026-10-31T00:00:00Z".into(),
            detail_scraped: desc.is_some(),
        }
    }

    #[test]
    fn upsert_and_search() {
        let db = Db::in_memory().unwrap();
        let projects = vec![
            sample("aa1", "Alpha Net", Some("onchain latency attestation CDN")),
            sample(
                "bb2",
                "Brick Finance",
                Some("RWA launchpad with Uniswap v4 hook"),
            ),
        ];
        db.upsert_projects(&projects).unwrap();
        db.upsert_events(&[Event {
            id: "1".into(),
            name: "ETHOnline 2025".into(),
            slug: "ethonline2025".into(),
            kind: Some("hackathon".into()),
            start_time: Some("2025-09-01T00:00:00Z".into()),
            end_time: None,
            status: None,
        }])
        .unwrap();

        let (hits, total) = db
            .query_projects(&ProjectQuery {
                q: Some("latency".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].title, "Alpha Net");

        let (hits, total) = db
            .query_projects(&ProjectQuery {
                event: Some("ethonline2025".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(hits.len(), 2);

        let (hits, total) = db
            .query_projects(&ProjectQuery {
                year: Some(2025),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(total, 2, "year filter should match all 2025 projects");
        assert_eq!(hits.len(), 2);

        let (_, total) = db
            .query_projects(&ProjectQuery {
                year: Some(2024),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(total, 0, "no 2024 projects in the sample data");

        let got = db.get_project("alpha-net-aa1").unwrap().unwrap();
        assert_eq!(got.id, "aa1");
    }

    #[test]
    fn jsonl_roundtrip() {
        let dir = std::env::temp_dir().join(format!("forklore-store-{}", std::process::id()));
        let path = dir.join("p.jsonl");
        let projects = vec![sample("aa1", "Alpha Net", None)];
        write_jsonl(&path, &projects).unwrap();
        let read = read_jsonl(&path).unwrap();
        assert_eq!(read, projects);
        std::fs::remove_dir_all(&dir).ok();
    }
}
