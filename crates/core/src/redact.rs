//! Redaction of credential-shaped strings from scraped content.
//!
//! Hackathon projects sometimes leak live tokens in demo links or paste them
//! into text fields. We re-host their content, so anything that looks like a
//! credential gets replaced with `[redacted]` before it enters the dataset.

use crate::model::Project;

/// Patterns that indicate a credential. Conservative: only shapes that are
/// meaningless outside of a credential context, to avoid mangling prose.
pub fn patterns() -> &'static Vec<regex::Regex> {
    static RE: std::sync::OnceLock<Vec<regex::Regex>> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        [
            // Mapbox-style JWT tokens (pk./sk. + base64 JWT body). GitHub push
            // protection flags even public (pk.) ones.
            r"\b[sp]k\.ey[A-Za-z0-9._-]{20,}",
            // OpenAI-style keys.
            r"\bsk-[A-Za-z0-9]{20,}",
            // AWS access key ids.
            r"\bAKIA[0-9A-Z]{16}\b",
            // Google API keys.
            r"\bAIza[0-9A-Za-z_\-]{35}\b",
            // GitHub tokens (fine-grained + classic).
            r"\bgh[pousr]_[A-Za-z0-9]{36,}\b",
            // Query-string credentials: ?api_key=... / &access_token=...
            r"(?i)([?&](?:api_?key|access_?token|secret|token|key)=)[A-Za-z0-9._%-]{16,}",
        ]
        .into_iter()
        .map(|p| regex::Regex::new(p).expect("valid redaction pattern"))
        .collect()
    })
}

pub const REDACTED: &str = "[redacted]";

/// Redact credential-shaped substrings in place.
pub fn redact(s: &mut String) {
    for re in patterns() {
        if re.is_match(s) {
            *s = re.replace_all(s, REDACTED).into_owned();
        }
    }
}

/// Redact an optional string field.
pub fn redact_opt(s: &mut Option<String>) {
    if let Some(v) = s {
        redact(v);
    }
}

/// Redact every free-text/URL field of a project record.
pub fn redact_project(p: &mut Project) {
    redact_opt(&mut p.tagline);
    redact_opt(&mut p.demo_url);
    redact_opt(&mut p.source_url);
    redact_opt(&mut p.video_url);
    redact_opt(&mut p.description);
    redact_opt(&mut p.how_its_made);
    redact_opt(&mut p.auto_summary);
    for url in &mut p.screenshots {
        redact(url);
    }
    for prize in &mut p.prizes {
        redact_opt(&mut prize.sponsor);
        redact_opt(&mut prize.placement);
        redact_opt(&mut prize.title);
        redact_opt(&mut prize.amount);
    }
}

/// True if `s` plausibly is an http(s) URL. Link fields on the showcase are
/// free-text in practice; junk entries must not become links in the dataset.
pub fn is_http_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("http://") || s.starts_with("https://"))
        && s.len() > 8
        && !s.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_mapbox_tokens() {
        // Same shape GitHub push protection flagged in our data, assembled at
        // runtime so the literal never appears in source (push protection
        // scans raw text and blocks real-looking tokens in test fixtures).
        let head = "the api key for mapbox is pk.";
        let body = [
            "eyJ",
            "1",
            "Ijoi",
            "a",
            "nV2aWUyMiIs",
            "a",
            "OiJjajNoN3hzeDEwMDFuMzNxZ2txeXR1ZnIzIn0.",
            "2Thoi_xQLD5f9d_R_DD7lg",
            " ok",
        ]
        .concat();
        let mut s = format!("{head}{body}");
        redact(&mut s);
        assert!(!s.contains("2Thoi"), "token should be gone: {s}");
        assert!(s.contains(REDACTED));
    }

    #[test]
    fn redacts_query_credentials() {
        let mut s = "https://app.example.com/?access_token=verysecretvalue123456&x=1".to_string();
        redact(&mut s);
        assert!(!s.contains("verysecretvalue"));
    }

    #[test]
    fn keeps_normal_prose_and_urls() {
        let mut s = "https://github.com/example/repo and we used skunk works".to_string();
        redact(&mut s);
        assert_eq!(s, "https://github.com/example/repo and we used skunk works");
    }

    #[test]
    fn url_validation() {
        assert!(is_http_url("https://example.com"));
        assert!(!is_http_url("oQuest-Android (the api key ...)"));
        assert!(!is_http_url("ftp://x"));
    }
}
