//! Reads Claude Code JSONL transcripts incrementally and turns assistant messages into records.
//! Port of `LogScanner.swift`.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::record::UsageRecord;

/// Where a transcript file was last read up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCheckpoint {
    /// Inode on Unix; creation time (100 ns ticks) on Windows, which has no stable inode in std.
    pub inode: u64,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: String,
    pub checkpoint: FileCheckpoint,
    pub records: Vec<UsageRecord>,
}

#[derive(Debug, Clone, Default)]
pub struct LogScanner {
    pub roots: Vec<PathBuf>,
}

/// The user's home folder (`HOME`, or `USERPROFILE` on Windows).
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Expands a leading `~` to the home folder.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        home_dir().join(rest)
    } else if path == "~" {
        home_dir()
    } else {
        PathBuf::from(path)
    }
}

impl LogScanner {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// Default transcript folders: `~/.claude/projects`, `~/.config/claude/projects`,
    /// and `<dir>/projects` for each entry of `CLAUDE_CONFIG_DIR` (comma-separated).
    pub fn default_roots() -> Vec<PathBuf> {
        Self::default_roots_with(&home_dir(), std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref())
    }

    pub fn default_roots_with(home: &Path, claude_config_dir: Option<&str>) -> Vec<PathBuf> {
        let mut urls = vec![home.join(".claude").join("projects"), home.join(".config").join("claude").join("projects")];
        if let Some(dir) = claude_config_dir.filter(|d| !d.is_empty()) {
            for part in dir.split(',') {
                let p = part.trim();
                if !p.is_empty() {
                    urls.push(expand_tilde(p).join("projects"));
                }
            }
        }
        let mut seen = HashSet::new();
        urls.retain(|u| seen.insert(u.clone()));
        urls
    }

    /// All `.jsonl` files under the roots (recursive, so subagent transcripts are included).
    /// Hidden files and folders are skipped, as on the Mac.
    pub fn transcript_files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for root in &self.roots {
            walk(root, &mut out);
        }
        out
    }

    /// Scans every transcript, reading only bytes appended since `checkpoints`.
    pub fn scan(&self, checkpoints: &HashMap<String, FileCheckpoint>) -> Vec<FileResult> {
        let files = self.transcript_files();
        files
            .par_iter()
            .filter_map(|p| {
                let key = path_key(p);
                Self::scan_file(p, checkpoints.get(&key).copied())
            })
            .collect()
    }

    /// Reads one file from its checkpoint. Returns `None` when nothing changed.
    pub fn scan_file(path: &Path, checkpoint: Option<FileCheckpoint>) -> Option<FileResult> {
        let meta = std::fs::metadata(path).ok()?;
        let size = meta.len();
        let inode = file_identity(&meta);
        let key = path_key(path);

        let mut start = 0;
        if let Some(cp) = checkpoint {
            if cp.inode == inode && cp.offset <= size {
                if cp.offset == size {
                    return None;
                }
                start = cp.offset;
            }
        }
        let mut file = File::open(path).ok()?;
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut data = Vec::with_capacity((size - start) as usize);
        file.read_to_end(&mut data).ok()?;

        let same = |records| FileResult { path: key.clone(), checkpoint: FileCheckpoint { inode, offset: start }, records };
        if data.is_empty() {
            return Some(same(Vec::new()));
        }
        // Only consume complete lines; a partially written last line is re-read next time.
        let Some(last_newline) = memchr::memrchr(b'\n', &data) else {
            return Some(same(Vec::new()));
        };
        let complete = &data[..=last_newline];
        let is_subagent_file = path.components().any(|c| c.as_os_str() == "subagents");
        let records = parse_lines(complete, is_subagent_file);
        Some(FileResult {
            path: key,
            checkpoint: FileCheckpoint { inode, offset: start + complete.len() as u64 },
            records,
        })
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        if ft.is_dir() {
            walk(&path, out);
        } else if ft.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

/// Checkpoint key for a file.
pub fn path_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn file_identity(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.ino()
}

#[cfg(windows)]
fn file_identity(meta: &std::fs::Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;
    meta.creation_time()
}

#[cfg(not(any(unix, windows)))]
fn file_identity(_meta: &std::fs::Metadata) -> u64 {
    0
}

const USAGE_NEEDLE: &[u8] = b"\"usage\"";
const ASSISTANT_NEEDLE: &[u8] = b"\"assistant\"";

pub(crate) fn parse_lines(data: &[u8], is_subagent_file: bool) -> Vec<UsageRecord> {
    let usage = memchr::memmem::Finder::new(USAGE_NEEDLE);
    let assistant = memchr::memmem::Finder::new(ASSISTANT_NEEDLE);
    data.split(|&b| b == b'\n')
        .filter(|line| line.len() > 20 && usage.find(line).is_some() && assistant.find(line).is_some())
        .filter_map(|line| parse_line(line, is_subagent_file))
        .collect()
}

pub(crate) fn parse_line(line: &[u8], is_subagent_file: bool) -> Option<UsageRecord> {
    // A file saved by a Windows editor can start with a UTF-8 byte-order mark, which JSON parsers reject.
    let line = line.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(line);
    let raw: RawLine = serde_json::from_slice(line).ok()?;
    if raw.r#type.as_deref() != Some("assistant") || raw.is_api_error_message == Some(true) {
        return None;
    }
    let msg = raw.message?;
    let usage = msg.usage?;
    let model = msg.model.filter(|m| !m.is_empty() && m != "<synthetic>")?;
    let ts = raw.timestamp.as_deref().and_then(parse_date)?;
    let id = msg.id.or(raw.request_id).or(raw.uuid)?;

    let mut w5 = usage.cache_creation.as_ref().and_then(|c| c.ephemeral_5m_input_tokens).unwrap_or(0);
    let w1h = usage.cache_creation.as_ref().and_then(|c| c.ephemeral_1h_input_tokens).unwrap_or(0);
    let total_write = usage.cache_creation_input_tokens.unwrap_or(0);
    if usage.cache_creation.is_none() || w5 + w1h < total_write {
        w5 = w5.max(total_write.saturating_sub(w1h));
    }
    Some(UsageRecord {
        id,
        timestamp: ts,
        model,
        input: usage.input_tokens.unwrap_or(0),
        output: usage.output_tokens.unwrap_or(0),
        cache_write_5m: w5,
        cache_write_1h: w1h,
        cache_read: usage.cache_read_input_tokens.unwrap_or(0),
        web_searches: usage.server_tool_use.and_then(|s| s.web_search_requests).unwrap_or(0),
        fast: usage.speed.as_deref() == Some("fast"),
        cwd: raw.cwd.unwrap_or_default(),
        session_id: raw.session_id.unwrap_or_default(),
        slug: raw.slug,
        entrypoint: raw.entrypoint.unwrap_or_else(|| "cli".into()),
        is_subagent: is_subagent_file || raw.is_sidechain == Some(true),
        cost: 0.0,
        estimated: false,
    })
}

/// RFC 3339 timestamps, with or without fractional seconds, `Z` or an offset.
pub fn parse_date(s: &str) -> Option<Timestamp> {
    s.parse::<Timestamp>().ok()
}

// Raw JSON shapes: only the fields we need; message content is never decoded.

#[derive(Deserialize)]
struct RawLine {
    r#type: Option<String>,
    timestamp: Option<String>,
    uuid: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    cwd: Option<String>,
    slug: Option<String>,
    entrypoint: Option<String>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<bool>,
    #[serde(rename = "isApiErrorMessage")]
    is_api_error_message: Option<bool>,
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation: Option<RawCacheCreation>,
    server_tool_use: Option<RawServerToolUse>,
    speed: Option<String>,
}

#[derive(Deserialize)]
struct RawCacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct RawServerToolUse {
    web_search_requests: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An assistant line with the given message fields (JSON object members, without braces).
    fn line(extra: &str, message: &str) -> String {
        format!(r#"{{"type":"assistant","timestamp":"2026-11-20T01:00:00Z"{extra},"message":{{{message}}}}}"#)
    }

    const MSG: &str = r#""id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":2}"#;

    fn parse(s: &str) -> Option<UsageRecord> {
        parse_line(s.as_bytes(), false)
    }

    #[test]
    fn parses_a_minimal_assistant_line() {
        let r = parse(&line("", MSG)).unwrap();
        assert_eq!((r.id.as_str(), r.model.as_str(), r.input, r.output), ("m1", "claude-opus-5-5", 1, 2));
        assert_eq!(r.timestamp, "2026-11-20T01:00:00Z".parse::<Timestamp>().unwrap());
        assert_eq!(r.entrypoint, "cli");
        assert_eq!((r.cwd.as_str(), r.session_id.as_str(), r.slug.as_deref()), ("", "", None));
        assert!(!r.is_subagent && !r.fast);
    }

    #[test]
    fn crlf_line_endings_parse() {
        let data = format!("{}\r\n{}\r\n", line("", MSG), line("", &MSG.replace("m1", "m2")));
        let ids: Vec<String> = parse_lines(data.as_bytes(), false).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["m1", "m2"]);
    }

    #[test]
    fn byte_order_mark_on_the_first_line_is_ignored() {
        let data = format!("\u{feff}{}\n", line("", MSG));
        assert_eq!(parse_lines(data.as_bytes(), false).len(), 1);
    }

    #[test]
    fn malformed_short_and_blank_lines_are_skipped() {
        let data = format!("{{\"usage\" \"assistant\" broken\n\n   \n{{\"usage\":\"assistant\"}}\n{}\n", line("", MSG));
        let records = parse_lines(data.as_bytes(), false);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "m1");
    }

    #[test]
    fn lines_without_a_usage_key_are_not_decoded() {
        let no_usage = line("", r#""id":"m1","model":"claude-opus-5-5""#);
        assert!(parse_lines(format!("{no_usage}\n").as_bytes(), false).is_empty());
    }

    #[test]
    fn non_assistant_or_untyped_lines_are_dropped() {
        assert!(parse(&line("", MSG).replace(r#""type":"assistant""#, r#""type":"user""#)).is_none());
        assert!(parse(&line("", MSG).replace(r#""type":"assistant","#, "")).is_none());
    }

    #[test]
    fn api_error_messages_are_dropped() {
        assert!(parse(&line(r#","isApiErrorMessage":true"#, MSG)).is_none());
        assert!(parse(&line(r#","isApiErrorMessage":false"#, MSG)).is_some());
    }

    #[test]
    fn missing_message_or_usage_is_dropped() {
        assert!(parse(r#"{"type":"assistant","timestamp":"2026-11-20T01:00:00Z","uuid":"u"}"#).is_none());
        assert!(parse(&line("", r#""id":"m1","model":"claude-opus-5-5""#)).is_none());
    }

    #[test]
    fn empty_or_synthetic_model_is_dropped() {
        assert!(parse(&line("", &MSG.replace("claude-opus-5-5", ""))).is_none());
        assert!(parse(&line("", &MSG.replace("claude-opus-5-5", "<synthetic>"))).is_none());
        assert!(parse(&line("", r#""id":"m1","usage":{"input_tokens":1}"#)).is_none());
    }

    #[test]
    fn missing_or_unparseable_timestamp_is_dropped() {
        let l = line("", MSG);
        assert!(parse(&l.replace(r#""timestamp":"2026-11-20T01:00:00Z","#, "")).is_none());
        assert!(parse(&l.replace("2026-11-20T01:00:00Z", "yesterday")).is_none());
        assert!(parse(&l.replace("2026-11-20T01:00:00Z", "2026-11-20 01:00:00")).is_none());
    }

    #[test]
    fn offset_timestamps_are_converted_to_utc() {
        let r = parse(&line("", MSG).replace("2026-11-20T01:00:00Z", "2026-11-20T11:30:00.250+10:30")).unwrap();
        assert_eq!(r.timestamp, "2026-11-20T01:00:00.25Z".parse::<Timestamp>().unwrap());
    }

    #[test]
    fn id_falls_back_to_request_id_then_uuid() {
        let no_id = r#""model":"claude-opus-5-5","usage":{"input_tokens":1}"#;
        assert_eq!(parse(&line(r#","requestId":"req","uuid":"u""#, no_id)).unwrap().id, "req");
        assert_eq!(parse(&line(r#","uuid":"u""#, no_id)).unwrap().id, "u");
        assert!(parse(&line("", no_id)).is_none());
        assert_eq!(parse(&line(r#","requestId":"req","uuid":"u""#, MSG)).unwrap().id, "m1");
    }

    fn writes(usage: &str) -> (u64, u64) {
        let r = parse(&line("", &format!(r#""id":"m1","model":"claude-opus-5-5","usage":{{{usage}}}"#))).unwrap();
        (r.cache_write_5m, r.cache_write_1h)
    }

    #[test]
    fn cache_writes_without_a_breakdown_count_as_5_minute() {
        assert_eq!(writes(r#""cache_creation_input_tokens":700"#), (700, 0));
    }

    #[test]
    fn cache_write_breakdown_short_of_the_total_tops_up_5_minute() {
        let u = r#""cache_creation_input_tokens":1000,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":300}"#;
        assert_eq!(writes(u), (700, 300));
    }

    #[test]
    fn cache_write_breakdown_above_the_total_is_kept() {
        let u = r#""cache_creation_input_tokens":100,"cache_creation":{"ephemeral_5m_input_tokens":200,"ephemeral_1h_input_tokens":300}"#;
        assert_eq!(writes(u), (200, 300));
    }

    #[test]
    fn empty_cache_breakdown_takes_the_total_as_5_minute() {
        assert_eq!(writes(r#""cache_creation_input_tokens":50,"cache_creation":{}"#), (50, 0));
    }

    #[test]
    fn missing_token_counts_are_zero() {
        let r = parse(&line("", r#""id":"m1","model":"claude-opus-5-5","usage":{}"#)).unwrap();
        assert_eq!((r.input, r.output, r.cache_write_5m, r.cache_write_1h, r.cache_read, r.web_searches), (0, 0, 0, 0, 0, 0));
    }

    #[test]
    fn sidechain_flag_alone_marks_a_subagent() {
        assert!(parse(&line(r#","isSidechain":true"#, MSG)).unwrap().is_subagent);
        assert!(!parse(&line(r#","isSidechain":false"#, MSG)).unwrap().is_subagent);
        assert!(parse_line(line("", MSG).as_bytes(), true).unwrap().is_subagent);
    }

    #[test]
    fn copies_session_fields_and_speed() {
        let extra = r#","cwd":"/x","sessionId":"s","slug":"brave-otter","entrypoint":"sdk-ts""#;
        let msg = r#""id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":1,"speed":"fast","server_tool_use":{"web_search_requests":2}}"#;
        let r = parse(&line(extra, msg)).unwrap();
        assert_eq!((r.cwd.as_str(), r.session_id.as_str(), r.slug.as_deref(), r.entrypoint.as_str()), ("/x", "s", Some("brave-otter"), "sdk-ts"));
        assert!(r.fast);
        assert_eq!(r.web_searches, 2);
    }

    #[test]
    fn parse_date_accepts_rfc3339_variants() {
        assert_eq!(parse_date("2026-11-20T01:00:00Z").unwrap().as_second(), 1_795_136_400);
        assert_eq!(parse_date("2026-11-20T01:00:00.123456Z").unwrap().as_microsecond(), 1_795_136_400_123_456);
        assert_eq!(parse_date("2026-11-20T02:00:00+01:00").unwrap().as_second(), 1_795_136_400);
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("2026-11-20"), None);
    }
}
