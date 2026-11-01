//! Parser for React Server Components "flight" payloads.
//!
//! ETHGlobal's Next.js site embeds the RSC flight stream in
//! `self.__next_f.push([1, "..."])` script chunks. The stream is a series of
//! newline-separated rows: `<hex-id>:<payload>`. Payload kinds we care about:
//!
//! - `T<hex-len>,<raw text>`: text rows (long descriptions, referenced as `$<id>`)
//! - JSON values (`{`, `[`, `"`, primitives): structured data rows
//! - anything else (module refs like `I[...]`): skipped
//!
//! Strings of the form `$<id>` / `$L<id>` inside JSON rows reference other rows.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{Error, Result};

/// One flight row.
#[derive(Debug, Clone, PartialEq)]
pub enum FlightRow {
    /// Raw text row (`T<hex-len>,...`).
    Text(String),
    /// Parsed JSON value.
    Json(Value),
    /// Anything else (module refs, hints) — kept for completeness only.
    Other,
}

/// A parsed flight document: row id -> row payload.
#[derive(Debug, Clone, Default)]
pub struct FlightDoc {
    rows: BTreeMap<String, FlightRow>,
}

/// Extract and unescape the raw flight stream from an HTML page.
pub fn extract_stream(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(pos) = rest.find("self.__next_f.push(") {
        let tail = &rest[pos..];
        let Some(open) = tail.find('[') else { break };
        // The first argument is `1`, the second a JSON string literal.
        let args = &tail[open + 1..];
        let Some(quote_start_rel) = args.find('"') else {
            break;
        };
        let bytes = args.as_bytes();
        let mut i = quote_start_rel + 1;
        let mut lit = String::new();
        let mut closed = false;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => {
                    // JSON escape: delegate to serde_json for correctness by
                    // collecting the raw escape sequence.
                    lit.push('\\');
                    lit.push(bytes[i + 1] as char);
                    i += 2;
                }
                b'"' => {
                    closed = true;
                    i += 1;
                    break;
                }
                _ => {
                    // Collect the byte as part of a char boundary safely.
                    let ch_len = utf8_len(bytes[i]);
                    lit.push_str(&args[i..i + ch_len]);
                    i += ch_len;
                }
            }
        }
        if !closed {
            break;
        }
        if let Ok(decoded) = serde_json::from_str::<String>(&format!("\"{lit}\"")) {
            out.push_str(&decoded);
        }
        rest = &args[i..];
    }
    out
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

impl FlightDoc {
    /// Parse a raw flight stream into rows.
    pub fn parse(stream: &str) -> Result<Self> {
        let mut rows = BTreeMap::new();
        let bytes = stream.as_bytes();
        let mut i = 0usize;

        while i < bytes.len() {
            // Row id: hex digits followed by ':'.
            let id_start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i == id_start || i >= bytes.len() || bytes[i] != b':' {
                // Desync: skip to the next newline and try again.
                match stream[i..].find('\n') {
                    Some(p) => i += p + 1,
                    None => break,
                }
                continue;
            }
            let id = stream[id_start..i].to_string();
            i += 1; // past ':'

            if i < bytes.len() && bytes[i] == b'T' {
                // Text row: T<hex-len>,<bytes>
                i += 1;
                let len_start = i;
                while i < bytes.len() && bytes[i] != b',' {
                    i += 1;
                }
                let len = usize::from_str_radix(&stream[len_start..i], 16)
                    .map_err(|e| Error::Flight(format!("bad text row length for row {id}: {e}")))?;
                i += 1; // past ','
                let end = (i + len).min(bytes.len());
                rows.insert(id, FlightRow::Text(stream[i..end].to_string()));
                i = end;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                continue;
            }

            if i < bytes.len() && is_json_start(bytes[i]) {
                let end = scan_json_end(stream, i)
                    .map_err(|e| Error::Flight(format!("row {id}: {e}")))?;
                match serde_json::from_str::<Value>(&stream[i..end]) {
                    Ok(v) => {
                        rows.insert(id, FlightRow::Json(v));
                    }
                    Err(_) => {
                        rows.insert(id, FlightRow::Other);
                    }
                }
                i = end;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }

            // Unknown row kind: skip the line.
            match stream[i..].find('\n') {
                Some(p) => {
                    rows.entry(id).or_insert(FlightRow::Other);
                    i += p + 1;
                }
                None => break,
            }
        }

        Ok(FlightDoc { rows })
    }

    /// Resolve `$<id>` / `$L<id>` string references to their row payloads.
    pub fn resolve(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => {
                let id = s
                    .strip_prefix('$')
                    .map(|r| r.strip_prefix('L').unwrap_or(r));
                match id {
                    Some(id) if !id.is_empty() && is_row_id(id) => match self.rows.get(id) {
                        Some(FlightRow::Text(t)) => Value::String(t.clone()),
                        Some(FlightRow::Json(v)) => v.clone(),
                        _ => value.clone(),
                    },
                    _ => value.clone(),
                }
            }
            Value::Array(a) => Value::Array(a.iter().map(|v| self.resolve(v)).collect()),
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), self.resolve(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Walk every JSON row (depth-first) and yield values for which `pred`
    /// returns true.
    pub fn find<F>(&self, pred: &F) -> Vec<&Value>
    where
        F: Fn(&Value) -> bool,
    {
        let mut found = Vec::new();
        for row in self.rows.values() {
            if let FlightRow::Json(v) = row {
                walk(v, pred, &mut found);
            }
        }
        found
    }

    /// All JSON rows as resolved values (for debugging/inspection).
    pub fn rows(&self) -> &BTreeMap<String, FlightRow> {
        &self.rows
    }
}

fn is_row_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_json_start(b: u8) -> bool {
    matches!(
        b,
        b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
    )
}

/// Return the exclusive end index of the JSON value starting at `start`.
fn scan_json_end(s: &str, start: usize) -> Result<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i64;
    let mut in_str = false;
    let mut esc = false;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
                // A lone JSON string value ends here when depth == 0.
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(i + 1);
                    }
                    if depth < 0 {
                        return Err(Error::Flight("unbalanced brackets".into()));
                    }
                }
                b'\n' if depth == 0 => {
                    // Primitive value terminated by the row separator.
                    return Ok(i);
                }
                _ => {}
            }
        }
        i += 1;
    }
    Ok(bytes.len())
}

fn walk<'a, F>(v: &'a Value, pred: &F, out: &mut Vec<&'a Value>)
where
    F: Fn(&Value) -> bool,
{
    if pred(v) {
        out.push(v);
        // Still descend to find nested matches.
    }
    match v {
        Value::Object(map) => {
            for child in map.values() {
                walk(child, pred, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                walk(child, pred, out);
            }
        }
        _ => {}
    }
}

/// Parse an HTML page into a flight document.
pub fn parse_html(html: &str) -> Result<FlightDoc> {
    FlightDoc::parse(&extract_stream(html))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_text_and_json_rows() {
        let stream = "28:T6,hello!\n2:[\"$\",\"div\",null,{\"children\":\"x\"}]\nc:null\n";
        let doc = FlightDoc::parse(stream).unwrap();
        assert_eq!(
            doc.rows().get("28"),
            Some(&FlightRow::Text("hello!".into()))
        );
        assert_eq!(
            doc.rows().get("2"),
            Some(&FlightRow::Json(json!(["$","div",null,{"children":"x"}])))
        );
        assert_eq!(doc.rows().get("c"), Some(&FlightRow::Json(Value::Null)));
    }

    #[test]
    fn resolves_refs() {
        let stream = "28:T4,abcd\n3:{\"description\":\"$28\"}\n";
        let doc = FlightDoc::parse(stream).unwrap();
        let row = doc.rows().get("3").unwrap();
        let FlightRow::Json(v) = row else { panic!() };
        let resolved = doc.resolve(v);
        assert_eq!(resolved["description"], json!("abcd"));
    }

    #[test]
    fn finds_nested_objects() {
        let stream =
            "3:{\"a\":{\"b\":{\"uuid\":\"sk506\",\"slug\":\"dedan\",\"name\":\"Dedan\"}}}\n";
        let doc = FlightDoc::parse(stream).unwrap();
        let hits = doc.find(&|v: &Value| {
            v.get("uuid").is_some() && v.get("slug").is_some() && v.get("name").is_some()
        });
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn extracts_stream_from_html() {
        let html = r#"<html><body><script>self.__next_f.push([1,"3:{\"a\":1}\n"])</script><script>self.__next_f.push([1,"4:\"x\"\n"])</script></body></html>"#;
        let stream = extract_stream(html);
        assert_eq!(stream, "3:{\"a\":1}\n4:\"x\"\n");
        let doc = FlightDoc::parse(&stream).unwrap();
        assert_eq!(doc.rows().get("3"), Some(&FlightRow::Json(json!({"a": 1}))));
    }
}
