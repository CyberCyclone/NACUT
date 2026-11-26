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
    if trimmed.is_empty() {
        return "Unknown".into();
    }
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

    #[test]
    fn plain_worktrees_folder_folds_into_repo() {
        assert_eq!(project_path("/Users/me/app/.worktrees/feature"), "/Users/me/app");
        assert_eq!(project_path(r"C:\Users\me\app\.worktrees\feature"), r"C:\Users\me\app");
    }

    #[test]
    fn worktree_marker_needs_a_separator_after_it() {
        assert_eq!(project_path("/Users/me/app/.claude/worktrees"), "/Users/me/app/.claude/worktrees");
        assert_eq!(project_path("/Users/me/app/my.worktrees/x"), "/Users/me/app/my.worktrees/x");
    }

    #[test]
    fn mixed_separator_worktree_is_not_folded() {
        // Only one separator style per marker is recognised.
        assert_eq!(project_path(r"C:\Users\me\app/.claude\worktrees\x"), r"C:\Users\me\app/.claude\worktrees\x");
    }

    #[test]
    fn project_name_ignores_trailing_separators() {
        assert_eq!(project_name("/Users/me/app/"), "app");
        assert_eq!(project_name(r"C:\Users\me\app\\"), "app");
    }

    #[test]
    fn project_name_handles_mixed_separators() {
        assert_eq!(project_name(r"C:\Users\me/app"), "app");
        assert_eq!(project_name(r"/home/me\app"), "app");
    }

    #[test]
    fn project_name_of_a_bare_name_is_itself() {
        assert_eq!(project_name("app"), "app");
    }

    #[test]
    fn record_project_name_uses_the_folded_path() {
        let mut r = UsageRecord::new("x", Timestamp::UNIX_EPOCH, "m", 0, 0);
        r.cwd = "/Users/me/app/.claude/worktrees/x".into();
        assert_eq!(r.project_path(), "/Users/me/app");
        assert_eq!(r.project_name(), "app");
        r.cwd = String::new();
        assert_eq!(r.project_name(), "Unknown");
    }

    #[test]
    fn known_entrypoints_have_friendly_names() {
        assert_eq!(entrypoint_display_name("cli"), "Terminal CLI");
        assert_eq!(entrypoint_display_name(""), "Terminal CLI");
        assert_eq!(entrypoint_display_name("sdk-cli"), "Agent SDK");
        assert_eq!(entrypoint_display_name("sdk-ts"), "Agent SDK");
        assert_eq!(entrypoint_display_name("sdk-py"), "Agent SDK");
        assert_eq!(entrypoint_display_name("claude-vscode"), "VS Code");
    }

    #[test]
    fn unknown_hyphenated_entrypoints_are_title_cased() {
        assert_eq!(entrypoint_display_name("claude-code-web"), "Code-Web");
        assert_eq!(entrypoint_display_name("remote"), "Remote");
    }

    #[test]
    fn capitalize_words_matches_swift_capitalized() {
        assert_eq!(capitalize_words("hello WORLD"), "Hello World");
        assert_eq!(capitalize_words("a-b_c"), "A-B_C");
        assert_eq!(capitalize_words("x1y 2z"), "X1y 2z");
        assert_eq!(capitalize_words("élan"), "Élan");
        assert_eq!(capitalize_words(""), "");
    }

    #[test]
    fn token_totals_add_up() {
        let mut r = UsageRecord::new("x", Timestamp::UNIX_EPOCH, "m", 1, 2);
        r.cache_write_5m = 4;
        r.cache_write_1h = 8;
        r.cache_read = 16;
        assert_eq!(r.cache_write(), 12);
        assert_eq!(r.prompt_tokens(), 1 + 12 + 16);
        assert_eq!(r.total_tokens(), 1 + 2 + 12 + 16);
    }

    #[test]
    fn new_record_defaults() {
        let r = UsageRecord::new("x", Timestamp::UNIX_EPOCH, "m", 3, 4);
        assert_eq!(r.entrypoint, "cli");
        assert_eq!((r.cache_write_5m, r.cache_write_1h, r.cache_read, r.web_searches), (0, 0, 0, 0));
        assert!(!r.fast && !r.is_subagent && !r.estimated);
        assert_eq!(r.slug, None);
    }

    #[test]
    fn serde_round_trip_drops_cost_and_estimated() {
        let mut r = UsageRecord::new("x", "2026-11-20T01:00:00Z".parse().unwrap(), "claude-opus-5-5", 3, 4);
        r.cost = 1.5;
        r.estimated = true;
        r.slug = Some("brave-otter".into());
        let json = serde_json::to_value(&r).unwrap();
        assert!(json.get("cost").is_none());
        assert!(json.get("estimated").is_none());
        let back: UsageRecord = serde_json::from_value(json).unwrap();
        assert_eq!(back.cost, 0.0);
        assert!(!back.estimated);
        assert_eq!(back, UsageRecord { cost: 0.0, estimated: false, ..r });
    }

    #[test]
    fn missing_optional_fields_take_defaults_on_load() {
        let json = r#"{"id":"x","timestamp":"2026-11-20T01:00:00Z","model":"m","input":1,"output":2,"cache_write_5m":0,"cache_write_1h":0,"cache_read":0}"#;
        let r: UsageRecord = serde_json::from_str(json).unwrap();
        assert_eq!(r.entrypoint, "cli");
        assert_eq!(r.cwd, "");
        assert_eq!(r.session_id, "");
        assert_eq!(r.web_searches, 0);
        assert!(!r.fast && !r.is_subagent);
    }
}
