//! Persisted, deduplicated message history plus per-file read checkpoints.
//! Port of `LogCache.swift`.
//!
//! The cache keeps history even after Claude Code deletes old transcripts,
//! so totals never shrink when transcripts are cleaned up.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::record::UsageRecord;
use crate::scanner::{FileCheckpoint, FileResult};

pub const CACHE_VERSION: u32 = 3;
pub const CACHE_FILE_NAME: &str = "log-cache-v3.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogCache {
    pub version: u32,
    pub checkpoints: HashMap<String, FileCheckpoint>,
    pub records: HashMap<String, UsageRecord>,
}

impl LogCache {
    pub fn new() -> Self {
        Self { version: CACHE_VERSION, ..Default::default() }
    }

    /// Merges scan results. A message ID can appear on several lines (streaming updates,
    /// resumed sessions); the line with the most output tokens is the final one.
    /// Returns whether any record changed.
    pub fn merge(&mut self, results: Vec<FileResult>) -> bool {
        let mut changed = false;
        for r in results {
            self.checkpoints.insert(r.path, r.checkpoint);
            for rec in r.records {
                match self.records.get(&rec.id) {
                    Some(existing) => {
                        if rec.output > existing.output
                            || (rec.output == existing.output && rec.total_tokens() > existing.total_tokens())
                        {
                            self.records.insert(rec.id.clone(), rec);
                            changed = true;
                        }
                    }
                    None => {
                        self.records.insert(rec.id.clone(), rec);
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// Reads the Swift app's v2 cache (`log-cache.json`: camelCase fields, dates as seconds since 1970),
    /// so history kept there survives the move to this core even if Claude Code has deleted the transcripts.
    pub fn import_swift_v2(path: &Path) -> Option<LogCache> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct V2Record {
            id: String,
            timestamp: f64,
            model: String,
            input: u64,
            output: u64,
            cache_write5m: u64,
            cache_write1h: u64,
            cache_read: u64,
            #[serde(default)]
            web_searches: u64,
            #[serde(default)]
            fast: bool,
            #[serde(default)]
            cwd: String,
            #[serde(default)]
            session_id: String,
            #[serde(default)]
            slug: Option<String>,
            #[serde(default)]
            entrypoint: Option<String>,
            #[serde(default)]
            is_subagent: bool,
        }
        #[derive(Deserialize)]
        struct V2 {
            version: u32,
            #[serde(default)]
            checkpoints: HashMap<String, FileCheckpoint>,
            records: HashMap<String, V2Record>,
        }
        let v2: V2 = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        if v2.version != 2 {
            return None;
        }
        let mut cache = LogCache::new();
        cache.checkpoints = v2.checkpoints;
        for (id, r) in v2.records {
            let Ok(ts) = jiff::Timestamp::from_millisecond((r.timestamp * 1000.0).round() as i64) else { continue };
            let mut rec = UsageRecord::new(&r.id, ts, &r.model, r.input, r.output);
            rec.cache_write_5m = r.cache_write5m;
            rec.cache_write_1h = r.cache_write1h;
            rec.cache_read = r.cache_read;
            rec.web_searches = r.web_searches;
            rec.fast = r.fast;
            rec.cwd = r.cwd;
            rec.session_id = r.session_id;
            rec.slug = r.slug;
            rec.entrypoint = r.entrypoint.unwrap_or_else(|| "cli".into());
            rec.is_subagent = r.is_subagent;
            cache.records.insert(id, rec);
        }
        Some(cache)
    }

    /// Loads a cache, or returns an empty one if the file is missing, unreadable or an old version.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|d| serde_json::from_slice::<LogCache>(&d).ok())
            .filter(|c| c.version == CACHE_VERSION)
            .unwrap_or_else(Self::new)
    }

    /// Atomic save (write to a temp file, then rename), readable only by this user.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let data = serde_json::to_vec(self).map_err(io::Error::other)?;
        crate::platform::write_private(path, &data)
    }
}
