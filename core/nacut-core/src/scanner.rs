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
