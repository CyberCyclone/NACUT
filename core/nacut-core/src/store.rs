//! In-memory index over priced records. Port of `UsageStore.swift`.

use std::collections::{BTreeMap, HashMap, HashSet};

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};

use crate::pricing::PriceBook;
use crate::record::{entrypoint_display_name, project_name, UsageRecord};

/// Where a day's number came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum CostSource {
    /// Computed from local transcripts at list price.
    #[default]
    Logs,
    /// Billed amount from the Usage & Cost API.
    Api,
    /// Local logs scaled by the API calibration ratio.
    Estimated,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DayCost {
    pub cost: f64,
    /// Cost per normalized model id.
    pub by_model: HashMap<String, f64>,
    pub messages: u64,
    pub tokens: u64,
    pub source: CostSource,
}

impl DayCost {
    pub fn with_cost(cost: f64) -> Self {
        Self { cost, ..Default::default() }
    }
}

/// Daily cost keyed by local calendar date.
pub type DailySeries = BTreeMap<Date, DayCost>;

/// A half-open time range `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: Timestamp,
    pub end: Timestamp,
}

impl Interval {
    pub fn new(start: Timestamp, end: Timestamp) -> Self {
        Self { start, end }
    }
    pub fn contains(&self, t: Timestamp) -> bool {
        t >= self.start && t < self.end
    }
}

/// Calendar helpers: local day boundaries in a time zone (DST-aware).
#[derive(Debug, Clone)]
pub struct Cal {
    pub tz: TimeZone,
}

impl Cal {
    pub fn new(tz: TimeZone) -> Self {
        Self { tz }
    }
    pub fn system() -> Self {
        Self { tz: TimeZone::system() }
    }
    pub fn utc() -> Self {
        Self { tz: TimeZone::UTC }
    }
    pub fn day_of(&self, t: Timestamp) -> Date {
        t.to_zoned(self.tz.clone()).date()
    }
    /// Midnight at the start of `d` (the first valid instant if midnight is skipped by DST).
    pub fn start_of(&self, d: Date) -> Timestamp {
        self.tz
            .to_ambiguous_zoned(d.to_datetime(jiff::civil::Time::midnight()))
            .compatible()
            .map(|z| z.timestamp())
            .unwrap_or_else(|_| Timestamp::UNIX_EPOCH)
    }
    pub fn day_interval(&self, d: Date) -> Interval {
        Interval::new(self.start_of(d), self.start_of(add_days(d, 1)))
    }
    pub fn dates_interval(&self, first: Date, end_exclusive: Date) -> Interval {
        Interval::new(self.start_of(first), self.start_of(end_exclusive))
    }
    pub fn hour_of(&self, t: Timestamp) -> usize {
        t.to_zoned(self.tz.clone()).hour() as usize
    }
    /// Monday = 0 … Sunday = 6.
    pub fn weekday_row(&self, t: Timestamp) -> usize {
        t.to_zoned(self.tz.clone()).weekday().to_monday_zero_offset() as usize
    }
}

/// `d` plus `n` days, or `d` unchanged if that leaves the supported calendar.
pub fn add_days(d: Date, n: i64) -> Date {
    Span::new().try_days(n).ok().and_then(|s| d.checked_add(s).ok()).unwrap_or(d)
}

/// `d` plus `n` months (clamped to the month's last day), or `d` unchanged if that leaves the supported calendar.
pub fn add_months(d: Date, n: i64) -> Date {
    Span::new().try_months(n).ok().and_then(|s| d.checked_add(s).ok()).unwrap_or(d)
}

/// Whole days from `a` to `b`.
pub fn days_between(a: Date, b: Date) -> i64 {
    (b - a).get_days() as i64
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelStat {
    /// Normalized model id.
    pub id: String,
    pub name: String,
    pub cost: f64,
    pub messages: u64,
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub fast_cost: f64,
    pub estimated: bool,
    pub share: f64,
}

impl ModelStat {
    pub fn tokens(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
    pub fn cost_per_message(&self) -> f64 {
        if self.messages > 0 { self.cost / self.messages as f64 } else { 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectStat {
    pub path: String,
    pub name: String,
    pub cost: f64,
    pub messages: u64,
    pub sessions: u64,
    pub last_active: Timestamp,
    pub share: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStat {
    pub id: String,
    pub title: String,
    pub project: String,
    pub start: Timestamp,
    pub end: Timestamp,
    pub cost: f64,
    pub messages: u64,
    pub models: Vec<String>,
    pub entrypoint: String,
}

impl SessionStat {
    pub const UNTITLED: &'static str = "Untitled session";

    pub fn duration_secs(&self) -> f64 {
        self.end.duration_since(self.start).as_secs_f64()
    }

    /// `i-want-you-to-cached-sloth-of-46b41c9d-fd9b` → `I want you to cached sloth`
    pub fn prettify(slug: &str) -> String {
        let mut words: Vec<&str> = slug.split('-').filter(|w| !w.is_empty()).collect();
        let is_hexish = |w: &str| {
            w.len() >= 4 && w.chars().all(|c| c.is_ascii_hexdigit()) && w.chars().any(|c| c.is_ascii_digit())
        };
        if let Some(i) = words.iter().position(|w| is_hexish(w)) {
            words.truncate(i);
        }
        if let Some(last) = words.last() {
            if ["of", "to", "for", "the", "a", "and"].contains(last) {
                words.pop();
            }
        }
        if words.is_empty() {
            return Self::UNTITLED.into();
        }
        let s = words.join(" ");
        let mut chars = s.chars();
        match chars.next() {
            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            None => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct NamedValue {
    pub name: String,
    pub value: f64,
    pub tokens: u64,
}

pub const TOKEN_CLASSES: [&str; 4] = ["Input", "Output", "Cache write", "Cache read"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct HourCost {
    pub hour: u32,
    pub model: String,
    pub cost: f64,
}

/// In-memory index over priced records, sorted by timestamp.
#[derive(Debug, Clone)]
pub struct UsageStore {
    pub records: Vec<UsageRecord>,
    pub cal: Cal,
    pub price_book: PriceBook,
    pub log_series: DailySeries,
}

impl UsageStore {
    pub fn new(mut records: Vec<UsageRecord>, price_book: PriceBook, cal: Cal) -> Self {
        price_book.price_all(&mut records);
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
        let mut series = DailySeries::new();
        for r in &records {
            let d = series.entry(cal.day_of(r.timestamp)).or_default();
            d.cost += r.cost;
            *d.by_model.entry(PriceBook::normalize(&r.model)).or_insert(0.0) += r.cost;
            d.messages += 1;
            d.tokens += r.total_tokens();
        }
        Self { records, cal, price_book, log_series: series }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new(), PriceBook::bundled().clone(), Cal::system())
    }

    pub fn first_date(&self) -> Option<Timestamp> {
        self.records.first().map(|r| r.timestamp)
    }

    pub fn last_date(&self) -> Option<Timestamp> {
        self.records.last().map(|r| r.timestamp)
    }

    fn lower_bound(&self, t: Timestamp) -> usize {
        self.records.partition_point(|r| r.timestamp < t)
    }

    pub fn records_in(&self, i: Interval) -> &[UsageRecord] {
        let a = self.lower_bound(i.start);
        let b = self.lower_bound(i.end).max(a);
        &self.records[a..b]
    }

    pub fn cost_in(&self, i: Interval) -> f64 {
        self.records_in(i).iter().map(|r| r.cost).sum()
    }

    /// Cost and counts for records at or after `after` on the local day `day`.
    pub fn day_cost_after(&self, day: Date, after: Timestamp) -> DayCost {
        let end = self.cal.start_of(add_days(day, 1));
        let mut d = DayCost::default();
        for r in self.records_in(Interval::new(after.max(self.cal.start_of(day)), end)) {
            d.cost += r.cost;
            *d.by_model.entry(PriceBook::normalize(&r.model)).or_insert(0.0) += r.cost;
            d.messages += 1;
            d.tokens += r.total_tokens();
        }
        d
    }

    pub fn model_stats(&self, i: Interval) -> Vec<ModelStat> {
        let mut map: HashMap<String, ModelStat> = HashMap::new();
        let mut total = 0.0;
        for r in self.records_in(i) {
            let key = PriceBook::normalize(&r.model);
            let s = map.entry(key.clone()).or_insert_with(|| ModelStat {
                id: key,
                name: self.price_book.display_name(&r.model),
                cost: 0.0,
                messages: 0,
                input: 0,
                output: 0,
                cache_write: 0,
                cache_read: 0,
                fast_cost: 0.0,
                estimated: false,
                share: 0.0,
            });
            s.cost += r.cost;
            s.messages += 1;
            s.input += r.input;
            s.output += r.output;
            s.cache_write += r.cache_write();
            s.cache_read += r.cache_read;
            if r.fast {
                s.fast_cost += r.cost;
            }
            s.estimated |= r.estimated;
            total += r.cost;
        }
        let mut out: Vec<ModelStat> = map
            .into_values()
            .map(|mut s| {
                s.share = if total > 0.0 { s.cost / total } else { 0.0 };
                s
            })
            .collect();
        out.sort_by(|a, b| b.cost.total_cmp(&a.cost).then_with(|| a.id.cmp(&b.id)));
        out
    }

    pub fn project_stats(&self, i: Interval) -> Vec<ProjectStat> {
        struct Acc<'a> {
            cost: f64,
            messages: u64,
            sessions: HashSet<&'a str>,
            last: Timestamp,
        }
        let mut map: HashMap<&str, Acc> = HashMap::new();
        let mut total = 0.0;
        for r in self.records_in(i) {
            let a = map.entry(r.project_path()).or_insert_with(|| Acc {
                cost: 0.0,
                messages: 0,
                sessions: HashSet::new(),
                last: Timestamp::MIN,
            });
            a.cost += r.cost;
            a.messages += 1;
            a.sessions.insert(&r.session_id);
            a.last = a.last.max(r.timestamp);
            total += r.cost;
        }
        let mut out: Vec<ProjectStat> = map
            .into_iter()
            .map(|(path, a)| ProjectStat {
                path: path.to_string(),
                name: project_name(path),
                cost: a.cost,
                messages: a.messages,
                sessions: a.sessions.len() as u64,
                last_active: a.last,
                share: if total > 0.0 { a.cost / total } else { 0.0 },
            })
            .collect();
        out.sort_by(|a, b| b.cost.total_cmp(&a.cost).then_with(|| a.path.cmp(&b.path)));
        out
    }

    pub fn sessions(&self, i: Interval, limit: usize) -> Vec<SessionStat> {
        let mut map: HashMap<&str, SessionStat> = HashMap::new();
        for r in self.records_in(i) {
            let key: &str = if r.session_id.is_empty() { &r.id } else { &r.session_id };
            let name = self.price_book.display_name(&r.model);
            match map.get_mut(key) {
                Some(s) => {
                    s.cost += r.cost;
                    s.messages += 1;
                    s.start = s.start.min(r.timestamp);
                    s.end = s.end.max(r.timestamp);
                    if !s.models.contains(&name) {
                        s.models.push(name);
                    }
                    if s.title == SessionStat::UNTITLED {
                        if let Some(slug) = &r.slug {
                            s.title = SessionStat::prettify(slug);
                        }
                    }
                }
                None => {
                    map.insert(
                        key,
                        SessionStat {
                            id: key.to_string(),
                            title: r.slug.as_deref().map(SessionStat::prettify).unwrap_or_else(|| SessionStat::UNTITLED.into()),
                            project: r.project_name(),
                            start: r.timestamp,
                            end: r.timestamp,
                            cost: r.cost,
                            messages: 1,
                            models: vec![name],
                            entrypoint: entrypoint_display_name(&r.entrypoint),
                        },
                    );
                }
            }
        }
        let mut out: Vec<SessionStat> = map.into_values().collect();
        out.sort_by(|a, b| b.cost.total_cmp(&a.cost).then_with(|| a.id.cmp(&b.id)));
        out.truncate(limit);
        out
    }

    pub fn entrypoint_stats(&self, i: Interval) -> Vec<NamedValue> {
        let mut map: HashMap<String, f64> = HashMap::new();
        for r in self.records_in(i) {
            *map.entry(entrypoint_display_name(&r.entrypoint)).or_insert(0.0) += r.cost;
        }
        let mut out: Vec<NamedValue> = map.into_iter().map(|(name, value)| NamedValue { name, value, tokens: 0 }).collect();
        out.sort_by(|a, b| b.value.total_cmp(&a.value).then_with(|| a.name.cmp(&b.name)));
        out
    }

    /// (main agent cost, subagent cost)
    pub fn agent_split(&self, i: Interval) -> (f64, f64) {
        let (mut main, mut sub) = (0.0, 0.0);
        for r in self.records_in(i) {
            if r.is_subagent { sub += r.cost } else { main += r.cost }
        }
        (main, sub)
    }

    pub fn token_classes(&self, i: Interval) -> Vec<NamedValue> {
        let mut tok = [0u64; 4];
        let mut usd = [0f64; 4];
        for r in self.records_in(i) {
            let (rates, _) = self.price_book.rates(r);
            let m = if r.fast { self.price_book.table.fast_multiplier } else { 1.0 } / 1_000_000.0;
            tok[0] += r.input;
            usd[0] += r.input as f64 * rates.input * m;
            tok[1] += r.output;
            usd[1] += r.output as f64 * rates.output * m;
            tok[2] += r.cache_write();
            usd[2] += (r.cache_write_5m as f64 * rates.write5m + r.cache_write_1h as f64 * rates.write1h) * m;
            tok[3] += r.cache_read;
            usd[3] += r.cache_read as f64 * rates.read * m;
        }
        TOKEN_CLASSES
            .iter()
            .enumerate()
            .map(|(i, name)| NamedValue { name: (*name).into(), value: usd[i], tokens: tok[i] })
            .collect()
    }

    pub fn cache_savings(&self, i: Interval) -> f64 {
        self.records_in(i).iter().map(|r| self.price_book.cache_savings(r)).sum()
    }

    /// 7 × 24 grid of cost, rows Monday…Sunday, columns hour 0…23.
    pub fn weekday_hour_heatmap(&self, i: Interval) -> Vec<Vec<f64>> {
        let mut grid = vec![vec![0.0; 24]; 7];
        for r in self.records_in(i) {
            grid[self.cal.weekday_row(r.timestamp)][self.cal.hour_of(r.timestamp)] += r.cost;
        }
        grid
    }

    /// Cost per hour of one local day, split by model display name.
    pub fn hourly(&self, day: Date) -> Vec<HourCost> {
        let mut map: HashMap<(u32, String), f64> = HashMap::new();
        for r in self.records_in(self.cal.day_interval(day)) {
            let h = self.cal.hour_of(r.timestamp) as u32;
            *map.entry((h, self.price_book.display_name(&r.model))).or_insert(0.0) += r.cost;
        }
        let mut out: Vec<HourCost> = map.into_iter().map(|((hour, model), cost)| HourCost { hour, model, cost }).collect();
        out.sort_by(|a, b| a.hour.cmp(&b.hour).then_with(|| a.model.cmp(&b.model)));
        out
    }

    /// Busiest hour of day across an interval.
    pub fn busiest_hour(&self, i: Interval) -> Option<usize> {
        let mut hours = [0.0f64; 24];
        for r in self.records_in(i) {
            hours[self.cal.hour_of(r.timestamp)] += r.cost;
        }
        let m = hours.iter().copied().fold(0.0, f64::max);
        if m <= 0.0 {
            return None;
        }
        hours.iter().position(|&h| h == m)
    }
}
