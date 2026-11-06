//! Gzipped on-disk cache of raw HTML, content-addressed by key.
//!
//! Re-parses never re-hit ETHGlobal; interrupted runs resume from cache.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::Result;

pub struct HtmlCache {
    root: PathBuf,
}

impl HtmlCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, key: &str) -> PathBuf {
        let shard = key.get(0..2).unwrap_or("zz");
        self.root.join(shard).join(format!("{key}.html.gz"))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let path = self.path(key);
        let file = std::fs::File::open(path).ok()?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut html = String::new();
        decoder.read_to_string(&mut html).ok()?;
        Some(html)
    }

    pub fn put(&self, key: &str, html: &str) -> Result<()> {
        let path = self.path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("gz.tmp");
        {
            let file = std::fs::File::create(&tmp)?;
            let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            encoder.write_all(html.as_bytes())?;
            encoder.finish()?;
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// All cached keys (file stems).
    pub fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return keys;
        };
        for shard in entries.filter_map(|e| e.ok()) {
            let Ok(files) = std::fs::read_dir(shard.path()) else {
                continue;
            };
            for f in files.filter_map(|e| e.ok()) {
                if let Some(stem) = f.path().file_stem().and_then(|s| s.to_str()) {
                    keys.push(stem.trim_end_matches(".html").to_string());
                }
            }
        }
        keys
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = std::env::temp_dir().join(format!("forklore-cache-test-{}", std::process::id()));
        let cache = HtmlCache::new(&dir);
        cache.put("abc123", "<html>hi</html>").unwrap();
        assert_eq!(cache.get("abc123").as_deref(), Some("<html>hi</html>"));
        assert!(cache.get("missing").is_none());
        assert_eq!(cache.keys(), vec!["abc123".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
