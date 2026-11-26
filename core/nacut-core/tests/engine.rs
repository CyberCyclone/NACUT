//! The engine end to end against the shared fixtures: background start, observer, queries, settings.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nacut_core::alerts::Alert;
use nacut_core::engine::ApiState;
use nacut_core::merge::DataSourceMode;
use nacut_core::settings::{AccountMode, SecondFigure, UsageDisplay};
use nacut_core::views::DayKey;
use nacut_core::{Engine, EngineConfig, EngineObserver, PlanLimit, Settings, StatRange};

mod common;

use common::fixtures;

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

// MARK: Without the background worker
//
// The engine uses the real clock and the local time zone, so these only assert values that don't depend on
// either: all-time totals, counts, settings, and API days far from any date boundary.

fn engine_with(dir: &tempfile::TempDir, settings: Settings, roots: Vec<std::path::PathBuf>) -> Engine {
    Engine::new(EngineConfig {
        data_dir: dir.path().join("data"),
        settings_path: Some(dir.path().join("settings.json")),
        settings: Some(settings),
        roots_override: Some(roots),
    })
}

fn scanned(dir: &tempfile::TempDir, settings: Settings) -> Engine {
    let e = engine(dir, settings);
    e.scan_now();
    e
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

fn series_total(e: &Engine) -> f64 {
    e.snapshot().series.iter().map(|d| d.cost).sum()
}

#[test]
fn first_run_imports_the_swift_v2_cache() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("log-cache.json"),
        r#"{"version":2,"checkpoints":{},"records":{"m1":{"id":"m1","timestamp":1795429672.5,"model":"claude-opus-5-5","input":1000000,"output":0,"cacheWrite5m":0,"cacheWrite1h":0,"cacheRead":0}}}"#,
    )
    .unwrap();
    let e = engine_with(&dir, Settings::default(), vec![]);
    let snap = e.snapshot();
    assert_eq!(snap.record_count, 1);
    assert_eq!(snap.first_record_ms, Some(1_795_429_672_500));
    close(snap.insights.all_time, 4.0);
    assert!(!data.join("log-cache-v3.json").exists());
    e.stop();
    let saved = nacut_core::LogCache::load(&data.join("log-cache-v3.json"));
    assert_eq!(saved.records.keys().collect::<Vec<_>>(), ["m1"]);
}

#[test]
fn an_existing_v3_cache_skips_the_v2_import() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    nacut_core::LogCache::new().save(&data.join("log-cache-v3.json")).unwrap();
    std::fs::write(
        data.join("log-cache.json"),
        r#"{"version":2,"records":{"m1":{"id":"m1","timestamp":1795429672.5,"model":"claude-opus-5-5","input":1,"output":0,"cacheWrite5m":0,"cacheWrite1h":0,"cacheRead":0}}}"#,
    )
    .unwrap();
    let e = engine_with(&dir, Settings::default(), vec![]);
    assert_eq!(e.snapshot().record_count, 0);
}

/// API cache with $7 on Oct 10 and $3 on Oct 20 (noon UTC, so the same day in every time zone).
fn seed_api_cache(dir: &tempfile::TempDir, actor: Option<&str>) {
    let row = |start: &str, end: &str, amount: f64| {
        nacut_core::analytics::ApiCostRow { start: common::ts(start), end: common::ts(end), model: Some("claude-opus-5-5".into()), amount, list_amount: amount }
    };
    let data = nacut_core::analytics::ApiCostData {
        rows: vec![row("2026-10-10T12:00:00Z", "2026-10-10T13:00:00Z", 7.0), row("2026-10-20T12:00:00Z", "2026-10-20T13:00:00Z", 3.0)],
        refreshed_at: Some(common::ts("2026-10-30T00:00:00Z")),
        coverage_start: common::ts("2026-10-01T00:00:00Z"),
        coverage_end: common::ts("2026-10-31T00:00:00Z"),
        fetched_at: common::ts("2026-10-31T00:00:00Z"),
    };
    let file = serde_json::json!({ "actor_id": actor, "data": data });
    let path = dir.path().join("data").join("api-cache-v2.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&file).unwrap()).unwrap();
}

#[test]
fn api_mode_shows_only_billed_days_from_the_cached_api_data() {
    let dir = tempfile::tempdir().unwrap();
    seed_api_cache(&dir, None);
    let e = scanned(&dir, Settings { data_source: DataSourceMode::Api, ..Settings::default() });
    let snap = e.snapshot();
    // The fixture logs are after the API coverage starts, so only the two API days remain.
    assert_eq!(snap.series.len(), 2);
    close(series_total(&e), 10.0);
    close(snap.insights.all_time, 10.0);
    assert_eq!(snap.api.billed_through_ms, Some(common::ts("2026-10-30T00:00:00Z").as_millisecond()));
    assert!(!snap.api.has_key);
    assert_eq!(snap.api.state, ApiState::NotConfigured);
    assert_eq!(snap.api.message, "Not connected. Using local logs.");
    assert_eq!(snap.status.source_label, "API");
    // Model breakdowns still come from the logs.
    assert_eq!(e.model_stats(StatRange::AllTime)[0].id, "claude-sonnet-5-5");
}

#[test]
fn hybrid_mode_adds_billed_days_to_the_logs() {
    let dir = tempfile::tempdir().unwrap();
    seed_api_cache(&dir, None);
    let e = scanned(&dir, Settings { data_source: DataSourceMode::Hybrid, ..Settings::default() });
    let snap = e.snapshot();
    close(series_total(&e), 10.0 + 26.72344);
    // No day has both billed and logged cost, so nothing to calibrate against.
    assert_eq!(snap.api.calibration.sample_days, 0);
    assert_eq!(snap.api.calibration.ratio, 1.0);
    close(snap.reconciliation.iter().map(|r| r.api).sum(), 10.0);
    close(snap.reconciliation.iter().map(|r| r.logs).sum(), 0.0);
    assert_eq!(snap.api.billed_through_ms, Some(common::ts("2026-10-30T00:00:00Z").as_millisecond()));
    assert_eq!(snap.status.source_label, "hybrid");
}

#[test]
fn logs_mode_ignores_cached_api_data() {
    let dir = tempfile::tempdir().unwrap();
    seed_api_cache(&dir, Some("u1"));
    let e = scanned(&dir, Settings::default());
    let snap = e.snapshot();
    close(series_total(&e), 26.72344);
    assert_eq!(snap.api.billed_through_ms, None);
    assert_eq!(snap.status.source_label, "local logs");
}

#[test]
fn turning_limits_off_clears_them() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings { account_mode: AccountMode::Subscription, limits_enabled: true, ..Settings::default() });
    e.debug_set_limits(PlanLimit::parse(&std::fs::read(fixtures().join("limits-live.json")).unwrap()));
    assert_eq!(e.status().limits.len(), 3);
    e.set_settings(Settings { limits_enabled: false, ..e.settings() });
    let st = e.status();
    assert!(st.limits.is_empty());
    assert_eq!(st.limits_error, None);
    assert_eq!(st.limits_verdict, None);
    // Turning them back on doesn't bring the old values back.
    e.set_settings(Settings { limits_enabled: true, ..e.settings() });
    assert!(e.status().limits.is_empty());
}

#[test]
fn price_overrides_change_costs() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings::default());
    let mut s = e.settings();
    s.price_overrides.insert("claude-sonnet-5-5".into(), nacut_core::Rates::default());
    e.set_settings(s);
    close(e.snapshot().insights.all_time, 26.72344 - 12.1);
    close(e.agent_split(StatRange::AllTime).1, 0.0);
}

#[test]
fn cycle_start_day_moves_the_cycle_range() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings::default());
    e.set_settings(Settings { cycle_start_day: 15, ..e.settings() });
    let (start, end) = e.range_bounds_ms(StatRange::Cycle);
    let start = jiff::Timestamp::from_millisecond(start).unwrap().to_zoned(jiff::tz::TimeZone::system());
    assert_eq!((start.day(), start.hour(), start.minute()), (15, 0, 0));
    assert!(end > start.timestamp().as_millisecond());
    assert_eq!(e.snapshot().forecast.cycle_start_ms, start.timestamp().as_millisecond());
}

#[test]
fn set_settings_sanitizes_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings::default());
    e.set_settings(Settings { cycle_start_day: 0, alert_thresholds: vec![0, 90, 90], plan_price: -1.0, ..e.settings() });
    let s = e.settings();
    assert_eq!((s.cycle_start_day, s.alert_thresholds.clone(), s.plan_price), (1, vec![90], 0.0));
    assert_eq!(Settings::read(&dir.path().join("settings.json")), Some(s));
}

#[test]
fn starting_settings_are_sanitized() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings { cycle_start_day: 31, ..Settings::default() });
    assert_eq!(e.settings().cycle_start_day, 28);
}

#[test]
fn choosing_limits_display_turns_limits_on() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings { account_mode: AccountMode::Subscription, usage_display: UsageDisplay::Cost, ..Settings::default() });
    assert!(!e.settings().limits_enabled);
    e.set_usage_display(UsageDisplay::Limits);
    let s = e.settings();
    assert_eq!((s.usage_display, s.limits_enabled), (UsageDisplay::Limits, true));
    // Choosing cost leaves limit fetching as it was.
    e.set_usage_display(UsageDisplay::Cost);
    assert!(e.settings().limits_enabled);
}

#[test]
fn set_actor_stores_and_clears_the_actor() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    let actor = nacut_core::analytics::ApiActor { id: "u1".into(), kind: "user".into(), display: "a@x.test".into(), cost: 3.0 };
    e.set_actor(Some(actor));
    assert_eq!((e.settings().api_actor_id.as_deref(), e.settings().api_actor_name.as_deref()), (Some("u1"), Some("a@x.test")));
    e.set_actor(None);
    assert_eq!((e.settings().api_actor_id, e.settings().api_actor_name), (None, None));
}

#[test]
fn preview_menu_bar_text_leaves_the_engine_alone() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings::default());
    let before = e.settings();
    let text = e.status().menu_bar_text;
    assert_eq!(e.preview_menu_bar_text(&before), text);
    let all_time = Settings { second_figure: SecondFigure::AllTime, ..before.clone() };
    assert!(e.preview_menu_bar_text(&all_time).ends_with(" · $26.72"), "{}", e.preview_menu_bar_text(&all_time));
    let none = Settings { second_figure: SecondFigure::None, ..before.clone() };
    assert!(!e.preview_menu_bar_text(&none).contains(" · "));
    assert_eq!(e.settings(), before);
    assert_eq!(e.status().menu_bar_text, text);
}

#[test]
fn log_roots_are_the_override() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings { extra_roots: vec!["/elsewhere".into()], ..Settings::default() });
    assert_eq!(e.log_roots(), [fixtures().join("proj").to_string_lossy().into_owned()]);
}

#[test]
fn all_time_range_starts_at_the_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    let (start, end) = e.range_bounds_ms(StatRange::AllTime);
    assert_eq!(start, 0);
    assert_eq!(e.range_bounds_ms(StatRange::Last30).1, end);
    assert_eq!(e.range_bounds_ms(StatRange::Cycle).1, end);
}

#[test]
fn invalid_day_keys_give_empty_results() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings::default());
    let bad = DayKey { year: 2026, month: 13, day: 1 };
    assert!(e.hourly(bad).is_empty());
    assert!(e.day_models(bad).is_empty());
    assert!(e.day_sessions(bad, 10).is_empty());
    let feb30 = DayKey { year: 2026, month: 2, day: 30 };
    assert!(e.hourly(feb30).is_empty());
}

fn transcript_root(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let root = dir.path().join("logs");
    std::fs::create_dir_all(&root).unwrap();
    let line = |session: &str, id: &str, input: u64| {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-11-10T12:00:00Z","sessionId":"{session}","message":{{"id":"{id}","model":"claude-opus-5-5","usage":{{"input_tokens":{input}}}}}}}"#
        )
    };
    let text = [line("a", "m1", 1_000_000), line("b", "m2", 3_000_000), line("c", "m3", 2_000_000), line("a", "m4", 500_000)].join("\n");
    std::fs::write(root.join("s.jsonl"), text + "\n").unwrap();
    root
}

#[test]
fn sessions_are_sorted_by_cost_and_limited() {
    let dir = tempfile::tempdir().unwrap();
    let root = transcript_root(&dir);
    let e = engine_with(&dir, Settings::default(), vec![root]);
    e.scan_now();
    let all = e.sessions(StatRange::AllTime, 10);
    assert_eq!(all.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
    close(all[2].cost, 6.0);
    assert_eq!(all[2].messages, 2);
    assert_eq!(e.sessions(StatRange::AllTime, 2).len(), 2);
    assert!(e.sessions(StatRange::AllTime, 0).is_empty());
}

#[test]
fn breakdowns_pass_through_from_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings::default());
    let entry = e.entrypoint_stats(StatRange::AllTime);
    assert_eq!(entry.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(), ["Terminal CLI", "Desktop app"]);
    close(entry[0].value, 2.575 + 4.03 + 12.1);
    close(entry[1].value, 0.01844 + 8.0);
    let classes = e.token_classes(StatRange::AllTime);
    assert_eq!(classes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(), nacut_core::store::TOKEN_CLASSES);
    close(classes.iter().map(|c| c.value).sum::<f64>() + 0.03, 26.72344);
    let heat = e.weekday_hour_heatmap(StatRange::AllTime);
    assert_eq!((heat.len(), heat[0].len()), (7, 24));
    close(heat.iter().flatten().sum(), 26.72344);
    assert!(e.cache_savings(StatRange::AllTime) > 0.0);
    assert_eq!(e.project_stats(StatRange::AllTime).len(), 1);
}

#[test]
fn snapshot_basics() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings { account_mode: AccountMode::Subscription, plan_price: 200.0, ..Settings::default() });
    let snap = e.snapshot();
    assert_eq!(snap.plan_name, "Max 20x");
    assert_eq!(snap.first_record_ms, Some(common::ts("2026-11-20T01:00:06Z").as_millisecond()));
    assert!(snap.status.has_loaded);
    assert!(!snap.status.is_scanning);
    assert!(snap.status.last_updated_ms.is_some());
    assert_eq!(snap.settings, e.settings());
}

#[test]
fn an_empty_key_is_not_configured() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    assert_eq!(e.save_api_key("   "), nacut_core::analytics::ConnectionStatus::NotConfigured);
    assert_eq!(e.save_api_key(""), nacut_core::analytics::ConnectionStatus::NotConfigured);
}

#[test]
fn test_alert_reaches_the_observer() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    let obs = Arc::new(Counter::default());
    e.set_observer(Some(obs.clone()));
    e.send_test_alert();
    let alerts = obs.alerts.lock().unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].title, "NACUT alerts are on");
    assert!(alerts[0].id.starts_with("test-"));
}

#[test]
fn no_observer_is_fine() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    e.send_test_alert();
    e.scan_now();
    e.set_observer(None);
    e.scan_now();
}

#[test]
fn scans_and_settings_changes_notify_the_observer() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir, Settings::default());
    let obs = Arc::new(Counter::default());
    e.set_observer(Some(obs.clone()));
    e.scan_now();
    let after_scan = obs.changes.load(Ordering::SeqCst);
    assert!(after_scan >= 1);
    e.set_settings(e.settings());
    assert_eq!(obs.changes.load(Ordering::SeqCst), after_scan + 1);
}

#[test]
fn stop_without_start_saves_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let e = scanned(&dir, Settings::default());
    let path = dir.path().join("data").join("log-cache-v3.json");
    assert!(!path.exists(), "scans save at most every 30 s");
    e.stop();
    assert_eq!(nacut_core::LogCache::load(&path).records.len(), 5);
    // A second stop is harmless.
    e.stop();
}

#[test]
fn limit_alerts_fire_once_through_the_observer() {
    let dir = tempfile::tempdir().unwrap();
    let settings = Settings { account_mode: AccountMode::Subscription, limits_enabled: true, ..Settings::default() };
    let e = engine(&dir, settings);
    let obs = Arc::new(Counter::default());
    e.set_observer(Some(obs.clone()));
    let resets_at = jiff::Timestamp::now() + jiff::SignedDuration::from_hours(3);
    e.debug_set_limits(vec![PlanLimit { id: "five_hour".into(), label: "5-hour session".into(), utilization: 0.95, resets_at: Some(resets_at) }]);
    // The first scan only loads; alerts are evaluated from the next one.
    e.scan_now();
    assert!(obs.alerts.lock().unwrap().is_empty());
    e.scan_now();
    {
        let alerts = obs.alerts.lock().unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].id, format!("limit-five_hour-{resets_at}"));
        assert_eq!(alerts[0].title, "5-hour session limit at 95%");
        assert!(alerts[0].body.starts_with("Plan limit resets in 2h 5") || alerts[0].body.starts_with("Plan limit resets in 3h 0m"), "{}", alerts[0].body);
    }
    e.scan_now();
    assert_eq!(obs.alerts.lock().unwrap().len(), 1);
    assert!(dir.path().join("data").join("alerts-fired.json").exists());
}

#[test]
fn limit_alerts_respect_the_setting() {
    let dir = tempfile::tempdir().unwrap();
    let settings = Settings { account_mode: AccountMode::Subscription, limits_enabled: true, limit_alerts: false, ..Settings::default() };
    let e = engine(&dir, settings);
    let obs = Arc::new(Counter::default());
    e.set_observer(Some(obs.clone()));
    let resets_at = jiff::Timestamp::now() + jiff::SignedDuration::from_hours(3);
    e.debug_set_limits(vec![PlanLimit { id: "five_hour".into(), label: "5-hour session".into(), utilization: 0.95, resets_at: Some(resets_at) }]);
    e.scan_now();
    e.scan_now();
    assert!(obs.alerts.lock().unwrap().is_empty());
}
