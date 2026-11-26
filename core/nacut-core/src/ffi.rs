//! The FFI surface for the native apps (UniFFI → Swift on macOS, C# on Windows).
//! Built only with the `ffi` feature; the Linux app and CLI use the Rust API directly.

use std::sync::Arc;

use crate::alerts::Alert;
use crate::analytics::{ApiActor, ConnectionStatus};
use crate::engine::{ApiState, Engine, EngineConfig, EngineObserver, Snapshot, StatRange};
use crate::forecast::CurrencyFormat;
use crate::limits::PlanLimit;
use crate::settings::{Settings, UsageDisplay};
use crate::store::{Cal, HourCost, NamedValue};
use crate::views::{self, DayEntry, DayKey, ModelView, ProjectView, SessionView, StatusView};

/// Implemented by the app: called on background threads when data changes or an alert should show.
#[uniffi::export(with_foreign)]
pub trait NacutObserver: Send + Sync {
    fn on_change(&self);
    fn on_alert(&self, alert: Alert);
}

struct Bridge(Arc<dyn NacutObserver>);

impl EngineObserver for Bridge {
    fn on_change(&self) {
        self.0.on_change();
    }
    fn on_alert(&self, alert: Alert) {
        self.0.on_alert(alert);
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AgentSplit {
    pub main: f64,
    pub subagents: f64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct RangeBounds {
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ApiResult {
    pub state: ApiState,
    pub usable: bool,
    pub message: String,
}

impl From<ConnectionStatus> for ApiResult {
    fn from(s: ConnectionStatus) -> Self {
        let state = match &s {
            ConnectionStatus::NotConfigured => ApiState::NotConfigured,
            ConnectionStatus::Connected { .. } => ApiState::Connected,
            ConnectionStatus::NoDataYet => ApiState::NoDataYet,
            ConnectionStatus::Unauthorized(_) => ApiState::Unauthorized,
            ConnectionStatus::Forbidden(_) => ApiState::Forbidden,
            ConnectionStatus::Unavailable(_) => ApiState::Unavailable,
            ConnectionStatus::Failed(_) => ApiState::Failed,
        };
        Self { state, usable: s.is_usable(), message: s.message(&Cal::system()) }
    }
}

#[derive(uniffi::Object)]
pub struct NacutEngine {
    engine: Engine,
}

#[uniffi::export]
impl NacutEngine {
    /// `settings_path`: where the engine persists settings (`None` when the app stores them itself).
    /// `settings`: starting settings (`None`: load from `settings_path` or detect).
    #[uniffi::constructor]
    pub fn new(data_dir: String, settings_path: Option<String>, settings: Option<Settings>) -> Arc<Self> {
        Arc::new(Self {
            engine: Engine::new(EngineConfig {
                data_dir: data_dir.into(),
                settings_path: settings_path.map(Into::into),
                settings,
                roots_override: None,
            }),
        })
    }

    /// Standard folders for this OS, settings persisted by the engine.
    #[uniffi::constructor]
    pub fn standard() -> Arc<Self> {
        Arc::new(Self { engine: Engine::new(EngineConfig::standard()) })
    }

    pub fn set_observer(&self, observer: Option<Arc<dyn NacutObserver>>) {
        self.engine.set_observer(observer.map(|o| Arc::new(Bridge(o)) as Arc<dyn EngineObserver>));
    }

    pub fn start(&self) {
        self.engine.start();
    }
    pub fn stop(&self) {
        self.engine.stop();
    }
    pub fn refresh(&self) {
        self.engine.refresh();
    }
    pub fn refresh_limits(&self) {
        self.engine.refresh_limits();
    }
    pub fn refresh_api(&self, full: bool) {
        self.engine.refresh_api(full);
    }
    pub fn rebuild_cache(&self) {
        self.engine.rebuild_cache();
    }
    /// One blocking scan with no background worker, timers or network (screenshots).
    pub fn scan_now(&self) {
        self.engine.scan_now();
    }

    pub fn snapshot(&self) -> Snapshot {
        self.engine.snapshot()
    }
    pub fn status(&self) -> StatusView {
        self.engine.status()
    }
    pub fn settings(&self) -> Settings {
        self.engine.settings()
    }
    pub fn set_settings(&self, settings: Settings) {
        self.engine.set_settings(settings);
    }
    pub fn set_usage_display(&self, display: UsageDisplay) {
        self.engine.set_usage_display(display);
    }
    pub fn preview_menu_bar_text(&self, settings: Settings) -> String {
        self.engine.preview_menu_bar_text(&settings)
    }
    pub fn log_roots(&self) -> Vec<String> {
        self.engine.log_roots()
    }

    pub fn range_bounds(&self, range: StatRange) -> RangeBounds {
        let (start_ms, end_ms) = self.engine.range_bounds_ms(range);
        RangeBounds { start_ms, end_ms }
    }
    pub fn model_stats(&self, range: StatRange) -> Vec<ModelView> {
        self.engine.model_stats(range)
    }
    pub fn project_stats(&self, range: StatRange) -> Vec<ProjectView> {
        self.engine.project_stats(range)
    }
    pub fn sessions(&self, range: StatRange, limit: u32) -> Vec<SessionView> {
        self.engine.sessions(range, limit)
    }
    pub fn entrypoint_stats(&self, range: StatRange) -> Vec<NamedValue> {
        self.engine.entrypoint_stats(range)
    }
    pub fn agent_split(&self, range: StatRange) -> AgentSplit {
        let (main, subagents) = self.engine.agent_split(range);
        AgentSplit { main, subagents }
    }
    pub fn token_classes(&self, range: StatRange) -> Vec<NamedValue> {
        self.engine.token_classes(range)
    }
    pub fn cache_savings(&self, range: StatRange) -> f64 {
        self.engine.cache_savings(range)
    }
    pub fn weekday_hour_heatmap(&self, range: StatRange) -> Vec<Vec<f64>> {
        self.engine.weekday_hour_heatmap(range)
    }
    pub fn hourly(&self, day: DayKey) -> Vec<HourCost> {
        self.engine.hourly(day)
    }
    pub fn day_models(&self, day: DayKey) -> Vec<ModelView> {
        self.engine.day_models(day)
    }
    pub fn day_sessions(&self, day: DayKey, limit: u32) -> Vec<SessionView> {
        self.engine.day_sessions(day, limit)
    }
    pub fn last_days(&self, count: u32) -> Vec<DayEntry> {
        self.engine.last_days(count)
    }
    pub fn display_name(&self, model: String) -> String {
        self.engine.display_name(&model)
    }

    /// Blocking network call: run off the UI thread.
    pub fn save_api_key(&self, key: String) -> ApiResult {
        self.engine.save_api_key(&key).into()
    }
    pub fn remove_api_key(&self) {
        self.engine.remove_api_key();
    }
    /// Blocking network call: run off the UI thread.
    pub fn test_api(&self) -> ApiResult {
        self.engine.test_api().into()
    }
    /// Blocking network call: run off the UI thread.
    pub fn load_actors(&self) -> Vec<ApiActor> {
        self.engine.load_actors()
    }
    pub fn set_actor(&self, actor: Option<ApiActor>) {
        self.engine.set_actor(actor);
    }

    pub fn send_test_alert(&self) {
        self.engine.send_test_alert();
    }

    /// Replaces plan limits with a saved `/api/oauth/usage` response (screenshots, tests). Returns how many parsed.
    pub fn debug_set_limits_json(&self, json: String) -> u32 {
        let limits = PlanLimit::parse(json.as_bytes());
        let n = limits.len() as u32;
        self.engine.debug_set_limits(limits);
        n
    }
}

/// `$4.21` under $100, `$1,234` above; `compact` turns ≥ $10k into `$12.3k`.
#[uniffi::export]
pub fn format_usd(value: f64, compact: bool) -> String {
    CurrencyFormat::string(value, compact)
}

/// 2–4 decimals, for tooltips.
#[uniffi::export]
pub fn format_usd_exact(value: f64) -> String {
    CurrencyFormat::exact(value)
}

/// `957.2M`
#[uniffi::export]
pub fn format_tokens(count: u64) -> String {
    CurrencyFormat::tokens(count)
}

/// `15%`
#[uniffi::export]
pub fn format_percent(fraction: f64) -> String {
    views::percent(fraction)
}

/// Short chart-axis dollars: `$0`, `$4.50`, `$120`, `$1.5k`.
#[uniffi::export]
pub fn format_usd_axis(value: f64) -> String {
    CurrencyFormat::axis(value)
}

/// Color slot (0–7) of a normalized model id, or `None` for the "other" color.
#[uniffi::export]
pub fn model_slot(id: String) -> Option<u32> {
    views::model_slot(&id)
}

/// Sort key that puts registered models first, in slot order (99 for the rest).
#[uniffi::export]
pub fn model_slot_order(id: String) -> u32 {
    views::model_slot_order(&id)
}

/// "Nice" chart-axis values from 0 to at least `max`, about `count` steps.
#[uniffi::export]
pub fn chart_ticks(max: f64, count: u32) -> Vec<f64> {
    views::chart_ticks(max, count)
}

/// Defaults with account mode and plan detected from `~/.claude.json`.
#[uniffi::export]
pub fn detected_settings() -> Settings {
    Settings::detected()
}

#[uniffi::export]
pub fn default_settings() -> Settings {
    Settings::default()
}

/// `~/Library/Application Support/NACUT`, `%APPDATA%\NACUT` or `~/.local/share/nacut`.
#[uniffi::export]
pub fn standard_data_dir() -> String {
    crate::platform::data_dir().to_string_lossy().into_owned()
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// One row of the bundled price list (USD per million tokens), for the Pricing settings.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ModelPriceView {
    pub id: String,
    pub name: String,
    pub family: String,
    pub rates: crate::pricing::Rates,
    pub has_long_context_tier: bool,
}

/// The bundled price list, newest models first within each family.
#[uniffi::export]
pub fn price_table() -> Vec<ModelPriceView> {
    crate::pricing::PriceBook::bundled()
        .table
        .models
        .iter()
        .map(|m| ModelPriceView {
            id: m.id.clone(),
            name: m.name.clone(),
            family: m.family.clone(),
            rates: m.rates(),
            has_long_context_tier: m.long_context.is_some(),
        })
        .collect()
}

/// `~/.claude/projects`, `~/.config/claude/projects` and `$CLAUDE_CONFIG_DIR/projects`.
#[uniffi::export]
pub fn default_log_roots() -> Vec<String> {
    crate::scanner::LogScanner::default_roots().iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PriceTableInfo {
    /// Date the bundled list was last updated, e.g. `2026-11-25`.
    pub updated: String,
    pub fast_multiplier: f64,
    pub web_search_per_thousand: f64,
}

#[uniffi::export]
pub fn price_table_info() -> PriceTableInfo {
    let t = &crate::pricing::PriceBook::bundled().table;
    PriceTableInfo { updated: t.updated.clone(), fast_multiplier: t.fast_multiplier, web_search_per_thousand: t.web_search_per_thousand }
}

// Runs when nacut-core is built with the `ffi` feature: `cargo test --workspace` (nacut-ffi turns it on for the
// whole workspace build) or `cargo test -p nacut-core --features ffi`.
#[cfg(all(test, feature = "ffi"))]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn data_dir() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").to_string_lossy().into_owned();
        (dir, path)
    }

    /// No roots override exists through FFI, but constructing the engine only computes folder names; nothing is
    /// scanned until `scan_now`/`start`, which these tests never call.
    fn engine() -> (tempfile::TempDir, Arc<NacutEngine>) {
        let (dir, path) = data_dir();
        (dir, NacutEngine::new(path, None, Some(Settings::default())))
    }

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/fixtures").join(name)).unwrap()
    }

    #[test]
    fn currency_wrappers() {
        assert_eq!(format_usd(4.21, false), "$4.21");
        assert_eq!(format_usd(1234.0, false), "$1,234");
        assert_eq!(format_usd(12_345.0, true), "$12.3k");
        assert_eq!(format_usd_exact(0.01844), "$0.0184");
        assert_eq!(format_tokens(957_200_000), "957.2M");
        assert_eq!(format_percent(0.15), "15%");
        assert_eq!(format_percent(-0.0), "0%");
    }

    #[test]
    fn axis_dollars() {
        assert_eq!(format_usd_axis(0.0), "$0");
        assert_eq!(format_usd_axis(4.5), "$4.50");
        assert_eq!(format_usd_axis(120.0), "$120");
        assert_eq!(format_usd_axis(1500.0), "$1.5k");
        assert_eq!(format_usd_axis(-1500.0), "-$1.5k");
        assert_eq!(format_usd_axis(2_300_000.0), "$2.3M");
        assert_eq!(format_usd_axis(f64::NAN), "$0");
    }

    #[test]
    fn model_slots() {
        assert_eq!(model_slot("claude-opus-5-5".into()), Some(0));
        assert_eq!(model_slot("claude-opus-4-8".into()), Some(7));
        assert_eq!(model_slot("claude-opus-5-5-20261001".into()), None, "ids must be normalized first");
        assert_eq!(model_slot("gpt-5".into()), None);
        assert_eq!(model_slot_order("claude-sonnet-5-5".into()), 2);
        assert_eq!(model_slot_order("gpt-5".into()), 99);
    }

    #[test]
    fn ticks() {
        assert_eq!(chart_ticks(10.0, 5), [0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(chart_ticks(0.0, 5), views::chart_ticks(0.0, 5));
    }

    #[test]
    fn price_table_follows_the_bundled_order() {
        let rows = price_table();
        let book = crate::pricing::PriceBook::bundled();
        assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), book.table.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>());
        assert_eq!(rows[0].id, "claude-fable-5-2");
        let opus = rows.iter().find(|r| r.id == "claude-opus-5-5").unwrap();
        assert_eq!((opus.name.as_str(), opus.family.as_str()), ("Opus 5.5", "opus"));
        assert_eq!(opus.rates, crate::pricing::Rates { input: 4.0, output: 20.0, write5m: 5.0, write1h: 8.0, read: 0.2 });
    }

    #[test]
    fn only_haiku_5_5_has_a_long_context_tier() {
        let tiered: Vec<String> = price_table().into_iter().filter(|r| r.has_long_context_tier).map(|r| r.id).collect();
        assert_eq!(tiered, ["claude-haiku-5-5"]);
    }

    #[test]
    fn price_table_info_matches_the_bundled_table() {
        let info = price_table_info();
        assert_eq!(info.updated, "2026-11-25");
        assert_eq!((info.fast_multiplier, info.web_search_per_thousand), (2.0, 10.0));
    }

    #[test]
    fn api_results_for_every_status() {
        let r = |s: ConnectionStatus| {
            let r = ApiResult::from(s);
            (r.state, r.usable)
        };
        assert_eq!(r(ConnectionStatus::NotConfigured), (ApiState::NotConfigured, false));
        assert_eq!(r(ConnectionStatus::Connected { refreshed_at: None }), (ApiState::Connected, true));
        assert_eq!(r(ConnectionStatus::NoDataYet), (ApiState::NoDataYet, true));
        assert_eq!(r(ConnectionStatus::Unauthorized("m".into())), (ApiState::Unauthorized, false));
        assert_eq!(r(ConnectionStatus::Forbidden("m".into())), (ApiState::Forbidden, false));
        assert_eq!(r(ConnectionStatus::Unavailable("m".into())), (ApiState::Unavailable, false));
        assert_eq!(r(ConnectionStatus::Failed("m".into())), (ApiState::Failed, false));
    }

    #[test]
    fn api_result_messages() {
        assert_eq!(ApiResult::from(ConnectionStatus::Failed("offline".into())).message, "Couldn't reach the API: offline");
        assert_eq!(ApiResult::from(ConnectionStatus::Connected { refreshed_at: None }).message, "Connected");
        let at = "2026-11-24T16:02:11Z".parse().unwrap();
        // The time is shown in the local zone, so only the prefix is fixed.
        assert!(ApiResult::from(ConnectionStatus::Connected { refreshed_at: Some(at) }).message.starts_with("Connected · data through Nov 2"));
    }

    #[test]
    fn debug_limits_json_reports_how_many_parsed() {
        let (_dir, e) = engine();
        assert_eq!(e.debug_set_limits_json(fixture("limits-live.json")), 3);
        assert_eq!(e.status().limits.len(), 3);
        assert_eq!(e.debug_set_limits_json(fixture("limits.json")), 2);
        assert_eq!(e.debug_set_limits_json("not json".into()), 0);
        assert!(e.status().limits.is_empty());
    }

    #[test]
    fn engine_wrappers_without_scanning() {
        let (_dir, e) = engine();
        assert_eq!(e.range_bounds(StatRange::AllTime).start_ms, 0);
        let split = e.agent_split(StatRange::AllTime);
        assert_eq!((split.main, split.subagents), (0.0, 0.0));
        assert_eq!(e.snapshot().record_count, 0);
        assert_eq!(e.display_name("claude-opus-5-5-20261001".into()), "Opus 5.5");
        assert_eq!(e.save_api_key("  ".into()).state, ApiState::NotConfigured);
        assert_eq!(e.settings(), Settings::default());
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Alert>>);

    impl NacutObserver for Recorder {
        fn on_change(&self) {}
        fn on_alert(&self, alert: Alert) {
            self.0.lock().unwrap().push(alert);
        }
    }

    #[test]
    fn observer_bridge_forwards_alerts() {
        let (_dir, e) = engine();
        let rec = Arc::new(Recorder::default());
        e.set_observer(Some(rec.clone()));
        e.send_test_alert();
        assert_eq!(rec.0.lock().unwrap().len(), 1);
        e.set_observer(None);
        e.send_test_alert();
        assert_eq!(rec.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn versions_and_defaults() {
        assert_eq!(core_version(), crate::VERSION);
        assert_eq!(default_settings(), Settings::default());
        assert_eq!(standard_data_dir(), crate::platform::data_dir().to_string_lossy());
        assert_eq!(
            default_log_roots(),
            crate::scanner::LogScanner::default_roots().iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>()
        );
    }
}
