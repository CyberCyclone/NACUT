//! One deduplicated assistant message from a Claude Code transcript.
//! Port of `UsageRecord.swift`.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub timestamp: Timestamp,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    #[serde(default)]
    pub web_searches: u64,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    #[serde(default)]
    pub is_subagent: bool,

    /// Computed by `PriceBook`; not persisted.
    #[serde(skip)]
    pub cost: f64,
    /// True when the model's price was guessed from its family.
    #[serde(skip)]
    pub estimated: bool,
}

fn default_entrypoint() -> String {
    "cli".into()
}

impl UsageRecord {
    /// A record with only the token fields set; used by tests and callers that build records by hand.
    pub fn new(id: &str, timestamp: Timestamp, model: &str, input: u64, output: u64) -> Self {
        Self {
            id: id.into(),
            timestamp,
            model: model.into(),
            input,
            output,
            cache_write_5m: 0,
            cache_write_1h: 0,
            cache_read: 0,
            web_searches: 0,
            fast: false,
            cwd: String::new(),
            session_id: String::new(),
            slug: None,
            entrypoint: default_entrypoint(),
            is_subagent: false,
            cost: 0.0,
            estimated: false,
        }
    }

    pub fn cache_write(&self) -> u64 {
        self.cache_write_5m + self.cache_write_1h
    }

    pub fn total_tokens(&self) -> u64 {
        self.input + self.output + self.cache_write() + self.cache_read
    }

    /// Tokens the model read for this request (used for long-context pricing tiers).
    pub fn prompt_tokens(&self) -> u64 {
        self.input + self.cache_write() + self.cache_read
    }

    /// Repo-level project path: worktree paths fold into the repo they belong to.
    pub fn project_path(&self) -> &str {
        project_path(&self.cwd)
    }

    pub fn project_name(&self) -> String {
        project_name(self.project_path())
    }
}

/// `/Users/me/app/.claude/worktrees/x` → `/Users/me/app`. Handles `\` separators too.
pub fn project_path(cwd: &str) -> &str {
    for marker in [
        "/.claude/worktrees/",
        "/.worktrees/",
        "\\.claude\\worktrees\\",
        "\\.worktrees\\",
    ] {
        if let Some(i) = cwd.find(marker) {
            return &cwd[..i];
        }
    }
    cwd
}

/// Last path component, for either separator style.
pub fn project_name(path: &str) -> String {
    if path.is_empty() {
        return "Unknown".into();
    }
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

/// `claude-desktop` → `Desktop app`.
pub fn entrypoint_display_name(raw: &str) -> String {
    match raw {
        "claude-desktop" => "Desktop app".into(),
        "claude-vscode" => "VS Code".into(),
        "cli" | "" => "Terminal CLI".into(),
        "sdk-cli" | "sdk-ts" | "sdk-py" => "Agent SDK".into(),
        other => capitalize_words(&other.replace("claude-", "")),
    }
}

/// Swift's `String.capitalized`: first letter of each word upper-cased, the rest lower-cased.
pub(crate) fn capitalize_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            if start {
                out.extend(ch.to_uppercase());
            } else {
                out.extend(ch.to_lowercase());
            }
            start = false;
        } else {
            out.push(ch);
            start = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktrees_fold_into_repo() {
        assert_eq!(project_path("/Users/me/Code/app/.claude/worktrees/x"), "/Users/me/Code/app");
        assert_eq!(project_path(r"C:\Users\me\app\.claude\worktrees\x"), r"C:\Users\me\app");
        assert_eq!(project_name("/Users/me/Code/app"), "app");
        assert_eq!(project_name(r"C:\Users\me\app"), "app");
        assert_eq!(project_name(""), "Unknown");
    }

    #[test]
    fn entrypoints() {
        assert_eq!(entrypoint_display_name("claude-desktop"), "Desktop app");
        assert_eq!(entrypoint_display_name("claude-jetbrains"), "Jetbrains");
    }
}
