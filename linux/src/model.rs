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
        let config = if render_mode { crate::render::engine_config() } else { EngineConfig::standard() };
        let engine = Arc::new(Engine::new(config));
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

pub fn pct(f: f64) -> String {
    views::percent(f)
}

pub fn tokens(n: u64) -> String {
    CurrencyFormat::tokens(n)
}

/// A Unix-milliseconds timestamp in `tz` (out-of-range values show as the epoch).
fn local(ms: i64, tz: &TimeZone) -> Zoned {
    Timestamp::from_millisecond(ms).unwrap_or(Timestamp::UNIX_EPOCH).to_zoned(tz.clone())
}

/// strftime on a Unix-milliseconds timestamp in local time, e.g. `fmt(ms, "%b %-d")`.
pub fn fmt(ms: i64, pattern: &str) -> String {
    fmt_in(ms, pattern, &TimeZone::system())
}

fn fmt_in(ms: i64, pattern: &str, tz: &TimeZone) -> String {
    local(ms, tz).strftime(pattern).to_string()
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
    relative_at(Timestamp::now().as_millisecond(), ms, &TimeZone::system())
}

/// `just now`, `5 min ago`, `3 hr ago`, `yesterday`, `12 days ago`, then the date in `tz`. Future times are "just now".
pub fn relative_at(now_ms: i64, ms: i64, tz: &TimeZone) -> String {
    match elapsed_secs(now_ms, ms) {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} hr ago", s / 3600),
        s if s < 2 * 86_400 => "yesterday".into(),
        s if s < 30 * 86_400 => format!("{} days ago", s / 86_400),
        _ => fmt_in(ms, "%b %-d", tz),
    }
}

/// Panel footer: `Updated just now`, `Updated 12m ago` (minutes only, like the Mac), or `Loading…` before the first scan.
pub fn updated_text(now_ms: i64, last_updated_ms: Option<i64>) -> String {
    match last_updated_ms.map(|ms| elapsed_secs(now_ms, ms)) {
        Some(s) if s < 60 => "Updated just now".into(),
        Some(s) => format!("Updated {}m ago", s / 60),
        None => "Loading…".into(),
    }
}

fn elapsed_secs(now_ms: i64, ms: i64) -> i64 {
    now_ms.saturating_sub(ms) / 1000
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

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;
    /// 2026-11-26T12:00:00Z
    const NOW: i64 = 1_795_694_400_000;

    fn rel(ago_ms: i64) -> String {
        relative_at(NOW, NOW - ago_ms, &TimeZone::UTC)
    }

    #[test]
    fn now_constant_is_what_it_says() {
        assert_eq!(fmt_in(NOW, "%Y-%m-%d %H:%M", &TimeZone::UTC), "2026-11-26 12:00");
    }

    #[test]
    fn relative_buckets() {
        assert_eq!(rel(0), "just now");
        assert_eq!(rel(59_999), "just now");
        assert_eq!(rel(MIN), "1 min ago");
        assert_eq!(rel(59 * MIN + 59_999), "59 min ago");
        assert_eq!(rel(HOUR), "1 hr ago");
        assert_eq!(rel(DAY - 1), "23 hr ago");
        assert_eq!(rel(DAY), "yesterday");
        assert_eq!(rel(2 * DAY - 1), "yesterday");
        assert_eq!(rel(2 * DAY), "2 days ago");
        assert_eq!(rel(30 * DAY - 1), "29 days ago");
        assert_eq!(rel(30 * DAY), "Oct 27");
        assert_eq!(rel(400 * DAY), "Oct 22");
    }

    #[test]
    fn relative_future_and_extremes() {
        assert_eq!(rel(-5 * MIN), "just now");
        assert_eq!(relative_at(NOW, i64::MAX, &TimeZone::UTC), "just now");
        // Out of jiff's range: shown as the epoch rather than panicking or overflowing.
        assert_eq!(relative_at(NOW, i64::MIN, &TimeZone::UTC), "Jan 1");
        assert_eq!(relative_at(i64::MAX, 0, &TimeZone::UTC), "Jan 1");
    }

    #[test]
    fn relative_date_uses_the_time_zone() {
        let tz = TimeZone::fixed(jiff::tz::offset(-14));
        // 2026-10-01T05:00Z is still Sep 30 at UTC-14.
        let ms = 1_790_830_800_000;
        assert_eq!(fmt_in(ms, "%Y-%m-%d %H:%M", &TimeZone::UTC), "2026-10-01 05:00");
        assert_eq!(relative_at(NOW, ms, &tz), "Sep 30");
        assert_eq!(relative_at(NOW, ms, &TimeZone::UTC), "Oct 1");
    }

    #[test]
    fn footer_updated_text() {
        assert_eq!(updated_text(NOW, None), "Loading…");
        assert_eq!(updated_text(NOW, Some(NOW)), "Updated just now");
        assert_eq!(updated_text(NOW, Some(NOW + HOUR)), "Updated just now");
        assert_eq!(updated_text(NOW, Some(NOW - 59_999)), "Updated just now");
        assert_eq!(updated_text(NOW, Some(NOW - MIN)), "Updated 1m ago");
        assert_eq!(updated_text(NOW, Some(NOW - 3 * HOUR)), "Updated 180m ago");
        assert!(updated_text(NOW, Some(i64::MIN)).starts_with("Updated "));
    }

    #[test]
    fn fmt_patterns() {
        assert_eq!(fmt_in(NOW, "%b %-d", &TimeZone::UTC), "Nov 26");
        assert_eq!(fmt_in(NOW, "%a %b %-d", &TimeZone::UTC), "Thu Nov 26");
        assert_eq!(fmt_in(NOW + 3 * HOUR + 5 * MIN, "%b %-d, %-I:%M %p", &TimeZone::UTC), "Nov 26, 3:05 PM");
        assert_eq!(fmt_in(0, "%Y-%m-%d", &TimeZone::UTC), "1970-01-01");
    }

    #[test]
    fn hour_labels() {
        let labels: Vec<String> = (0..24).map(hour_label).collect();
        assert_eq!(labels[0], "12 am");
        assert_eq!(labels[1], "1 am");
        assert_eq!(labels[6], "6 am");
        assert_eq!(labels[11], "11 am");
        assert_eq!(labels[12], "12 pm");
        assert_eq!(labels[13], "1 pm");
        assert_eq!(labels[18], "6 pm");
        assert_eq!(labels[23], "11 pm");
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 24);
    }

    #[test]
    fn day_key_round_trip() {
        for (y, m, d) in [(2024, 2, 29), (1999, 12, 31), (2026, 1, 1), (2026, 11, 26), (2100, 2, 28)] {
            let date = jiff::civil::date(y, m, d);
            let key = key_of(date);
            assert_eq!(key, DayKey { year: y as i32, month: m as u8, day: d as u8 });
            assert_eq!(key, DayKey::from(date));
            assert_eq!(date_of(&key), date);
        }
    }

    #[test]
    fn titles() {
        assert_eq!(range_title(StatRange::Cycle), "This cycle");
        assert_eq!(range_title(StatRange::Last30), "30 days");
        assert_eq!(range_title(StatRange::AllTime), "All time");
        assert_eq!(account_title(AccountMode::Subscription), "Max / Pro subscription");
        assert_eq!(account_title(AccountMode::Api), "API pay-as-you-go");
        assert_eq!(display_title(UsageDisplay::Limits), "Plan limits");
        assert_eq!(display_title(UsageDisplay::Cost), "API value");
        assert_eq!(second_title(SecondFigure::Cycle), "This cycle");
        assert_eq!(second_title(SecondFigure::AllTime), "All time");
        assert_eq!(second_title(SecondFigure::None), "Hidden");
        assert_eq!(source_title(DataSourceMode::Logs), "Local logs");
        assert_eq!(source_title(DataSourceMode::Api), "Anthropic API only");
        assert_eq!(source_title(DataSourceMode::Hybrid), "Hybrid (API + live logs)");
    }
}
