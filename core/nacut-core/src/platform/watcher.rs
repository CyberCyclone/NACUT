//! Recursive file watching on the transcript folders (FSEvents, ReadDirectoryChangesW or inotify).

use std::path::PathBuf;

use notify::{RecursiveMode, Watcher};

/// Keeps the OS watcher alive; dropping it stops watching.
pub struct TranscriptWatcher {
    _inner: notify::RecommendedWatcher,
}

impl TranscriptWatcher {
    /// Calls `on_change` (on a background thread) whenever a `.jsonl` file under `roots` changes.
    /// Folders that don't exist are skipped. Returns `None` if nothing could be watched.
    pub fn start(roots: &[PathBuf], on_change: impl Fn() + Send + 'static) -> Option<Self> {
        let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if ev.paths.iter().any(|p| p.extension().is_some_and(|e| e == "jsonl")) {
                    on_change();
                }
            }
        })
        .ok()?;
        let mut any = false;
        for r in roots.iter().filter(|r| r.is_dir()) {
            any |= w.watch(r, RecursiveMode::Recursive).is_ok();
        }
        any.then_some(Self { _inner: w })
    }
}
