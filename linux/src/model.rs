//! The app's view of the shared core engine: republishes snapshots on the GTK main thread.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use jiff::{tz::TimeZone, Timestamp, Zoned};
use nacut_core::alerts::Alert;
use nacut_core::engine::{Engine, EngineConfig, EngineObserver, Snapshot, StatRange};
use nacut_core::forecast::CurrencyFormat;
use nacut_core::merge::DataSourceMode;
use nacut_core::settings::{AccountMode, SecondFigure, Settings, UsageDisplay};
use nacut_core::views::{self, DayKey};

/// Messages from the engine's background threads to the main loop.
pub enum CoreEvent {
    Changed,
    Alert(Alert),
}

struct Bridge(async_channel::Sender<CoreEvent>);

impl EngineObserver for Bridge {
    fn on_change(&self) {
        let _ = self.0.send_blocking(CoreEvent::Changed);
    }
    fn on_alert(&self, alert: Alert) {
        let _ = self.0.send_blocking(CoreEvent::Alert(alert));
    }
}

type Listener = Box<dyn Fn()>;

/// Shared by every view (main thread only).
pub struct AppModel {
    pub engine: Arc<Engine>,
    snap: RefCell<Snapshot>,
    listeners: RefCell<Vec<Listener>>,
    reload_scheduled: Cell<bool>,
}

impl AppModel {
    /// `render_mode`: one scan, no background work, network or keyring access (screenshots).
    pub fn new(render_mode: bool) -> (Rc<Self>, async_channel::Receiver<CoreEvent>) {
        let engine = Arc::new(Engine::new(EngineConfig::standard()));
        let (tx, rx) = async_channel::unbounded();
        engine.set_observer(Some(Arc::new(Bridge(tx))));
        if render_mode {
            engine.scan_now();
        } else {
            engine.start();
        }
        let snap = engine.snapshot();
        let model = Rc::new(Self { engine, snap: RefCell::new(snap), listeners: RefCell::new(Vec::new()), reload_scheduled: Cell::new(false) });
        (model, rx)
    }

    /// Coalesces bursts of change notifications into one snapshot read.
    pub fn schedule_reload(self: &Rc<Self>) {
        if self.reload_scheduled.replace(true) {
            return;
        }
        let me = self.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            me.reload_scheduled.set(false);
            me.reload();
        });
    }

    pub fn reload(&self) {
        *self.snap.borrow_mut() = self.engine.snapshot();
        for l in self.listeners.borrow().iter() {
            l();
        }
    }

    /// Called on the main thread after every snapshot change.
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    pub fn snap(&self) -> std::cell::Ref<'_, Snapshot> {
        self.snap.borrow()
    }

    pub fn settings(&self) -> Settings {
        self.snap.borrow().settings.clone()
    }

    pub fn is_subscription(&self) -> bool {
        self.snap.borrow().settings.account_mode == AccountMode::Subscription
    }

    pub fn update(&self, change: impl FnOnce(&mut Settings)) {
        let mut s = self.engine.settings();
        change(&mut s);
        self.engine.set_settings(s);
        self.reload();
    }

    pub fn set_usage_display(&self, display: UsageDisplay) {
        self.update(|s| {
            s.usage_display = display;
            if display == UsageDisplay::Limits {
                s.limits_enabled = true;
            }
        });
    }

    pub fn refresh_all(&self) {
        self.engine.refresh();
        let s = self.engine.settings();
        if s.limits_enabled {
            self.engine.refresh_limits();
        }
        if s.data_source != DataSourceMode::Logs {
            self.engine.refresh_api(false);
        }
    }

    /// Runs a blocking engine call on a worker thread, then `done` on the main thread.
    pub fn background<T: Send + 'static>(self: &Rc<Self>, work: impl FnOnce(&Engine) -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
        let engine = self.engine.clone();
        let me = self.clone();
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(work(&engine));
        });
        gtk::glib::spawn_future_local(async move {
            if let Ok(v) = rx.recv().await {
                done(v);
                me.reload();
            }
        });
    }
}

// MARK: Formatting (the same helpers every platform uses)

pub fn usd(v: f64) -> String {
    CurrencyFormat::string(v, false)
}

pub fn usd_exact(v: f64) -> String {
    CurrencyFormat::exact(v)
}

pub fn usd_compact(v: f64) -> String {
    if v == 0.0 {
        "$0".into()
    } else if v < 10.0 {
        usd(v)
    } else if v < 1000.0 {
        format!("${}", v.round() as i64)
    } else {
        format!("${:.1}k", v / 1000.0)
    }
}

pub fn pct(f: f64) -> String {
    views::percent(f)
}

pub fn tokens(n: u64) -> String {
    CurrencyFormat::tokens(n)
}

pub fn local(ms: i64) -> Zoned {
    Timestamp::from_millisecond(ms).unwrap_or(Timestamp::UNIX_EPOCH).to_zoned(TimeZone::system())
}

/// strftime on a Unix-milliseconds timestamp in local time, e.g. `fmt(ms, "%b %-d")`.
pub fn fmt(ms: i64, pattern: &str) -> String {
    local(ms).strftime(pattern).to_string()
}

pub fn today() -> jiff::civil::Date {
    Zoned::now().date()
}

pub fn date_of(k: &DayKey) -> jiff::civil::Date {
    jiff::civil::date(k.year as i16, k.month as i8, k.day as i8)
}

pub fn key_of(d: jiff::civil::Date) -> DayKey {
    DayKey { year: d.year() as i32, month: d.month() as u8, day: d.day() as u8 }
}

pub fn hour_label(h: u32) -> String {
    match h {
        0 => "12 am".into(),
        12 => "12 pm".into(),
        h if h < 12 => format!("{h} am"),
        h => format!("{} pm", h - 12),
    }
}

pub fn relative(ms: i64) -> String {
    let secs = (Timestamp::now().as_millisecond() - ms) / 1000;
    match secs {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} hr ago", s / 3600),
        s if s < 2 * 86_400 => "yesterday".into(),
        s if s < 30 * 86_400 => format!("{} days ago", s / 86_400),
        _ => fmt(ms, "%b %-d"),
    }
}

pub fn range_title(r: StatRange) -> &'static str {
    match r {
        StatRange::Cycle => "This cycle",
        StatRange::Last30 => "30 days",
        StatRange::AllTime => "All time",
    }
}

pub fn account_title(m: AccountMode) -> &'static str {
    match m {
        AccountMode::Subscription => "Max / Pro subscription",
        AccountMode::Api => "API pay-as-you-go",
    }
}

pub fn display_title(d: UsageDisplay) -> &'static str {
    match d {
        UsageDisplay::Limits => "Plan limits",
        UsageDisplay::Cost => "API value",
    }
}

pub fn second_title(f: SecondFigure) -> &'static str {
    match f {
        SecondFigure::Cycle => "This cycle",
        SecondFigure::AllTime => "All time",
        SecondFigure::None => "Hidden",
    }
}

pub fn source_title(m: DataSourceMode) -> &'static str {
    match m {
        DataSourceMode::Logs => "Local logs",
        DataSourceMode::Api => "Anthropic API only",
        DataSourceMode::Hybrid => "Hybrid (API + live logs)",
    }
}
