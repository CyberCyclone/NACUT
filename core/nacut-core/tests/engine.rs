//! The engine end to end against the shared fixtures: background start, observer, queries, settings.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nacut_core::alerts::Alert;
use nacut_core::settings::{AccountMode, UsageDisplay};
use nacut_core::{Engine, EngineConfig, EngineObserver, PlanLimit, Settings, StatRange};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shared/fixtures")
}

#[derive(Default)]
struct Counter {
    changes: AtomicUsize,
    alerts: Mutex<Vec<Alert>>,
}

impl EngineObserver for Counter {
    fn on_change(&self) {
        self.changes.fetch_add(1, Ordering::SeqCst);
    }
    fn on_alert(&self, alert: Alert) {
        self.alerts.lock().unwrap().push(alert);
    }
}

fn engine(dir: &tempfile::TempDir, settings: Settings) -> Engine {
    Engine::new(EngineConfig {
        data_dir: dir.path().join("data"),
        settings_path: Some(dir.path().join("settings.json")),
        settings: Some(settings),
        roots_override: Some(vec![fixtures().join("proj")]),
    })
}

#[test]
fn scans_queries_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    e.scan_now();
    let snap = e.snapshot();
    assert_eq!(snap.record_count, 5);
    assert!((snap.insights.all_time - 26.72344).abs() < 1e-9);
    let models = e.model_stats(StatRange::AllTime);
    assert_eq!(models[0].id, "claude-sonnet-5-5");
    assert_eq!(models[0].name, "Sonnet 5.5");
    assert!((e.agent_split(StatRange::AllTime).1 - 12.1).abs() < 1e-9);
    assert_eq!(e.display_name("claude-opus-5-5-20261001"), "Opus 5.5");
    assert_eq!(e.last_days(30).len(), 30);

    // Settings are persisted by the engine when it owns them.
    let mut s = e.settings();
    s.monthly_budget = 50.0;
    e.set_settings(s);
    assert_eq!(Settings::load(&dir.path().join("settings.json")).monthly_budget, 50.0);
    e.stop();

    // The cache survives a restart.
    let e2 = engine(&dir, Settings::default());
    assert_eq!(e2.snapshot().record_count, 5);
}

#[test]
fn background_start_notifies() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    let obs = Arc::new(Counter::default());
    e.set_observer(Some(obs.clone()));
    e.start();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !e.status().has_loaded && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(e.status().has_loaded);
    assert!(obs.changes.load(Ordering::SeqCst) >= 1);
    e.stop();
}

#[test]
fn menu_bar_text_follows_usage_display() {
    let dir = tempfile::tempdir().unwrap();
    let settings = Settings { account_mode: AccountMode::Subscription, limits_enabled: true, ..Settings::default() };
    let e = engine(&dir, settings);
    e.scan_now();
    // No limits yet: falls back to cost.
    assert!(e.status().menu_bar_text.starts_with('$'));
    let data = std::fs::read(fixtures().join("limits.json")).unwrap();
    e.debug_set_limits(PlanLimit::parse(&data));
    let status = e.status();
    assert!(status.shows_limits);
    // limits.json resets in the past relative to now, so both read 0% once expired, or 42%/71% before.
    assert!(status.menu_bar_text.starts_with("5h ") && status.menu_bar_text.contains(" · Week "));
    e.set_usage_display(UsageDisplay::Cost);
    assert!(e.status().menu_bar_text.starts_with('$'));
    assert!(!e.status().shows_limits);
}
