//! Extraction of typed records from flight documents and HTML.

use serde_json::{Map, Value};

use crate::error::Result;
use crate::flight::{FlightDoc, FlightRow};
use crate::model::{Event, EventRef, Prize, Project, Scores};

/// Extract every project object from a flight document.
///
/// Project objects carry `uuid` + `slug` + `name` + `event`; deduped by id.
pub fn extract_projects(doc: &FlightDoc, now: &str) -> Vec<Project> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for v in doc.find(&is_project) {
        if let Some(p) = normalize_project(v, doc, now)
            && seen.insert(p.id.clone())
        {
            out.push(p);
        }
    }
    out
}

/// Extract every event object from a flight document.
pub fn extract_events(doc: &FlightDoc) -> Vec<Event> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for v in doc.find(&is_event) {
        let Some(obj) = v.as_object() else { continue };
        let id = obj.get("id").map(id_to_string).unwrap_or_default();
        let Some(name) = obj.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(slug) = obj.get("slug").and_then(Value::as_str) else {
            continue;
        };
        if !seen.insert(slug.to_string()) {
            continue;
        }
        out.push(Event {
            id,
            name: name.to_string(),
            slug: slug.to_string(),
            kind: obj.get("type").and_then(Value::as_str).map(str::to_string),
            start_time: obj
                .get("startTime")
                .and_then(Value::as_str)
                .map(str::to_string),
            end_time: obj
                .get("endTime")
                .and_then(Value::as_str)
                .map(str::to_string),
            status: obj
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    out
}

fn id_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn is_project(v: &Value) -> bool {
    v.get("uuid").and_then(Value::as_str).is_some()
        && v.get("slug").and_then(Value::as_str).is_some()
        && v.get("name").and_then(Value::as_str).is_some()
        && v.get("event").is_some()
}

fn is_event(v: &Value) -> bool {
    v.get("slug").and_then(Value::as_str).is_some()
        && v.get("name").and_then(Value::as_str).is_some()
        && v.get("startTime").and_then(Value::as_str).is_some()
        && v.get("uuid").is_none()
}

fn str_field<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    v.as_object().and_then(|m| map_field(m, keys))
}

fn map_field<'a>(m: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(s) = m.get(*k).and_then(Value::as_str)
            && !s.is_empty()
        {
            return Some(s);
        }
    }
    None
}

fn file_url(v: &Value, key: &str) -> Option<String> {
    v.get(key)?
        .get("file")?
        .get("fullUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn normalize_project(v: &Value, doc: &FlightDoc, now: &str) -> Option<Project> {
    let obj = v.as_object()?;
    let id = obj.get("uuid")?.as_str()?.to_string();
    let slug = obj.get("slug")?.as_str()?.to_string();
    let title = obj.get("name")?.as_str()?.to_string();
    let showcase_url = Project::showcase_url(&slug, &id);
    // Stable public CDN banner path (R2 presigned URLs expire after an hour).
    let banner_url = format!("https://cdn.ethglobal.com/projects/{id}/banner/default.jpg");

    // Resolve `$<row>` references (descriptions live in text rows).
    let resolved = doc.resolve(v);
    let robj = resolved.as_object();

    let event = robj.as_ref().and_then(|o| o.get("event")).and_then(|e| {
        let eo = e.as_object()?;
        let name = eo.get("name")?.as_str()?.to_string();
        // Listing pages only embed the name; slug/startTime arrive from the
        // events array (filled in by Scraper::scrape_index) or the detail page.
        let slug = eo
            .get("slug")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Some(EventRef {
            slug,
            name,
            start_time: eo
                .get("startTime")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    });

    let demo_url = robj
        .as_ref()
        .and_then(|o| map_field(o, &["url", "demoUrl", "demoLink", "liveUrl"]))
        .map(str::to_string);
    let source_url = robj
        .as_ref()
        .and_then(|o| map_field(o, &["sourceCodeUrl", "repoUrl", "repositoryUrl"]))
        .map(str::to_string)
        .or_else(|| {
            obj.get("primaryRepository")
                .and_then(|r| r.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    let prizes = robj
        .as_ref()
        .and_then(|o| o.get("prizes"))
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(normalize_prize).collect::<Vec<_>>())
        .unwrap_or_default();

    let finalist = prizes.iter().any(|p| {
        p.placement
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("finalist"))
    }) || obj
        .get("isFinalist")
        .or_else(|| obj.get("finalist"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let description = robj
        .as_ref()
        .and_then(|o| o.get("description"))
        .and_then(Value::as_str)
        // Skip non-text placeholder values that survived resolution.
        .filter(|s| !s.starts_with('$'))
        .map(str::to_string);
    let how_its_made = robj
        .as_ref()
        .and_then(|o| o.get("howItsMade"))
        .and_then(Value::as_str)
        .filter(|s| !s.starts_with('$'))
        .map(str::to_string);

    let auto_summary = obj
        .get("meta")
        .and_then(|m| m.get("autoSummary"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let scores = obj.get("meta").map(|m| Scores {
        originality: m
            .get("autoOriginality")
            .and_then(Value::as_u64)
            .map(|n| n as u8),
        practicality: m
            .get("autoPracticality")
            .and_then(Value::as_u64)
            .map(|n| n as u8),
        technicality: m
            .get("autoTechnicality")
            .and_then(Value::as_u64)
            .map(|n| n as u8),
    });

    let screenshots = obj
        .get("screenshots")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(|s| file_url(s, "file")).collect())
        .unwrap_or_default();

    let video_url = obj
        .get("video")
        .and_then(|v| v.get("muxUrl").and_then(Value::as_str).map(str::to_string));
    let youtube_id = obj.get("video").and_then(|v| {
        v.get("youtubeId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });

    Some(Project {
        id,
        slug: slug.clone(),
        title,
        tagline: obj
            .get("tagline")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        event,
        banner_url: Some(banner_url),
        logo_url: file_url(v, "logo"),
        video_url,
        youtube_id,
        screenshots,
        demo_url,
        source_url,
        prizes,
        finalist,
        description,
        how_its_made,
        auto_summary,
        scores,
        showcase_url,
        scraped_at: now.to_string(),
        detail_scraped: false,
    })
}

fn normalize_prize(v: &Value) -> Option<Prize> {
    // Entry shape (detail pages): {uuid, name: "3rd place", prize: {name,
    // sponsor: {name, organization}}, poolPrize}. Listing pages only embed
    // {prize: {sponsor: {organization}}}.
    let prize = v.get("prize").unwrap_or(v);
    let sponsor_obj = prize.get("sponsor");
    let org = sponsor_obj.and_then(|s| s.get("organization"));
    let sponsor = sponsor_obj
        .and_then(|s| s.get("name"))
        .and_then(Value::as_str)
        .or_else(|| org.and_then(|o| o.get("name")).and_then(Value::as_str))
        .map(str::to_string);
    let sponsor_id = org
        .and_then(|o| o.get("uuid"))
        .and_then(Value::as_str)
        .map(str::to_string);
    // Placement: the entry's own name ("3rd place", "Finalist", ...), falling
    // back to explicit placement-ish fields.
    let placement = v
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| str_field(prize, &["placement", "rank"]))
        .map(str::to_string);
    let title = str_field(prize, &["name", "title"]).map(str::to_string);
    let amount = str_field(prize, &["amount", "prizeAmount"]).map(str::to_string);
    if sponsor.is_none() && placement.is_none() && sponsor_id.is_none() {
        return None;
    }
    Some(Prize {
        sponsor,
        sponsor_id,
        placement,
        title,
        amount,
    })
}

/// Debug helper: dump the resolved project objects of a flight document.
pub fn dump_resolved(doc: &FlightDoc) -> Vec<Value> {
    doc.find(&is_project)
        .into_iter()
        .map(|v| doc.resolve(v))
        .collect()
}

/// Convenience: parse an HTML file's flight stream and return (projects, events).
pub fn parse_page(html: &str, now: &str) -> Result<(Vec<Project>, Vec<Event>)> {
    let doc = FlightDoc::parse(&crate::flight::extract_stream(html))?;
    Ok((extract_projects(&doc, now), extract_events(&doc)))
}

/// Iterate over text rows (used by `inspect`).
pub fn text_rows(doc: &FlightDoc) -> Vec<&String> {
    doc.rows()
        .values()
        .filter_map(|r| match r {
            FlightRow::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}
