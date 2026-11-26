//! Small per-user UI preferences (last tab, ranges, Flatpak open-at-login choice) in
//! `~/.local/share/nacut/ui.json`. Nothing sensitive is stored here; the file is still written private (0600).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use nacut_core::engine::StatRange;

thread_local! {
    static VALUES: RefCell<Option<HashMap<String, String>>> = const { RefCell::new(None) };
    static READ_ONLY: Cell<bool> = const { Cell::new(false) };
}

/// Keeps changes in memory only (`--render-to` screenshots must not touch the real ui.json).
pub fn set_read_only() {
    READ_ONLY.with(|r| r.set(true));
}

fn path() -> PathBuf {
    nacut_core::platform::data_dir().join("ui.json")
}

fn with<T>(f: impl FnOnce(&mut HashMap<String, String>) -> T) -> T {
    VALUES.with(|v| {
        let mut v = v.borrow_mut();
        let map = v.get_or_insert_with(|| std::fs::read(path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default());
        f(map)
    })
}

pub fn get(key: &str, fallback: &str) -> String {
    with(|m| m.get(key).cloned().unwrap_or_else(|| fallback.to_string()))
}

pub fn set(key: &str, value: &str) {
    with(|m| {
        m.insert(key.to_string(), value.to_string());
        if READ_ONLY.with(|r| r.get()) {
            return;
        }
        if let Err(e) = nacut_core::platform::write_private(&path(), &serde_json::to_vec(m).unwrap_or_default()) {
            eprintln!("ui.json: {e}");
        }
    });
}

pub fn range(key: &str) -> StatRange {
    parse_range(&get(key, "cycle")).unwrap_or(StatRange::Cycle)
}

/// The inverse of [`range_key`].
pub fn parse_range(s: &str) -> Option<StatRange> {
    match s {
        "cycle" => Some(StatRange::Cycle),
        "last30" => Some(StatRange::Last30),
        "allTime" => Some(StatRange::AllTime),
        _ => None,
    }
}

pub fn range_key(r: StatRange) -> &'static str {
    match r {
        StatRange::Cycle => "cycle",
        StatRange::Last30 => "last30",
        StatRange::AllTime => "allTime",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_keys_round_trip() {
        for r in [StatRange::Cycle, StatRange::Last30, StatRange::AllTime] {
            assert_eq!(parse_range(range_key(r)), Some(r));
        }
    }

    #[test]
    fn range_keys_are_the_core_serde_names() {
        for r in [StatRange::Cycle, StatRange::Last30, StatRange::AllTime] {
            assert_eq!(serde_json::to_value(r).unwrap(), serde_json::Value::String(range_key(r).into()));
        }
    }

    #[test]
    fn unknown_range_keys() {
        for s in ["", "Cycle", "last_30", "alltime", "allTime ", "30"] {
            assert_eq!(parse_range(s), None, "{s:?}");
        }
    }
}
