//! The persisted history cache: merge tie-breaks, loading, the Swift v2 import and saving.

use nacut_core::cache::{CACHE_FILE_NAME, CACHE_VERSION};
use nacut_core::scanner::{FileCheckpoint, FileResult};
use nacut_core::{LogCache, UsageRecord};

mod common;

use common::record;

fn result(path: &str, offset: u64, records: Vec<UsageRecord>) -> FileResult {
    FileResult { path: path.into(), checkpoint: FileCheckpoint { inode: 1, offset }, records }
}

fn with_cache_read(mut r: UsageRecord, read: u64) -> UsageRecord {
    r.cache_read = read;
    r
}

#[test]
fn new_cache_is_current_version_and_empty() {
    let c = LogCache::new();
    assert_eq!(c.version, CACHE_VERSION);
    assert_eq!(CACHE_VERSION, 3);
    assert_eq!(CACHE_FILE_NAME, "log-cache-v3.json");
    assert!(c.records.is_empty() && c.checkpoints.is_empty());
}

#[test]
fn merge_adds_new_records() {
    let mut c = LogCache::new();
    assert!(c.merge(vec![result("/a", 10, vec![record("m1", "2026-11-20T01:00:00Z", "claude-opus-5-5", 1, 1)])]));
    assert_eq!(c.records.len(), 1);
    assert_eq!(c.checkpoints["/a"].offset, 10);
}

#[test]
fn same_output_and_more_total_tokens_replaces() {
    let mut c = LogCache::new();
    c.merge(vec![result("/a", 1, vec![record("m1", "2026-11-20T01:00:00Z", "claude-opus-5-5", 1, 5)])]);
    let bigger = with_cache_read(record("m1", "2026-11-20T01:00:01Z", "claude-opus-5-5", 1, 5), 100);
    assert!(c.merge(vec![result("/a", 2, vec![bigger.clone()])]));
    assert_eq!(c.records["m1"], bigger);
}

#[test]
fn same_output_and_same_total_keeps_the_existing_record() {
    let mut c = LogCache::new();
    let first = record("m1", "2026-11-20T01:00:00Z", "claude-opus-5-5", 1, 5);
    c.merge(vec![result("/a", 1, vec![first.clone()])]);
    let mut twin = record("m1", "2026-11-21T01:00:00Z", "claude-opus-5-5", 1, 5);
    twin.cwd = "/elsewhere".into();
    assert!(!c.merge(vec![result("/b", 2, vec![twin])]));
    assert_eq!(c.records["m1"], first);
}

#[test]
fn lower_output_never_replaces_even_with_more_tokens() {
    let mut c = LogCache::new();
    let first = record("m1", "2026-11-20T01:00:00Z", "claude-opus-5-5", 1, 5);
    c.merge(vec![result("/a", 1, vec![first.clone()])]);
    let streaming = with_cache_read(record("m1", "2026-11-20T01:00:00Z", "claude-opus-5-5", 1_000, 4), 1_000_000);
    assert!(!c.merge(vec![result("/a", 2, vec![streaming])]));
    assert_eq!(c.records["m1"], first);
}

#[test]
fn checkpoint_only_results_report_no_change_but_move_checkpoints() {
    let mut c = LogCache::new();
    c.merge(vec![result("/a", 1, vec![])]);
    assert!(!c.merge(vec![result("/a", 50, vec![]), result("/b", 7, vec![])]));
    assert_eq!(c.checkpoints["/a"].offset, 50);
    assert_eq!(c.checkpoints["/b"].offset, 7);
    assert!(c.records.is_empty());
}

#[test]
fn load_missing_file_gives_an_empty_current_cache() {
    let dir = tempfile::tempdir().unwrap();
    let c = LogCache::load(&dir.path().join("none.json"));
    assert_eq!(c.version, CACHE_VERSION);
    assert!(c.records.is_empty());
}

#[test]
fn load_corrupt_file_gives_an_empty_current_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    std::fs::write(&path, b"{ not json").unwrap();
    let c = LogCache::load(&path);
    assert_eq!(c.version, CACHE_VERSION);
    assert!(c.records.is_empty());
}

#[test]
fn load_other_version_gives_an_empty_current_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    let mut old = common::scan_fixtures();
    old.version = 2;
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let c = LogCache::load(&path);
    assert_eq!(c.version, CACHE_VERSION);
    assert!(c.records.is_empty() && c.checkpoints.is_empty());
}

#[test]
fn save_to_an_unwritable_path_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, b"x").unwrap();
    // The parent "folder" is a regular file.
    assert!(LogCache::new().save(&file.join("cache.json")).is_err());
}

fn v2(json: &str) -> Option<LogCache> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log-cache.json");
    std::fs::write(&path, json).unwrap();
    LogCache::import_swift_v2(&path)
}

const V2_RECORD: &str = r#"{"id":"m1","timestamp":1795429672.5,"model":"claude-opus-5-5","input":1,"output":2,"cacheWrite5m":3,"cacheWrite1h":4,"cacheRead":5}"#;

#[test]
fn v2_import_rejects_other_versions() {
    assert!(v2(&format!(r#"{{"version":3,"records":{{"m1":{V2_RECORD}}}}}"#)).is_none());
    assert!(v2(&format!(r#"{{"version":1,"records":{{"m1":{V2_RECORD}}}}}"#)).is_none());
}

#[test]
fn v2_import_of_a_missing_or_corrupt_file_is_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(LogCache::import_swift_v2(&dir.path().join("none.json")).is_none());
    assert!(v2("{ nope").is_none());
}

#[test]
fn v2_import_fills_defaults_for_missing_fields() {
    let c = v2(&format!(r#"{{"version":2,"records":{{"m1":{V2_RECORD}}}}}"#)).unwrap();
    assert_eq!(c.version, CACHE_VERSION);
    assert!(c.checkpoints.is_empty());
    let r = &c.records["m1"];
    assert_eq!(r.entrypoint, "cli");
    assert_eq!((r.web_searches, r.fast, r.is_subagent), (0, false, false));
    assert_eq!((r.cwd.as_str(), r.session_id.as_str(), r.slug.as_deref()), ("", "", None));
}

#[test]
fn v2_import_copies_every_field() {
    let rec = r#"{"id":"m1","timestamp":1795429672.5,"model":"claude-opus-5-5","input":1,"output":2,"cacheWrite5m":3,"cacheWrite1h":4,"cacheRead":5,"webSearches":6,"fast":true,"cwd":"/x","sessionId":"s","slug":"brave-otter","entrypoint":"sdk-py","isSubagent":true}"#;
    let c = v2(&format!(r#"{{"version":2,"records":{{"m1":{rec}}}}}"#)).unwrap();
    let r = &c.records["m1"];
    assert_eq!(r.web_searches, 6);
    assert!(r.fast && r.is_subagent);
    assert_eq!((r.cwd.as_str(), r.session_id.as_str(), r.slug.as_deref(), r.entrypoint.as_str()), ("/x", "s", Some("brave-otter"), "sdk-py"));
    assert_eq!(r.cost, 0.0);
}

#[test]
fn v2_import_skips_only_records_with_out_of_range_timestamps() {
    let bad = V2_RECORD.replace("1795429672.5", "1e300").replace("\"m1\"", "\"m2\"");
    let c = v2(&format!(r#"{{"version":2,"records":{{"m1":{V2_RECORD},"m2":{bad}}}}}"#)).unwrap();
    assert_eq!(c.records.keys().collect::<Vec<_>>(), ["m1"]);
}

#[test]
fn v2_import_keys_records_by_their_map_key() {
    let c = v2(&format!(r#"{{"version":2,"records":{{"key":{V2_RECORD}}}}}"#)).unwrap();
    assert_eq!(c.records.keys().collect::<Vec<_>>(), ["key"]);
    assert_eq!(c.records["key"].id, "m1");
}
