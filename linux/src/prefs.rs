//! Small per-user UI preferences (last tab, ranges) in `~/.local/share/nacut/ui.json`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use nacut_core::engine::StatRange;

thread_local! {
    static VALUES: RefCell<Option<HashMap<String, String>>> = const { RefCell::new(None) };
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
        let _ = std::fs::create_dir_all(nacut_core::platform::data_dir());
        let _ = std::fs::write(path(), serde_json::to_vec(m).unwrap_or_default());
    });
}

pub fn range(key: &str) -> StatRange {
    match get(key, "cycle").as_str() {
        "last30" => StatRange::Last30,
        "allTime" => StatRange::AllTime,
        _ => StatRange::Cycle,
    }
}

pub fn range_key(r: StatRange) -> &'static str {
    match r {
        StatRange::Cycle => "cycle",
        StatRange::Last30 => "last30",
        StatRange::AllTime => "allTime",
    }
}
