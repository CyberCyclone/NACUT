//! Helpers shared by the integration tests: fixture paths, timestamps, sample records and a fake HTTP transport.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use jiff::civil::Date;
use jiff::Timestamp;
use nacut_core::analytics::{HttpRequest, Transport};
use nacut_core::store::{DailySeries, DayCost};
use nacut_core::{LogCache, LogScanner, UsageRecord};

/// `shared/fixtures`.
pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shared/fixtures")
}

pub fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

/// The fixture transcripts (`shared/fixtures/proj`) scanned into a fresh cache.
pub fn scan_fixtures() -> LogCache {
    let scanner = LogScanner::new(vec![fixtures().join("proj")]);
    let mut cache = LogCache::new();
    cache.merge(scanner.scan(&HashMap::new()));
    cache
}

/// A daily series with only costs set.
pub fn series(pairs: &[(Date, f64)]) -> DailySeries {
    pairs.iter().map(|(d, c)| (*d, DayCost::with_cost(*c))).collect()
}

/// A record with the given id, time, model and input/output tokens; everything else default.
pub fn record(id: &str, at: &str, model: &str, input: u64, output: u64) -> UsageRecord {
    UsageRecord::new(id, ts(at), model, input, output)
}

/// Fake [`Transport`] that replays queued `(status, body)` responses and records every request.
/// Panics if asked for more responses than were queued.
pub struct Pages {
    pub calls: Mutex<Vec<HttpRequest>>,
    pub pages: Mutex<Vec<(u16, String)>>,
}

impl Pages {
    pub fn new(pages: Vec<(u16, &str)>) -> Arc<Self> {
        Arc::new(Self { calls: Mutex::new(vec![]), pages: Mutex::new(pages.into_iter().map(|(c, b)| (c, b.to_string())).collect()) })
    }

    pub fn calls(&self) -> Vec<HttpRequest> {
        self.calls.lock().unwrap().clone()
    }
}

/// A [`Transport`] backed by shared [`Pages`], so the test can inspect the calls afterwards.
pub struct Shared(pub Arc<Pages>);

impl Transport for Shared {
    fn get(&self, req: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
        self.0.calls.lock().unwrap().push(req.clone());
        let (c, b) = self.0.pages.lock().unwrap().remove(0);
        Ok((c, b.into_bytes()))
    }
}
