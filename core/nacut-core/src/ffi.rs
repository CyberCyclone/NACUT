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
