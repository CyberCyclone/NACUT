//! Combines local-log estimates with billed API data. Port of `SourceMerger.swift`.

use jiff::civil::Date;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::analytics::ApiCostData;
use crate::pricing::PriceBook;
use crate::store::{add_days, Cal, CostSource, DailySeries, DayCost};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum DataSourceMode {
    #[default]
    Logs,
    Api,
    Hybrid,
}

impl DataSourceMode {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Logs => "Local logs",
            Self::Api => "Anthropic API only",
            Self::Hybrid => "Hybrid (API + live logs)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Calibration {
    /// Billed ÷ list-price estimate over recent complete days. 1 when unknown.
    pub ratio: f64,
    pub sample_days: u32,
    pub api_total: f64,
    pub log_total: f64,
}

impl Calibration {
    pub const IDENTITY: Calibration = Calibration { ratio: 1.0, sample_days: 0, api_total: 0.0, log_total: 0.0 };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationDay {
    pub date: Date,
    pub api: f64,
    pub logs: f64,
}

pub struct SourceMerger {
    pub cal: Cal,
}

impl SourceMerger {
    pub fn new(cal: Cal) -> Self {
        Self { cal }
    }

    /// API rows folded into local days.
    pub fn api_series(&self, data: &ApiCostData) -> DailySeries {
        let mut s = DailySeries::new();
        for row in &data.rows {
            let d = s.entry(self.cal.day_of(row.start)).or_insert_with(|| DayCost { source: CostSource::Api, ..Default::default() });
            d.cost += row.amount;
            if let Some(m) = &row.model {
                *d.by_model.entry(PriceBook::normalize(m)).or_insert(0.0) += row.amount;
            }
        }
        s
    }

    /// Local days entirely covered by API data (before the refresh point): `[first, end_exclusive)`.
    fn complete_days(&self, data: &ApiCostData) -> Option<(Date, Date)> {
        let refreshed = data.refreshed_at?;
        let first = self.cal.day_of(data.coverage_start);
        let first_full = if self.cal.start_of(first) < data.coverage_start { add_days(first, 1) } else { first };
        let end = self.cal.day_of(refreshed.min(data.coverage_end));
        (first_full < end).then_some((first_full, end))
    }

    pub fn calibration(&self, logs: &DailySeries, api: &ApiCostData, window: u32) -> Calibration {
        let Some((first, end_ex)) = self.complete_days(api) else { return Calibration::IDENTITY };
        let api_days = self.api_series(api);
        let (mut api_sum, mut log_sum, mut n) = (0.0, 0.0, 0);
        let mut d = add_days(end_ex, -1);
        while d >= first && n < window {
            let a = api_days.get(&d).map(|x| x.cost).unwrap_or(0.0);
            let l = logs.get(&d).map(|x| x.cost).unwrap_or(0.0);
            if a > 0.0 && l > 0.0 {
                api_sum += a;
                log_sum += l;
                n += 1;
            }
            d = add_days(d, -1);
        }
        if n < 3 || log_sum <= 0.0 {
            return Calibration { ratio: 1.0, sample_days: n, api_total: api_sum, log_total: log_sum };
        }
        Calibration { ratio: (api_sum / log_sum).clamp(0.2, 5.0), sample_days: n, api_total: api_sum, log_total: log_sum }
    }

    /// The series the UI shows. `log_cost_after(day, t)` is the local-log cost on `day` at or after `t`
    /// (to fill the partial refresh day).
    pub fn merge(
        &self,
        mode: DataSourceMode,
        logs: &DailySeries,
        api: Option<&ApiCostData>,
        log_cost_after: impl Fn(Date, Timestamp) -> DayCost,
    ) -> (DailySeries, Calibration) {
        let Some(api) = api.filter(|_| mode != DataSourceMode::Logs) else { return (logs.clone(), Calibration::IDENTITY) };
        let api_days = self.api_series(api);
        let cal = self.calibration(logs, api, 14);
        let coverage_first = self.cal.day_of(api.coverage_start);

        if mode == DataSourceMode::Api {
            let mut s: DailySeries = logs.range(..coverage_first).map(|(k, v)| (*k, v.clone())).collect();
            for (k, v) in api_days {
                s.insert(k, v);
            }
            return (s, cal);
        }

        // Hybrid
        let Some(refreshed) = api.refreshed_at else { return (logs.clone(), cal) };
        let mut s = logs.clone();
        let refresh_day = self.cal.day_of(refreshed);
        for (day, log_day) in logs.range(add_days(refresh_day, 1)..) {
            s.insert(*day, scaled(log_day.clone(), cal.ratio));
        }
        // Complete API days override logs.
        let mut d = coverage_first;
        while d < refresh_day {
            s.insert(d, api_days.get(&d).cloned().unwrap_or(DayCost { source: CostSource::Api, ..Default::default() }));
            d = add_days(d, 1);
        }
        // Refresh day: billed part from the API plus the live estimate after the refresh point.
        let mut partial = api_days.get(&refresh_day).cloned().unwrap_or(DayCost { source: CostSource::Api, ..Default::default() });
        let rest = scaled(log_cost_after(refresh_day, refreshed), cal.ratio);
        partial.cost += rest.cost;
        partial.messages += rest.messages;
        partial.tokens += rest.tokens;
        for (m, v) in &rest.by_model {
            *partial.by_model.entry(m.clone()).or_insert(0.0) += v;
        }
        partial.source = if rest.cost > 0.0 { CostSource::Estimated } else { CostSource::Api };
        s.insert(refresh_day, partial);
        (s, cal)
    }

    pub fn reconciliation(&self, logs: &DailySeries, api: &ApiCostData, days: usize) -> Vec<ReconciliationDay> {
        let Some((first, end_ex)) = self.complete_days(api) else { return Vec::new() };
        let api_days = self.api_series(api);
        let mut out = Vec::new();
        let mut d = add_days(end_ex, -1);
        while d >= first && out.len() < days {
            out.push(ReconciliationDay {
                date: d,
                api: api_days.get(&d).map(|x| x.cost).unwrap_or(0.0),
                logs: logs.get(&d).map(|x| x.cost).unwrap_or(0.0),
            });
            d = add_days(d, -1);
        }
        out.reverse();
        out
    }
}

fn scaled(mut day: DayCost, r: f64) -> DayCost {
    day.cost *= r;
    for v in day.by_model.values_mut() {
        *v *= r;
    }
    day.source = if r == 1.0 { CostSource::Logs } else { CostSource::Estimated };
    day
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn data(coverage_start: &str, coverage_end: &str, refreshed: Option<&str>) -> ApiCostData {
        ApiCostData {
            rows: Vec::new(),
            refreshed_at: refreshed.map(|r| r.parse().unwrap()),
            coverage_start: coverage_start.parse().unwrap(),
            coverage_end: coverage_end.parse().unwrap(),
            fetched_at: coverage_end.parse().unwrap(),
        }
    }

    #[test]
    fn complete_days_run_from_the_first_full_day_to_the_refresh_day() {
        let m = SourceMerger::new(Cal::utc());
        let full = data("2026-11-10T00:00:00Z", "2026-11-20T00:00:00Z", Some("2026-11-18T12:00:00Z"));
        assert_eq!(m.complete_days(&full), Some((date(2026, 11, 10), date(2026, 11, 18))));
        let mid_day = data("2026-11-10T00:00:01Z", "2026-11-20T00:00:00Z", Some("2026-11-18T12:00:00Z"));
        assert_eq!(m.complete_days(&mid_day), Some((date(2026, 11, 11), date(2026, 11, 18))));
    }

    #[test]
    fn complete_days_stop_at_the_coverage_end_when_it_comes_first() {
        let m = SourceMerger::new(Cal::utc());
        let d = data("2026-11-10T00:00:00Z", "2026-11-15T06:00:00Z", Some("2026-11-18T12:00:00Z"));
        assert_eq!(m.complete_days(&d), Some((date(2026, 11, 10), date(2026, 11, 15))));
    }

    #[test]
    fn no_complete_days_without_a_refresh_or_a_whole_day() {
        let m = SourceMerger::new(Cal::utc());
        assert_eq!(m.complete_days(&data("2026-11-10T00:00:00Z", "2026-11-20T00:00:00Z", None)), None);
        assert_eq!(m.complete_days(&data("2026-11-10T06:00:00Z", "2026-11-20T00:00:00Z", Some("2026-11-11T12:00:00Z"))), None);
    }
}
