//! View models: plain data the native UIs draw, with no `jiff` types, so they cross the FFI boundary
//! (UniFFI → Swift and C#) unchanged. Timestamps are Unix milliseconds; days are [`DayKey`]s.
//!
//! Wording that every platform shows (menu bar text, ring titles, the pace verdict) is decided here,
//! so the three apps read the same.

use std::collections::HashMap;

use jiff::civil::Date;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::forecast::{month_day, BudgetStatus, CurrencyFormat, Forecast, PointKind};
use crate::insights::Insights;
use crate::limits::PlanLimit;
use crate::settings::{SecondFigure, Settings};
use crate::store::{Cal, CostSource, DayCost, ModelStat, ProjectStat, SessionStat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct DayKey {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl From<Date> for DayKey {
    fn from(d: Date) -> Self {
        Self { year: d.year() as i32, month: d.month() as u8, day: d.day() as u8 }
    }
}

impl DayKey {
    pub fn to_date(self) -> Option<Date> {
        let year = i16::try_from(self.year).ok()?;
        Date::new(year, self.month as i8, self.day as i8).ok()
    }
}

pub fn ms(t: Timestamp) -> i64 {
    t.as_millisecond()
}

/// How urgent something looks: drives tray/menu bar tint and ring colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Level {
    Normal,
    Warning,
    Critical,
}

impl Level {
    /// Plan limits: ≥ 90% critical, ≥ 70% warning.
    pub fn for_utilization(u: f64) -> Self {
        if u >= 0.9 {
            Self::Critical
        } else if u >= 0.7 {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    pub fn for_budget(s: BudgetStatus) -> Self {
        match s {
            BudgetStatus::Exceeded => Self::Critical,
            BudgetStatus::WillExceed | BudgetStatus::Tight => Self::Warning,
            BudgetStatus::NoBudget | BudgetStatus::UnderBudget => Self::Normal,
        }
    }
}

/// `15%`
pub fn percent(f: f64) -> String {
    let f = if f.is_finite() { f } else { 0.0 };
    // `+ 0.0` turns -0.0 (e.g. an empty float sum) into 0.0, so it never prints "-0%".
    format!("{:.0}%", (f * 100.0).round() + 0.0)
}

/// Fixed model → color-slot registry, so a model keeps its color in every view, filter and app. Each app maps a
/// slot to its own palette entry; models not listed here use the apps' "other" color.
pub const MODEL_SLOTS: [&str; 8] = [
    "claude-opus-5-5",
    "claude-fable-5-2",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
    "claude-fable-5-1",
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-opus-4-8",
];

/// Color slot of a normalized model id (e.g. `claude-opus-5-5`), or `None` for the "other" color.
pub fn model_slot(id: &str) -> Option<u32> {
    MODEL_SLOTS.iter().position(|m| *m == id).map(|i| i as u32)
}

/// Sort key that puts registered models first, in slot order.
pub fn model_slot_order(id: &str) -> u32 {
    model_slot(id).unwrap_or(99)
}

/// "Nice" chart-axis values from 0 to at least `max`, about `count` steps apart (steps of 1, 2, 2.5 or 5 × 10ⁿ).
pub fn chart_ticks(max: f64, count: u32) -> Vec<f64> {
    if !max.is_finite() || max <= 0.0 || count == 0 {
        return vec![0.0, 1.0];
    }
    let raw = max / count as f64;
    let exp = raw.log10().floor();
    if !exp.is_finite() {
        return vec![0.0, max];
    }
    let exp = exp as i32;
    // Divide by an exact power of ten for small steps, so 3 × 0.1 is 0.3, not 0.30000000000000004.
    let scale = |k: f64| if exp < 0 { k / 10f64.powi(-exp) } else { k * 10f64.powi(exp) };
    // 20 only matters when `log10` rounds a value just above a power of ten down.
    let Some(m) = [1.0, 2.0, 2.5, 5.0, 10.0, 20.0].into_iter().find(|m| scale(*m) >= raw) else { return vec![0.0, max] };
    if !(scale(m).is_finite() && scale(m) > 0.0) {
        return vec![0.0, max];
    }
    // Stop at the first tick at or above `max`, so the top of the axis always covers it.
    let mut ticks = Vec::new();
    for i in 0.. {
        let v = scale(i as f64 * m);
        if !v.is_finite() {
            ticks.push(max);
            break;
        }
        ticks.push(v);
        if v >= max {
            break;
        }
    }
    ticks
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct DayEntry {
    pub date: DayKey,
    pub start_ms: i64,
    pub cost: f64,
    pub messages: u64,
    pub tokens: u64,
    /// Cost per normalized model id.
    pub by_model: HashMap<String, f64>,
    pub source: CostSource,
}

impl DayEntry {
    pub fn new(date: Date, d: &DayCost, cal: &Cal) -> Self {
        Self {
            date: date.into(),
            start_ms: ms(cal.start_of(date)),
            cost: d.cost,
            messages: d.messages,
            tokens: d.tokens,
            by_model: d.by_model.clone(),
            source: d.source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct LimitView {
    pub id: String,
    /// `5-hour session`, `Weekly · all models`, `Weekly · Fable`
    pub label: String,
    /// `5h`, `Week`, `Fable`
    pub short_label: String,
    /// Ring title: `5-hour session`, `Weekly`, `Fable weekly`
    pub title: String,
    /// 0…1 as of now (0 once the window has reset).
    pub utilization: f64,
    pub resets_at_ms: Option<i64>,
    /// `resets in 4h 35m`
    pub reset_text: String,
    /// Where the limit will be at reset at the current pace (the faint arc).
    pub projected_at_reset: Option<f64>,
    pub projected_hit_ms: Option<i64>,
    /// `full ~Fri 3 PM` if it'll run out before reset, else the reset text.
    pub subtitle: String,
    pub level: Level,
}

impl LimitView {
    pub fn new(l: &PlanLimit, now: Timestamp, cal: &Cal) -> Self {
        let utilization = l.effective_utilization(now);
        let hit = l.projected_hit(now);
        let reset_text = l.reset_text(now, cal);
        let subtitle = match hit {
            Some(h) => format!("full ~{}", h.to_zoned(cal.tz.clone()).strftime("%a %-I %p")),
            None => reset_text.clone(),
        };
        Self {
            id: l.id.clone(),
            label: l.label.clone(),
            short_label: l.short_label(),
            title: ring_title(l),
            utilization,
            resets_at_ms: l.resets_at.map(ms),
            reset_text,
            projected_at_reset: l.projected_at_reset(now),
            projected_hit_ms: hit.map(ms),
            subtitle,
            level: Level::for_utilization(utilization),
        }
    }
}

fn ring_title(l: &PlanLimit) -> String {
    match l.id.as_str() {
        "five_hour" => "5-hour session".into(),
        "seven_day" => "Weekly".into(),
        _ => format!("{} weekly", l.label.replace("Weekly · ", "")),
    }
}

fn limit_name(l: &PlanLimit) -> String {
    match l.id.as_str() {
        "five_hour" => "5-hour limit".into(),
        "seven_day" => "weekly limit".into(),
        _ => format!("{} weekly limit", l.label.replace("Weekly · ", "")),
    }
}

/// One-line verdict for the header: is anything going to run out?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Verdict {
    pub status: BudgetStatus,
    pub level: Level,
    pub text: String,
}

/// Plan limits: will any limit run out before it resets?
pub fn limits_verdict(limits: &[PlanLimit], now: Timestamp, cal: &Cal) -> Verdict {
    let v = |status, text: String| Verdict { status, level: Level::for_budget(status), text };
    if let Some(full) = limits.iter().find(|l| l.effective_utilization(now) >= 1.0) {
        let name = limit_name(full);
        let mut chars = name.chars();
        let name: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
        return v(BudgetStatus::Exceeded, format!("{name} reached · {}", full.reset_text(now, cal)));
    }
    if let Some((l, at)) = limits.iter().filter_map(|l| l.projected_hit(now).map(|h| (l, h))).min_by_key(|(_, h)| *h) {
        let when = at.to_zoned(cal.tz.clone()).strftime("%a %-I:%M %p");
        return v(BudgetStatus::WillExceed, format!("At this pace the {} runs out {when}", limit_name(l)));
    }
    if let Some(p) = limits.iter().find(|l| l.id == "seven_day").and_then(|w| w.projected_at_reset(now)) {
        let status = if p >= 0.8 { BudgetStatus::Tight } else { BudgetStatus::UnderBudget };
        return v(status, format!("On pace for ~{} of your weekly limit by reset", percent(p)));
    }
    v(BudgetStatus::UnderBudget, "Plenty of room in your plan limits".into())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ForecastPointView {
    pub date: DayKey,
    pub start_ms: i64,
    pub cumulative: f64,
    pub kind: PointKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ForecastView {
    pub cycle_start: DayKey,
    /// Exclusive.
    pub cycle_end: DayKey,
    pub cycle_start_ms: i64,
    pub cycle_end_ms: i64,
    /// `API value · Nov 1 – Nov 30` / `Spend · Nov 1 – Nov 30`
    pub cycle_label: String,
    pub budget: f64,
    pub spent: f64,
    pub today: f64,
    pub daily_average: f64,
    pub trailing7_average: f64,
    pub projected: f64,
    pub projected_rate: f64,
    pub elapsed_days: f64,
    pub remaining_days: f64,
    pub total_days: f64,
    pub status: BudgetStatus,
    pub level: Level,
    pub headline: String,
    pub exhaustion_ms: Option<i64>,
    pub safe_daily_spend: Option<f64>,
    pub budget_used_fraction: f64,
    pub projected_fraction: f64,
    pub points: Vec<ForecastPointView>,
}

impl ForecastView {
    pub fn new(f: &Forecast, settings: &Settings, cal: &Cal) -> Self {
        let noun = if settings.is_subscription() { "API value" } else { "Spend" };
        let last = crate::store::add_days(f.cycle.end, -1);
        Self {
            cycle_start: f.cycle.start.into(),
            cycle_end: f.cycle.end.into(),
            cycle_start_ms: ms(cal.start_of(f.cycle.start)),
            cycle_end_ms: ms(cal.start_of(f.cycle.end)),
            cycle_label: format!("{noun} · {} – {}", month_day(f.cycle.start), month_day(last)),
            budget: f.budget,
            spent: f.spent,
            today: f.today,
            daily_average: f.daily_average,
            trailing7_average: f.trailing7_average,
            projected: f.projected,
            projected_rate: f.projected_rate,
            elapsed_days: f.elapsed_days,
            remaining_days: f.remaining_days,
            total_days: f.total_days,
            status: f.status,
            level: Level::for_budget(f.status),
            headline: f.headline(cal),
            exhaustion_ms: f.exhaustion_date.map(ms),
            safe_daily_spend: f.safe_daily_spend,
            budget_used_fraction: f.budget_used_fraction(),
            projected_fraction: f.projected_fraction(),
            points: f
                .points
                .iter()
                .map(|p| ForecastPointView { date: p.date.into(), start_ms: ms(cal.start_of(p.date)), cumulative: p.cumulative, kind: p.kind })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct InsightsView {
    pub all_time: f64,
    pub last7_days: f64,
    pub last30_days: f64,
    pub peak_day: Option<DayKey>,
    pub peak_cost: f64,
    pub active_streak: u32,
    pub active_days_this_cycle: u32,
    pub vs_previous_cycle: Option<f64>,
    pub previous_cycle_total: f64,
}

impl From<&Insights> for InsightsView {
    fn from(i: &Insights) -> Self {
        Self {
            all_time: i.all_time,
            last7_days: i.last7_days,
            last30_days: i.last30_days,
            peak_day: i.peak_day.as_ref().map(|p| p.date.into()),
            peak_cost: i.peak_day.as_ref().map(|p| p.cost).unwrap_or(0.0),
            active_streak: i.active_streak,
            active_days_this_cycle: i.active_days_this_cycle,
            vs_previous_cycle: i.vs_previous_cycle,
            previous_cycle_total: i.previous_cycle_total,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub cost: f64,
    pub messages: u64,
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub tokens: u64,
    pub cost_per_message: f64,
    pub fast_cost: f64,
    pub fast_share: f64,
    pub estimated: bool,
    pub share: f64,
}

impl From<&ModelStat> for ModelView {
    fn from(m: &ModelStat) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            cost: m.cost,
            messages: m.messages,
            input: m.input,
            output: m.output,
            cache_write: m.cache_write,
            cache_read: m.cache_read,
            tokens: m.tokens(),
            cost_per_message: m.cost_per_message(),
            fast_cost: m.fast_cost,
            fast_share: if m.cost > 0.0 { m.fast_cost / m.cost } else { 0.0 },
            estimated: m.estimated,
            share: m.share,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ProjectView {
    pub path: String,
    pub name: String,
    pub cost: f64,
    pub messages: u64,
    pub sessions: u64,
    pub last_active_ms: i64,
    pub share: f64,
}

impl From<&ProjectStat> for ProjectView {
    fn from(p: &ProjectStat) -> Self {
        Self {
            path: p.path.clone(),
            name: p.name.clone(),
            cost: p.cost,
            messages: p.messages,
            sessions: p.sessions,
            last_active_ms: ms(p.last_active),
            share: p.share,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct SessionView {
    pub id: String,
    pub title: String,
    pub project: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub duration_secs: f64,
    pub cost: f64,
    pub messages: u64,
    pub models: Vec<String>,
    pub entrypoint: String,
}

impl From<&SessionStat> for SessionView {
    fn from(s: &SessionStat) -> Self {
        Self {
            id: s.id.clone(),
            title: s.title.clone(),
            project: s.project.clone(),
            start_ms: ms(s.start),
            end_ms: ms(s.end),
            duration_secs: s.duration_secs(),
            cost: s.cost,
            messages: s.messages,
            models: s.models.clone(),
            entrypoint: s.entrypoint.clone(),
        }
    }
}

/// Menu bar / tray summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct StatusView {
    pub has_loaded: bool,
    pub is_scanning: bool,
    pub shows_limits: bool,
    /// `5h 3% · Week 15%` or `$13.25 · $525`.
    pub menu_bar_text: String,
    /// Longer text for tooltips (Windows tray, accessibility).
    pub tooltip: String,
    /// The figure a tray icon or gauge should show (worst limit, or budget used), if any.
    pub gauge_fraction: Option<f64>,
    pub level: Level,
    pub today: f64,
    pub cycle_spent: f64,
    pub all_time: f64,
    pub last_updated_ms: Option<i64>,
    /// `local logs`, `hybrid`, `API`
    pub source_label: String,
    pub limits: Vec<LimitView>,
    pub limits_error: Option<String>,
    pub limits_updated_ms: Option<i64>,
    pub limits_verdict: Option<Verdict>,
}

pub struct StatusInputs<'a> {
    pub settings: &'a Settings,
    pub forecast: &'a Forecast,
    pub all_time: f64,
    pub limits: &'a [PlanLimit],
    pub now: Timestamp,
    pub cal: &'a Cal,
}

/// The menu bar text, shared with each Settings preview.
pub fn menu_bar_text(inp: &StatusInputs) -> String {
    let s = inp.settings;
    let usd = |v| CurrencyFormat::string(v, false);
    if s.shows_limits() {
        let five = inp.limits.iter().find(|l| l.id == "five_hour").or(inp.limits.first());
        if let Some(five) = five {
            let five_text = format!("{} {}", five.short_label(), percent(five.effective_utilization(inp.now)));
            let week = inp.limits.iter().find(|l| l.id == "seven_day" && l.id != five.id);
            return match week {
                Some(w) if !s.compact_menu_bar => format!("{five_text} · {} {}", w.short_label(), percent(w.effective_utilization(inp.now))),
                _ => five_text,
            };
        }
    }
    if s.compact_menu_bar || s.second_figure == SecondFigure::None {
        return usd(inp.forecast.today);
    }
    let second = if s.second_figure == SecondFigure::AllTime { inp.all_time } else { inp.forecast.spent };
    format!("{} · {}", usd(inp.forecast.today), CurrencyFormat::string(second, true))
}

pub fn status_view(inp: &StatusInputs, has_loaded: bool, is_scanning: bool, last_updated: Option<Timestamp>, source_label: &str, limits_error: Option<String>, limits_updated: Option<Timestamp>) -> StatusView {
    let s = inp.settings;
    let f = inp.forecast;
    let shows_limits = s.shows_limits() && !inp.limits.is_empty();
    let worst = inp.limits.iter().map(|l| l.effective_utilization(inp.now)).fold(None, |a: Option<f64>, u| Some(a.map_or(u, |a| a.max(u))));
    let (gauge_fraction, level) = if shows_limits {
        (worst, Level::for_utilization(worst.unwrap_or(0.0)))
    } else if f.budget > 0.0 {
        (Some(f.budget_used_fraction()), Level::for_budget(f.status))
    } else {
        (worst, Level::for_budget(f.status))
    };
    let usd = |v| CurrencyFormat::string(v, false);
    let noun = if s.is_subscription() { "API value" } else { "spend" };
    let tooltip = if shows_limits {
        let parts: Vec<String> = inp
            .limits
            .iter()
            .map(|l| format!("{}: {} ({})", l.label, percent(l.effective_utilization(inp.now)), l.reset_text(inp.now, inp.cal)))
            .collect();
        format!("NACUT\n{}\nToday {} · this cycle {} {noun}", parts.join("\n"), usd(f.today), usd(f.spent))
    } else {
        format!("NACUT\nToday {} · this cycle {} {noun}\n{}", usd(f.today), usd(f.spent), f.headline(inp.cal))
    };
    StatusView {
        has_loaded,
        is_scanning,
        shows_limits,
        menu_bar_text: menu_bar_text(inp),
        tooltip,
        gauge_fraction,
        level,
        today: f.today,
        cycle_spent: f.spent,
        all_time: inp.all_time,
        last_updated_ms: last_updated.map(ms),
        source_label: source_label.to_string(),
        limits: inp.limits.iter().map(|l| LimitView::new(l, inp.now, inp.cal)).collect(),
        limits_error,
        limits_updated_ms: limits_updated.map(ms),
        limits_verdict: (!inp.limits.is_empty()).then(|| limits_verdict(inp.limits, inp.now, inp.cal)),
    }
}
