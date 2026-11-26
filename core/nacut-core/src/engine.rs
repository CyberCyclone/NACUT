//! The engine every app drives: owns the history cache and derived state, watches transcript folders,
//! refreshes plan limits and the Usage & Cost API on timers, decides alerts, and answers UI queries.
//! Replaces the data side of the Mac app's `AppModel.swift`.
//!
//! Thread-safe: call it from any thread. Long operations (scans, network) run on a background worker;
//! the host learns about changes through [`EngineObserver::on_change`] and re-reads [`Engine::snapshot`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::alerts::{test_alert, Alert, AlertLedger};
use crate::analytics::{api_epoch, status_for, AnalyticsClient, ApiActor, ApiCostData, ConnectionStatus};
use crate::cache::{LogCache, CACHE_FILE_NAME};
use crate::forecast::{BillingCycle, Forecast};
use crate::insights::{days_ending_at, Insights};
use crate::limits::PlanLimit;
use crate::merge::{Calibration, DataSourceMode, ReconciliationDay, SourceMerger};
use crate::platform::{secrets, watcher::TranscriptWatcher, wsl};
use crate::pricing::PriceBook;
use crate::scanner::LogScanner;
use crate::settings::{Settings, UsageDisplay};
use crate::store::{add_days, Cal, DailySeries, HourCost, Interval, NamedValue, UsageStore};
use crate::views::{self, DayEntry, DayKey, ForecastView, InsightsView, ModelView, ProjectView, SessionView, StatusInputs, StatusView};

const SCAN_EVERY: Duration = Duration::from_secs(60);
const LIMITS_EVERY: Duration = Duration::from_secs(300);
const API_EVERY: Duration = Duration::from_secs(900);
const SAVE_EVERY: Duration = Duration::from_secs(30);
const DEBOUNCE: Duration = Duration::from_millis(1500);
const API_CACHE_FILE: &str = "api-cache-v2.json";
const ALERTS_FILE: &str = "alerts-fired.json";

/// Callbacks from the engine to the app. Called on background threads.
pub trait EngineObserver: Send + Sync {
    /// Something changed: re-read [`Engine::snapshot`] / [`Engine::status`].
    fn on_change(&self);
    /// Show this notification.
    fn on_alert(&self, alert: Alert);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum StatRange {
    Cycle,
    Last30,
    AllTime,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// History cache, API cache and alert ledger.
    pub data_dir: PathBuf,
    /// Where settings are persisted as JSON. `None` when the host stores them (the Mac's UserDefaults).
    pub settings_path: Option<PathBuf>,
    /// Starting settings. `None`: load from `settings_path`, or detect defaults.
    pub settings: Option<Settings>,
    /// Scan only these folders instead of the defaults (tests, `nacut --root`). Extras and WSL are ignored.
    pub roots_override: Option<Vec<PathBuf>>,
}

impl EngineConfig {
    /// Standard folders for this OS, with settings persisted by the engine (Windows and Linux).
    pub fn standard() -> Self {
        Self {
            data_dir: crate::platform::data_dir(),
            settings_path: Some(crate::platform::config_dir().join("settings.json")),
            settings: None,
            roots_override: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum ApiState {
    NotConfigured,
    Connected,
    NoDataYet,
    Unauthorized,
    Forbidden,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ApiView {
    pub has_key: bool,
    pub state: ApiState,
    pub usable: bool,
    pub message: String,
    pub is_fetching: bool,
    /// In hybrid/API mode, data newer than this is a live estimate.
    pub billed_through_ms: Option<i64>,
    pub actors: Vec<ApiActor>,
    pub calibration: Calibration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ReconciliationView {
    pub date: DayKey,
    pub api: f64,
    pub logs: f64,
}

/// Everything the header, overview, calendar and settings need, in one read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Snapshot {
    pub status: StatusView,
    pub forecast: ForecastView,
    pub insights: InsightsView,
    /// Every day with data, oldest first (merged with API data in hybrid/API mode).
    pub series: Vec<DayEntry>,
    pub reconciliation: Vec<ReconciliationView>,
    pub api: ApiView,
    pub settings: Settings,
    pub record_count: u64,
    pub first_record_ms: Option<i64>,
    pub today: DayKey,
    pub busiest_hour_last30: Option<u8>,
    pub cache_savings_cycle: f64,
    /// `Max 5x`, `Pro`…
    pub plan_name: String,
}

enum Cmd {
    Scan,
    Limits,
    Api { full: bool },
    RebuildCache,
    Stop,
}

#[derive(Serialize, Deserialize)]
struct ApiCacheFile {
    actor_id: Option<String>,
    data: ApiCostData,
}

struct State {
    settings: Settings,
    cal: Cal,
    roots: Vec<PathBuf>,
    cache: LogCache,
    cache_dirty: bool,
    last_save: Instant,
    store: UsageStore,
    series: DailySeries,
    forecast: Forecast,
    insights: Insights,
    calibration: Calibration,
    reconciliation: Vec<ReconciliationDay>,
    has_loaded: bool,
    is_scanning: bool,
    last_updated: Option<Timestamp>,
    limits: Vec<PlanLimit>,
    limits_error: Option<String>,
    limits_updated: Option<Timestamp>,
    has_api_key: bool,
    api_status: ConnectionStatus,
    api_data: Option<ApiCostData>,
    api_cache_actor: Option<String>,
    api_actors: Vec<ApiActor>,
    is_fetching_api: bool,
    ledger: AlertLedger,
}

struct Inner {
    state: RwLock<State>,
    observer: RwLock<Option<Arc<dyn EngineObserver>>>,
    tx: Mutex<Option<Sender<Cmd>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    watcher: Mutex<Option<TranscriptWatcher>>,
    scan_lock: Mutex<()>,
    limits_busy: AtomicBool,
    data_dir: PathBuf,
    settings_path: Option<PathBuf>,
    roots_override: Option<Vec<PathBuf>>,
}

pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    /// Loads the cache and settings and computes the initial state. Call [`Engine::start`] to begin scanning.
    pub fn new(config: EngineConfig) -> Self {
        let mut settings = match (&config.settings, &config.settings_path) {
            (Some(s), _) => s.clone(),
            (None, Some(p)) => Settings::load(p),
            (None, None) => Settings::detected(),
        };
        settings.sanitize();
        let cal = Cal::system();
        let cache_path = config.data_dir.join(CACHE_FILE_NAME);
        let mut cache = LogCache::load(&cache_path);
        let mut imported = false;
        if cache.records.is_empty() && !cache_path.exists() {
            // First run of the shared core on a Mac that ran the Swift-only app: keep its history.
            if let Some(old) = LogCache::import_swift_v2(&config.data_dir.join("log-cache.json")) {
                cache = old;
                imported = true;
            }
        }
        let api_file: Option<ApiCacheFile> =
            std::fs::read(config.data_dir.join(API_CACHE_FILE)).ok().and_then(|d| serde_json::from_slice(&d).ok());
        let now = Timestamp::now();
        let forecast = Forecast::new(&DailySeries::new(), 0.0, settings.cycle_start_day, now, &cal);
        let insights = Insights::new(&DailySeries::new(), &forecast.cycle, now, &cal);
        let mut state = State {
            roots: Vec::new(),
            cal: cal.clone(),
            cache,
            cache_dirty: imported,
            last_save: Instant::now(),
            store: UsageStore::new(Vec::new(), PriceBook::bundled().clone(), cal),
            series: DailySeries::new(),
            forecast,
            insights,
            calibration: Calibration::IDENTITY,
            reconciliation: Vec::new(),
            has_loaded: false,
            is_scanning: false,
            last_updated: None,
            limits: Vec::new(),
            limits_error: None,
            limits_updated: None,
            has_api_key: false,
            api_status: ConnectionStatus::NotConfigured,
            api_cache_actor: api_file.as_ref().and_then(|f| f.actor_id.clone()),
            // Caches saved before coverage was clamped can start at the first local record; days before the API's
            // epoch were never fetched, so they must not count as billed $0.
            api_data: api_file.map(|f| ApiCostData { coverage_start: f.data.coverage_start.max(api_epoch()), ..f.data }),
            api_actors: Vec::new(),
            is_fetching_api: false,
            ledger: AlertLedger::load(&config.data_dir.join(ALERTS_FILE)),
            settings,
        };
        state.roots = compute_roots(&state.settings, config.roots_override.as_deref());
        rebuild(&mut state);
        Self {
            inner: Arc::new(Inner {
                state: RwLock::new(state),
                observer: RwLock::new(None),
                tx: Mutex::new(None),
                worker: Mutex::new(None),
                watcher: Mutex::new(None),
                scan_lock: Mutex::new(()),
                limits_busy: AtomicBool::new(false),
                data_dir: config.data_dir,
                settings_path: config.settings_path,
                roots_override: config.roots_override,
            }),
        }
    }

    pub fn set_observer(&self, observer: Option<Arc<dyn EngineObserver>>) {
        *self.inner.observer.write().unwrap() = observer;
    }

    /// Starts the background worker: first scan, file watching, and the refresh timers.
    pub fn start(&self) {
        let mut worker = self.inner.worker.lock().unwrap();
        if worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel::<Cmd>();
        *self.inner.tx.lock().unwrap() = Some(tx);
        let inner = self.inner.clone();
        *worker = Some(
            std::thread::Builder::new()
                .name("nacut-engine".into())
                .spawn(move || run_worker(inner, rx))
                .expect("spawn engine thread"),
        );
    }

    /// Stops the worker and saves the cache.
    pub fn stop(&self) {
        self.send(Cmd::Stop);
        if let Some(h) = self.inner.worker.lock().unwrap().take() {
            let _ = h.join();
        }
        *self.inner.watcher.lock().unwrap() = None;
        save_cache(&self.inner, true);
    }

    fn send(&self, cmd: Cmd) {
        send(&self.inner, cmd);
    }

    /// Rescans transcripts now (debounced).
    pub fn refresh(&self) {
        self.send(Cmd::Scan);
    }

    pub fn refresh_limits(&self) {
        self.send(Cmd::Limits);
    }

    pub fn refresh_api(&self, full: bool) {
        self.send(Cmd::Api { full });
    }

    /// Forgets the cache and reads every transcript again.
    pub fn rebuild_cache(&self) {
        self.send(Cmd::RebuildCache);
    }

    // MARK: Reads

    pub fn snapshot(&self) -> Snapshot {
        let s = self.inner.state.read().unwrap();
        let now = Timestamp::now();
        let today = s.cal.day_of(now);
        let last30 = range_interval(&s, StatRange::Last30);
        Snapshot {
            status: status_of(&s, now),
            forecast: ForecastView::new(&s.forecast, &s.settings, &s.cal),
            insights: (&s.insights).into(),
            series: s.series.iter().map(|(d, c)| DayEntry::new(*d, c, &s.cal)).collect(),
            reconciliation: s.reconciliation.iter().map(|r| ReconciliationView { date: r.date.into(), api: r.api, logs: r.logs }).collect(),
            api: api_view(&s),
            settings: s.settings.clone(),
            record_count: s.store.records.len() as u64,
            first_record_ms: s.store.first_date().map(views::ms),
            today: today.into(),
            busiest_hour_last30: s.store.busiest_hour(last30).map(|h| h as u8),
            cache_savings_cycle: s.store.cache_savings(range_interval(&s, StatRange::Cycle)),
            plan_name: s.settings.plan_name().into(),
        }
    }

    /// Just the menu bar / tray summary (cheap).
    pub fn status(&self) -> StatusView {
        let s = self.inner.state.read().unwrap();
        status_of(&s, Timestamp::now())
    }

    pub fn settings(&self) -> Settings {
        self.inner.state.read().unwrap().settings.clone()
    }

    pub fn range_bounds_ms(&self, range: StatRange) -> (i64, i64) {
        let s = self.inner.state.read().unwrap();
        let i = range_interval(&s, range);
        (views::ms(i.start.max(Timestamp::UNIX_EPOCH)), views::ms(i.end))
    }

    pub fn model_stats(&self, range: StatRange) -> Vec<ModelView> {
        self.query(range, |st, i| st.model_stats(i).iter().map(ModelView::from).collect())
    }

    pub fn project_stats(&self, range: StatRange) -> Vec<ProjectView> {
        self.query(range, |st, i| st.project_stats(i).iter().map(ProjectView::from).collect())
    }

    pub fn sessions(&self, range: StatRange, limit: u32) -> Vec<SessionView> {
        self.query(range, |st, i| st.sessions(i, limit as usize).iter().map(SessionView::from).collect())
    }

    pub fn entrypoint_stats(&self, range: StatRange) -> Vec<NamedValue> {
        self.query(range, |st, i| st.entrypoint_stats(i))
    }

    /// (main agent, subagents)
    pub fn agent_split(&self, range: StatRange) -> (f64, f64) {
        self.query(range, |st, i| st.agent_split(i))
    }

    pub fn token_classes(&self, range: StatRange) -> Vec<NamedValue> {
        self.query(range, |st, i| st.token_classes(i))
    }

    pub fn cache_savings(&self, range: StatRange) -> f64 {
        self.query(range, |st, i| st.cache_savings(i))
    }

    /// 7 × 24, Monday first.
    pub fn weekday_hour_heatmap(&self, range: StatRange) -> Vec<Vec<f64>> {
        self.query(range, |st, i| st.weekday_hour_heatmap(i))
    }

    pub fn hourly(&self, day: DayKey) -> Vec<HourCost> {
        let s = self.inner.state.read().unwrap();
        day.to_date().map(|d| s.store.hourly(d)).unwrap_or_default()
    }

    pub fn day_models(&self, day: DayKey) -> Vec<ModelView> {
        self.day_query(day, |st, i| st.model_stats(i).iter().map(ModelView::from).collect())
    }

    pub fn day_sessions(&self, day: DayKey, limit: u32) -> Vec<SessionView> {
        self.day_query(day, |st, i| st.sessions(i, limit as usize).iter().map(SessionView::from).collect())
    }

    /// The last `count` days ending today, gaps filled with zero.
    pub fn last_days(&self, count: u32) -> Vec<DayEntry> {
        let s = self.inner.state.read().unwrap();
        let today = s.cal.day_of(Timestamp::now());
        days_ending_at(&s.series, today, count as usize).into_iter().map(|(d, c)| DayEntry::new(d, &c, &s.cal)).collect()
    }

    /// `claude-opus-5-5` → `Opus 5.5`
    pub fn display_name(&self, model: &str) -> String {
        self.inner.state.read().unwrap().store.price_book.display_name(model)
    }

    fn query<T>(&self, range: StatRange, f: impl FnOnce(&UsageStore, Interval) -> T) -> T {
        let s = self.inner.state.read().unwrap();
        f(&s.store, range_interval(&s, range))
    }

    fn day_query<T: Default>(&self, day: DayKey, f: impl FnOnce(&UsageStore, Interval) -> T) -> T {
        let s = self.inner.state.read().unwrap();
        match day.to_date() {
            Some(d) => f(&s.store, s.cal.day_interval(d)),
            None => T::default(),
        }
    }

    // MARK: Settings

    /// Applies new settings: persists them (when the engine owns persistence), recomputes, and
    /// starts whatever they turn on (plan limits, the API, new folders).
    pub fn set_settings(&self, mut new: Settings) {
        new.sanitize();
        let (roots_changed, limits_on, api_on) = {
            let mut s = self.inner.state.write().unwrap();
            let old = std::mem::replace(&mut s.settings, new);
            let roots_changed = old.extra_roots != s.settings.extra_roots || old.include_wsl != s.settings.include_wsl;
            if roots_changed {
                s.roots = compute_roots(&s.settings, self.inner.roots_override.as_deref());
            }
            let limits_on = s.settings.limits_enabled && !old.limits_enabled;
            let limits_off = !s.settings.limits_enabled && old.limits_enabled;
            if limits_off {
                s.limits.clear();
                s.limits_error = None;
            }
            let api_on = s.settings.data_source != DataSourceMode::Logs
                && (old.data_source == DataSourceMode::Logs || old.api_actor_id != s.settings.api_actor_id);
            rebuild(&mut s);
            if let Some(p) = &self.inner.settings_path {
                let _ = s.settings.save(p);
            }
            (roots_changed, limits_on, api_on)
        };
        if roots_changed {
            restart_watcher(&self.inner);
            self.send(Cmd::Scan);
        }
        if limits_on {
            self.send(Cmd::Limits);
        }
        if api_on {
            self.send(Cmd::Api { full: true });
        }
        notify(&self.inner);
    }

    /// Switches what a subscription leads with; choosing limits turns limit fetching on.
    pub fn set_usage_display(&self, display: UsageDisplay) {
        let mut s = self.settings();
        s.usage_display = display;
        if display == UsageDisplay::Limits {
            s.limits_enabled = true;
        }
        self.set_settings(s);
    }

    /// The menu bar text for a candidate settings value (for a live Settings preview).
    pub fn preview_menu_bar_text(&self, settings: &Settings) -> String {
        let s = self.inner.state.read().unwrap();
        views::menu_bar_text(&StatusInputs {
            settings,
            forecast: &s.forecast,
            all_time: s.insights.all_time,
            limits: &s.limits,
            now: Timestamp::now(),
            cal: &s.cal,
        })
    }

    /// Transcript folders currently scanned (defaults, extras and WSL).
    pub fn log_roots(&self) -> Vec<String> {
        self.inner.state.read().unwrap().roots.iter().map(|p| p.to_string_lossy().into_owned()).collect()
    }

    // MARK: Usage & Cost API (blocking; call off the UI thread)

    /// Tests a key, stores it if usable, and starts fetching.
    pub fn save_api_key(&self, key: &str) -> ConnectionStatus {
        let key = key.trim();
        if key.is_empty() {
            return ConnectionStatus::NotConfigured;
        }
        let mut status = AnalyticsClient::new(key).test_connection(Timestamp::now());
        let had_key = self.inner.state.read().unwrap().has_api_key;
        // Keep a key the API accepted, or one that couldn't be checked because the network failed
        // (when there's no key yet). A key the API rejected is never stored.
        let unverified = matches!(status, ConnectionStatus::Failed(_));
        if status.is_usable() || (unverified && !had_key) {
            if let Err(e) = secrets::save_analytics_key(key) {
                status = ConnectionStatus::Failed(format!("Couldn't save the key to the system keychain: {e}"));
            }
        }
        {
            let mut s = self.inner.state.write().unwrap();
            s.has_api_key = secrets::analytics_key().is_some();
            s.api_status = status.clone();
        }
        if status.is_usable() {
            let mut settings = self.settings();
            if settings.data_source == DataSourceMode::Logs {
                settings.data_source = DataSourceMode::Hybrid;
                self.set_settings(settings);
            }
            self.load_actors();
            self.send(Cmd::Api { full: true });
        }
        notify(&self.inner);
        status
    }

    pub fn remove_api_key(&self) {
        secrets::delete_analytics_key();
        let _ = std::fs::remove_file(self.inner.data_dir.join(API_CACHE_FILE));
        {
            let mut s = self.inner.state.write().unwrap();
            s.has_api_key = false;
            s.api_status = ConnectionStatus::NotConfigured;
            s.api_data = None;
            s.api_actors.clear();
        }
        let mut settings = self.settings();
        settings.data_source = DataSourceMode::Logs;
        settings.api_actor_id = None;
        settings.api_actor_name = None;
        self.set_settings(settings);
    }

    pub fn test_api(&self) -> ConnectionStatus {
        let status = match secrets::analytics_key() {
            Some(k) => AnalyticsClient::new(k).test_connection(Timestamp::now()),
            None => ConnectionStatus::NotConfigured,
        };
        self.inner.state.write().unwrap().api_status = status.clone();
        notify(&self.inner);
        status
    }

    pub fn load_actors(&self) -> Vec<ApiActor> {
        let Some(key) = secrets::analytics_key() else { return Vec::new() };
        let actors = AnalyticsClient::new(key).actors(Timestamp::now()).unwrap_or_default();
        self.inner.state.write().unwrap().api_actors = actors.clone();
        notify(&self.inner);
        actors
    }

    /// Counts only this person's usage (Enterprise orgs), or everyone with `None`.
    pub fn set_actor(&self, actor: Option<ApiActor>) {
        let mut s = self.settings();
        s.api_actor_id = actor.as_ref().map(|a| a.id.clone());
        s.api_actor_name = actor.map(|a| a.display);
        self.set_settings(s);
    }

    // MARK: Alerts & testing

    pub fn send_test_alert(&self) {
        if let Some(o) = self.inner.observer.read().unwrap().clone() {
            o.on_alert(test_alert());
        }
    }

    /// Replaces plan limits with given data (screenshots and tests).
    pub fn debug_set_limits(&self, limits: Vec<PlanLimit>) {
        {
            let mut s = self.inner.state.write().unwrap();
            s.limits = limits;
            s.limits_error = None;
            s.limits_updated = Some(Timestamp::now());
        }
        notify(&self.inner);
    }

    /// Runs one full scan on the calling thread, without the background worker (CLI, screenshots, tests).
    pub fn scan_now(&self) {
        do_scan(&self.inner);
        self.inner.state.write().unwrap().has_loaded = true;
        notify(&self.inner);
    }

    /// Fetches limits on the calling thread (CLI).
    pub fn fetch_limits_now(&self) {
        do_limits(&self.inner);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) <= 2 {
            self.send(Cmd::Stop);
        }
    }
}

// MARK: Worker

fn send(inner: &Inner, cmd: Cmd) {
    if let Some(tx) = inner.tx.lock().unwrap().as_ref() {
        let _ = tx.send(cmd);
    }
}

fn notify(inner: &Inner) {
    let o = inner.observer.read().unwrap().clone();
    if let Some(o) = o {
        o.on_change();
    }
}

fn post_alerts(inner: &Inner, alerts: Vec<Alert>) {
    if alerts.is_empty() {
        return;
    }
    let o = inner.observer.read().unwrap().clone();
    if let Some(o) = o {
        for a in alerts {
            o.on_alert(a);
        }
    }
}

fn run_worker(inner: Arc<Inner>, rx: mpsc::Receiver<Cmd>) {
    // First load.
    do_scan(&inner);
    inner.state.write().unwrap().has_loaded = true;
    notify(&inner);
    restart_watcher(&inner);
    {
        let has_key = secrets::analytics_key().is_some();
        let mut s = inner.state.write().unwrap();
        s.has_api_key = has_key;
        if has_key && s.api_data.is_some() {
            s.api_status = ConnectionStatus::Connected { refreshed_at: s.api_data.as_ref().and_then(|d| d.refreshed_at) };
        }
    }
    let (limits_enabled, api_enabled) = {
        let s = inner.state.read().unwrap();
        (s.settings.limits_enabled, s.settings.data_source != DataSourceMode::Logs)
    };
    if limits_enabled {
        spawn_limits(&inner);
    }
    if api_enabled {
        spawn_api(&inner, false);
    }

    let mut last_scan = Instant::now();
    let mut last_limits = Instant::now();
    let mut last_api = Instant::now();
    let mut queued: Vec<Cmd> = Vec::new();

    loop {
        let next = [last_scan + SCAN_EVERY, last_limits + LIMITS_EVERY, last_api + API_EVERY].into_iter().min().unwrap();
        let wait = next.saturating_duration_since(Instant::now());
        let cmd = if let Some(c) = queued.pop() { Ok(c) } else { rx.recv_timeout(wait) };
        match cmd {
            Ok(Cmd::Scan) => {
                // Debounce bursts of file events.
                loop {
                    match rx.recv_timeout(DEBOUNCE) {
                        Ok(Cmd::Scan) => continue,
                        Ok(other) => queued.push(other),
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
                do_scan(&inner);
                last_scan = Instant::now();
            }
            Ok(Cmd::Limits) => {
                spawn_limits(&inner);
                last_limits = Instant::now();
            }
            Ok(Cmd::Api { full }) => {
                spawn_api(&inner, full);
                last_api = Instant::now();
            }
            Ok(Cmd::RebuildCache) => {
                {
                    let mut s = inner.state.write().unwrap();
                    s.cache = LogCache::new();
                    s.cache_dirty = true;
                }
                do_scan(&inner);
                save_cache(&inner, true);
                last_scan = Instant::now();
            }
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => {
                save_cache(&inner, true);
                return;
            }
            Err(RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                if now >= last_scan + SCAN_EVERY {
                    do_scan(&inner); // fallback rescan; also rolls "today" over at midnight
                    last_scan = now;
                }
                if now >= last_limits + LIMITS_EVERY {
                    last_limits = now;
                    let (enabled, settings) = {
                        let s = inner.state.read().unwrap();
                        (s.settings.limits_enabled, s.settings.clone())
                    };
                    // Pick up WSL distributions that started since.
                    let roots = compute_roots(&settings, inner.roots_override.as_deref());
                    let changed = {
                        let mut s = inner.state.write().unwrap();
                        let changed = s.roots != roots;
                        s.roots = roots;
                        changed
                    };
                    if changed {
                        restart_watcher(&inner);
                    }
                    if enabled {
                        spawn_limits(&inner);
                    }
                    save_cache(&inner, false);
                }
                if now >= last_api + API_EVERY {
                    last_api = now;
                    if inner.state.read().unwrap().settings.data_source != DataSourceMode::Logs {
                        spawn_api(&inner, false);
                    }
                }
            }
        }
    }
}

fn restart_watcher(inner: &Arc<Inner>) {
    let roots = inner.state.read().unwrap().roots.clone();
    let weak = Arc::downgrade(inner);
    let w = TranscriptWatcher::start(&roots, move || {
        if let Some(i) = weak.upgrade() {
            send(&i, Cmd::Scan);
        }
    });
    *inner.watcher.lock().unwrap() = w;
}

fn compute_roots(settings: &Settings, roots_override: Option<&[PathBuf]>) -> Vec<PathBuf> {
    if let Some(r) = roots_override {
        return r.to_vec();
    }
    let mut roots = LogScanner::default_roots();
    roots.extend(settings.extra_root_paths());
    if cfg!(windows) && settings.include_wsl {
        roots.extend(wsl::transcript_roots());
    }
    let mut seen = std::collections::HashSet::new();
    roots.retain(|r| seen.insert(r.clone()));
    roots
}

fn do_scan(inner: &Inner) {
    let _guard = inner.scan_lock.lock().unwrap();
    let (roots, checkpoints) = {
        let mut s = inner.state.write().unwrap();
        s.is_scanning = true;
        (s.roots.clone(), s.cache.checkpoints.clone())
    };
    let results = LogScanner::new(roots).scan(&checkpoints);
    let alerts = {
        let mut s = inner.state.write().unwrap();
        if !results.is_empty() {
            s.cache.merge(results);
            s.cache_dirty = true; // checkpoints moved even if no new records
        }
        rebuild(&mut s);
        s.is_scanning = false;
        s.last_updated = Some(Timestamp::now());
        evaluate_alerts(&mut s)
    };
    save_cache(inner, false);
    notify(inner);
    post_alerts(inner, alerts);
}

fn evaluate_alerts(s: &mut State) -> Vec<Alert> {
    if !s.has_loaded {
        return Vec::new();
    }
    let State { ledger, forecast, limits, settings, cal, .. } = s;
    ledger.evaluate(forecast, limits, settings, Timestamp::now(), cal)
}

fn save_cache(inner: &Inner, force: bool) {
    let data = {
        let mut s = inner.state.write().unwrap();
        if !s.cache_dirty || (!force && s.last_save.elapsed() < SAVE_EVERY) {
            return;
        }
        s.cache_dirty = false;
        s.last_save = Instant::now();
        serde_json::to_vec(&s.cache)
    };
    if let Ok(bytes) = data {
        write_atomic(&inner.data_dir.join(CACHE_FILE_NAME), &bytes);
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) {
    let _ = crate::platform::write_private(path, bytes);
}

fn spawn_limits(inner: &Arc<Inner>) {
    if inner.limits_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let inner = inner.clone();
    std::thread::spawn(move || {
        do_limits(&inner);
        inner.limits_busy.store(false, Ordering::SeqCst);
    });
}

fn do_limits(inner: &Inner) {
    let (enabled, allow_refresh) = {
        let s = inner.state.read().unwrap();
        (s.settings.limits_enabled, s.settings.auto_refresh_sign_in)
    };
    if !enabled {
        return;
    }
    let result = crate::limits_service::fetch(allow_refresh);
    let alerts = {
        let mut s = inner.state.write().unwrap();
        match result {
            Ok(l) => {
                s.limits = l;
                s.limits_error = None;
                s.limits_updated = Some(Timestamp::now());
                evaluate_alerts(&mut s)
            }
            Err(e) => {
                s.limits_error = Some(e.to_string());
                Vec::new()
            }
        }
    };
    notify(inner);
    post_alerts(inner, alerts);
}

fn spawn_api(inner: &Arc<Inner>, full: bool) {
    {
        let mut s = inner.state.write().unwrap();
        if s.is_fetching_api {
            return;
        }
        s.is_fetching_api = true;
    }
    let inner = inner.clone();
    std::thread::spawn(move || {
        do_api(&inner, full);
        inner.state.write().unwrap().is_fetching_api = false;
        notify(&inner);
    });
}

/// Fetches billed cost. Incremental by default: refreshes the last 40 days, keeping older cached rows.
fn do_api(inner: &Inner, full: bool) {
    let Some(key) = secrets::analytics_key() else {
        let mut s = inner.state.write().unwrap();
        s.has_api_key = false;
        s.api_status = ConnectionStatus::NotConfigured;
        return;
    };
    let now = Timestamp::now();
    let forty = now - SignedDuration::from_hours(40 * 24);
    let (from, incremental, actor) = {
        let s = inner.state.read().unwrap();
        let actor = s.settings.api_actor_id.clone();
        let actor_changed = s.api_data.is_some() && s.api_cache_actor != actor;
        let incremental = !full && !actor_changed && s.api_data.is_some();
        let history_start = s.store.first_date().unwrap_or(now).min(forty);
        (if incremental { forty } else { history_start }, incremental, actor)
    };
    match AnalyticsClient::new(key).hourly_cost(from, now, actor.as_deref()) {
        Ok(fresh) => {
            let mut s = inner.state.write().unwrap();
            let mut rows = fresh.rows;
            let mut coverage_start = fresh.coverage_start;
            if incremental {
                if let Some(old) = &s.api_data {
                    let mut kept: Vec<_> = old.rows.iter().filter(|r| r.start < from).cloned().collect();
                    kept.extend(rows);
                    rows = kept;
                    coverage_start = old.coverage_start.min(from).max(api_epoch());
                }
            }
            let refreshed_at = fresh.refreshed_at.or_else(|| s.api_data.as_ref().and_then(|d| d.refreshed_at));
            let data = ApiCostData { rows, refreshed_at, coverage_start, coverage_end: now, fetched_at: now };
            s.api_status = if fresh.refreshed_at.is_none() && data.rows.is_empty() {
                ConnectionStatus::NoDataYet
            } else {
                ConnectionStatus::Connected { refreshed_at }
            };
            s.has_api_key = true;
            s.api_cache_actor = actor.clone();
            if let Ok(bytes) = serde_json::to_vec(&ApiCacheFile { actor_id: actor, data: data.clone() }) {
                write_atomic(&inner.data_dir.join(API_CACHE_FILE), &bytes);
            }
            s.api_data = Some(data);
            rebuild(&mut s);
        }
        Err(e) => inner.state.write().unwrap().api_status = status_for(&e),
    }
}

// MARK: Derived state

fn rebuild(s: &mut State) {
    let now = Timestamp::now();
    let book = PriceBook::bundled().with_overrides(s.settings.price_overrides.clone());
    s.store = UsageStore::new(s.cache.records.values().cloned().collect(), book, s.cal.clone());
    let merger = SourceMerger::new(s.cal.clone());
    let store = &s.store;
    let (series, calibration) =
        merger.merge(s.settings.data_source, &store.log_series, s.api_data.as_ref(), |day, after| store.day_cost_after(day, after));
    s.reconciliation = s.api_data.as_ref().map(|a| merger.reconciliation(&store.log_series, a, 30)).unwrap_or_default();
    s.series = series;
    s.calibration = calibration;
    s.forecast = Forecast::new(&s.series, s.settings.monthly_budget, s.settings.cycle_start_day, now, &s.cal);
    s.insights = Insights::new(&s.series, &s.forecast.cycle, now, &s.cal);
}

fn range_interval(s: &State, range: StatRange) -> Interval {
    let today = s.cal.day_of(Timestamp::now());
    let end = s.cal.start_of(add_days(today, 1));
    match range {
        StatRange::Cycle => Interval::new(s.cal.start_of(s.forecast.cycle.start), end),
        StatRange::Last30 => Interval::new(s.cal.start_of(add_days(today, -29)), end),
        StatRange::AllTime => Interval::new(Timestamp::MIN, end),
    }
}

fn source_label(s: &State) -> &'static str {
    match s.settings.data_source {
        DataSourceMode::Logs => "local logs",
        DataSourceMode::Hybrid => "hybrid",
        DataSourceMode::Api => "API",
    }
}

fn status_of(s: &State, now: Timestamp) -> StatusView {
    let inp = StatusInputs {
        settings: &s.settings,
        forecast: &s.forecast,
        all_time: s.insights.all_time,
        limits: &s.limits,
        now,
        cal: &s.cal,
    };
    views::status_view(
        &inp,
        s.has_loaded,
        s.is_scanning,
        s.last_updated,
        source_label(s),
        if s.settings.limits_enabled { s.limits_error.clone() } else { None },
        s.limits_updated,
    )
}

fn api_view(s: &State) -> ApiView {
    let state = match &s.api_status {
        ConnectionStatus::NotConfigured => ApiState::NotConfigured,
        ConnectionStatus::Connected { .. } => ApiState::Connected,
        ConnectionStatus::NoDataYet => ApiState::NoDataYet,
        ConnectionStatus::Unauthorized(_) => ApiState::Unauthorized,
        ConnectionStatus::Forbidden(_) => ApiState::Forbidden,
        ConnectionStatus::Unavailable(_) => ApiState::Unavailable,
        ConnectionStatus::Failed(_) => ApiState::Failed,
    };
    ApiView {
        has_key: s.has_api_key,
        state,
        usable: s.api_status.is_usable(),
        message: s.api_status.message(&s.cal),
        is_fetching: s.is_fetching_api,
        billed_through_ms: if s.settings.data_source == DataSourceMode::Logs {
            None
        } else {
            s.api_data.as_ref().and_then(|d| d.refreshed_at).map(views::ms)
        },
        actors: s.api_actors.clone(),
        calibration: s.calibration,
    }
}

/// The billing cycle containing now, for hosts that need it before the first snapshot.
pub fn current_cycle(start_day: u8) -> BillingCycle {
    BillingCycle::containing(Timestamp::now(), start_day, &Cal::system())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_cache_file_shape() {
        let t: Timestamp = "2026-10-30T00:00:00Z".parse().unwrap();
        let file = ApiCacheFile {
            actor_id: Some("u1".into()),
            data: ApiCostData { rows: vec![], refreshed_at: Some(t), coverage_start: t, coverage_end: t, fetched_at: t },
        };
        let v = serde_json::to_value(&file).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "actor_id": "u1",
                "data": {
                    "rows": [],
                    "refreshed_at": "2026-10-30T00:00:00Z",
                    "coverage_start": "2026-10-30T00:00:00Z",
                    "coverage_end": "2026-10-30T00:00:00Z",
                    "fetched_at": "2026-10-30T00:00:00Z"
                }
            })
        );
        let back: ApiCacheFile = serde_json::from_value(v).unwrap();
        assert_eq!(back.data, file.data);
    }

    #[test]
    fn roots_override_ignores_extra_folders() {
        let s = Settings { extra_roots: vec!["/x".into()], ..Settings::default() };
        assert_eq!(compute_roots(&s, Some(&[PathBuf::from("/only")])), [PathBuf::from("/only")]);
        assert!(compute_roots(&s, Some(&[])).is_empty());
    }

    #[test]
    fn extra_folders_follow_the_defaults_once() {
        let s = Settings { extra_roots: vec!["/extra/a".into(), " /extra/a ".into(), "".into()], include_wsl: false, ..Settings::default() };
        let roots = compute_roots(&s, None);
        assert_eq!(roots.len(), LogScanner::default_roots().len() + 1);
        assert_eq!(roots.last(), Some(&PathBuf::from("/extra/a")));
    }
}
