//! All-time totals, streaks and cycle comparisons. Port of `Insights.swift`
//! (with half-open cycle ranges: the Swift version counted the next cycle's first day).

use jiff::civil::Date;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::forecast::BillingCycle;
use crate::store::{add_days, Cal, DailySeries, DayCost};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeakDay {
    pub date: Date,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insights {
    pub all_time: f64,
    pub last7_days: f64,
    pub last30_days: f64,
    pub peak_day: Option<PeakDay>,
    pub active_streak: u32,
    pub active_days_this_cycle: u32,
    /// Cycle-to-date vs the same point in the previous cycle; `None` without history.
    pub vs_previous_cycle: Option<f64>,
    pub previous_cycle_total: f64,
}

impl Insights {
    pub fn new(series: &DailySeries, cycle: &BillingCycle, now: Timestamp, cal: &Cal) -> Self {
        let today = cal.day_of(now);
        let cost = |d: &Date| series.get(d).map(|c| c.cost).unwrap_or(0.0);
        let all_time = series.values().map(|d| d.cost).sum();
        let sum_back = |n: i64| (0..n).map(|i| cost(&add_days(today, -i))).sum::<f64>();

        let peak_day = series
            .iter()
            .max_by(|a, b| a.1.cost.total_cmp(&b.1.cost))
            .filter(|(_, d)| d.cost > 0.0)
            .map(|(date, d)| PeakDay { date: *date, cost: d.cost });

        // Consecutive active days ending today (or yesterday if today has no usage yet).
        let mut streak = 0;
        let mut d = if cost(&today) > 0.0 { today } else { add_days(today, -1) };
        while cost(&d) > 0.0 {
            streak += 1;
            d = add_days(d, -1);
        }

        let active_days_this_cycle = series.iter().filter(|(k, v)| cycle.contains(**k) && v.cost > 0.0).count() as u32;

        let prev = cycle.previous();
        let previous_cycle_total = series.iter().filter(|(k, _)| prev.contains(**k)).map(|(_, v)| v.cost).sum();
        let elapsed = now.duration_since(cal.start_of(cycle.start));
        let prev_cutoff = cal.start_of(prev.start).checked_add(elapsed).unwrap_or(now);
        let prev_to_date: f64 = series
            .iter()
            .filter(|(k, _)| **k >= prev.start && cal.start_of(**k) < prev_cutoff)
            .map(|(_, v)| v.cost)
            .sum();
        let this_to_date: f64 = series.iter().filter(|(k, _)| cycle.contains(**k) && **k <= today).map(|(_, v)| v.cost).sum();
        let vs_previous_cycle = (prev_to_date > 0.0).then(|| (this_to_date - prev_to_date) / prev_to_date);

        Self {
            all_time,
            last7_days: sum_back(7),
            last30_days: sum_back(30),
            peak_day,
            active_streak: streak,
            active_days_this_cycle,
            vs_previous_cycle,
            previous_cycle_total,
        }
    }
}

/// Consecutive days ending at `end`, filling gaps with zero.
pub fn days_ending_at(series: &DailySeries, end: Date, count: usize) -> Vec<(Date, DayCost)> {
    (0..count as i64)
        .rev()
        .map(|i| {
            let d = add_days(end, -i);
            (d, series.get(&d).cloned().unwrap_or_default())
        })
        .collect()
}

/// Total cost of the days in `[first, end_exclusive)`.
pub fn series_cost(series: &DailySeries, first: Date, end_exclusive: Date) -> f64 {
    series.range(first..end_exclusive).map(|(_, d)| d.cost).sum()
}
