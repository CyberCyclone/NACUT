//! Billing cycles, budget forecast and dollar formatting. Port of `Forecast.swift`.

use jiff::civil::Date;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::store::{add_days, add_months, days_between, Cal, DailySeries, Interval};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum BudgetStatus {
    NoBudget,
    UnderBudget,
    Tight,
    WillExceed,
    Exceeded,
}

/// A billing cycle of one month starting on `start_day` (1…28). `end` is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingCycle {
    pub start: Date,
    pub end: Date,
}

impl BillingCycle {
    pub fn containing(now: Timestamp, start_day: u8, cal: &Cal) -> Self {
        Self::containing_date(cal.day_of(now), start_day)
    }

    pub fn containing_date(today: Date, start_day: u8) -> Self {
        let day = start_day.clamp(1, 28) as i8;
        let mut start = Date::new(today.year(), today.month(), day).expect("day ≤ 28 is always valid");
        if today.day() < day {
            start = add_months(start, -1);
        }
        Self { start, end: add_months(start, 1) }
    }

    pub fn previous(&self) -> Self {
        Self { start: add_months(self.start, -1), end: self.start }
    }

    pub fn days(&self) -> Vec<Date> {
        let mut out = Vec::new();
        let mut d = self.start;
        while d < self.end {
            out.push(d);
            d = add_days(d, 1);
        }
        out
    }

    pub fn interval(&self, cal: &Cal) -> Interval {
        cal.dates_interval(self.start, self.end)
    }

    pub fn contains(&self, d: Date) -> bool {
        d >= self.start && d < self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum PointKind {
    Actual,
    Projected,
    Pace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastPoint {
    pub date: Date,
    pub cumulative: f64,
    pub kind: PointKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Forecast {
    pub cycle: BillingCycle,
    pub budget: f64,
    pub spent: f64,
    pub today: f64,
    pub elapsed_days: f64,
    pub remaining_days: f64,
    pub total_days: f64,
    pub daily_average: f64,
    pub trailing7_average: f64,
    pub projected_rate: f64,
    pub projected: f64,
    pub exhaustion_date: Option<Timestamp>,
    pub status: BudgetStatus,
    pub safe_daily_spend: Option<f64>,
    pub points: Vec<ForecastPoint>,
}

impl Forecast {
    pub fn new(series: &DailySeries, budget: f64, cycle_start_day: u8, now: Timestamp, cal: &Cal) -> Self {
        let cycle = BillingCycle::containing(now, cycle_start_day, cal);
        let today_date = cal.day_of(now);
        let today_start = cal.start_of(today_date);
        let days = cycle.days();
        let total_days = days.len() as f64;
        let cost = |d: &Date| series.get(d).map(|c| c.cost).unwrap_or(0.0);

        let mut cumulative = 0.0;
        let mut actual = Vec::new();
        for d in days.iter().filter(|d| **d <= today_date) {
            cumulative += cost(d);
            actual.push(ForecastPoint { date: *d, cumulative, kind: PointKind::Actual });
        }
        let spent = cumulative;
        let today = cost(&today_date);

        let fraction_of_today = now.duration_since(today_start).as_secs_f64() / 86_400.0;
        let full_days_before = days_between(cycle.start, today_date) as f64;
        // Count today by how much of it has passed, but never less than a quarter day,
        // so an early-morning session doesn't make the average explode.
        let elapsed_days = (full_days_before + fraction_of_today.max(0.25)).max(0.25);
        let remaining_days = (total_days - full_days_before - fraction_of_today).max(0.0);
        let daily_average = spent / elapsed_days;

        // Trailing 7 complete days plus the part of today that has passed.
        let last7: f64 = (1..=7).map(|i| cost(&add_days(today_date, -i))).sum();
        let trailing7_average = (last7 + today) / (7.0 + fraction_of_today.max(0.25));

        let has_history = full_days_before >= 3.0;
        let projected_rate = if has_history {
            0.6 * trailing7_average + 0.4 * daily_average
        } else {
            trailing7_average.max(daily_average)
        };
        let projected = spent + projected_rate * remaining_days;

        let (status, exhaustion_date, safe_daily_spend);
        if budget > 0.0 {
            safe_daily_spend = (remaining_days > 0.0).then(|| (budget - spent).max(0.0) / remaining_days);
            if spent >= budget {
                status = BudgetStatus::Exceeded;
                exhaustion_date = actual.iter().find(|p| p.cumulative >= budget).map(|p| cal.start_of(p.date));
            } else if projected > budget {
                status = BudgetStatus::WillExceed;
                let days_left = if projected_rate > 0.0 { (budget - spent) / projected_rate } else { f64::INFINITY };
                exhaustion_date = days_left
                    .is_finite()
                    .then(|| now.checked_add(jiff::SignedDuration::from_secs_f64(days_left * 86_400.0)).ok())
                    .flatten();
            } else {
                status = if projected >= 0.9 * budget { BudgetStatus::Tight } else { BudgetStatus::UnderBudget };
                exhaustion_date = None;
            }
        } else {
            status = BudgetStatus::NoBudget;
            safe_daily_spend = None;
            exhaustion_date = None;
        }

        // Projection from end of today to cycle end, one point per day.
        let mut points = actual.clone();
        let mut running = spent;
        if let Some(last) = actual.last() {
            points.push(ForecastPoint { date: last.date, cumulative: last.cumulative, kind: PointKind::Projected });
        }
        for d in days.iter().filter(|d| **d > today_date) {
            running += projected_rate;
            points.push(ForecastPoint { date: *d, cumulative: running, kind: PointKind::Projected });
        }
        if budget > 0.0 {
            for (i, d) in days.iter().enumerate() {
                points.push(ForecastPoint { date: *d, cumulative: budget * (i + 1) as f64 / total_days, kind: PointKind::Pace });
            }
        }

        Self {
            cycle,
            budget,
            spent,
            today,
            elapsed_days,
            remaining_days,
            total_days,
            daily_average,
            trailing7_average,
            projected_rate,
            projected,
            exhaustion_date,
            status,
            safe_daily_spend,
            points,
        }
    }

    pub fn budget_used_fraction(&self) -> f64 {
        if self.budget > 0.0 { self.spent / self.budget } else { 0.0 }
    }

    pub fn projected_fraction(&self) -> f64 {
        if self.budget > 0.0 { self.projected / self.budget } else { 0.0 }
    }

    pub fn headline(&self, cal: &Cal) -> String {
        let f = |v| CurrencyFormat::string(v, false);
        match self.status {
            BudgetStatus::NoBudget => format!("Projected {} this cycle", f(self.projected)),
            BudgetStatus::UnderBudget => format!("On track · projected {} of {}", f(self.projected), f(self.budget)),
            BudgetStatus::Tight => format!("Tight · projected {} of {}", f(self.projected), f(self.budget)),
            BudgetStatus::WillExceed => match self.exhaustion_date {
                Some(d) => format!("Budget runs out ~{}", month_day(cal.day_of(d))),
                None => "Projected to exceed budget".into(),
            },
            BudgetStatus::Exceeded => format!("Over budget by {}", f(self.spent - self.budget)),
        }
    }
}

/// `Nov 27`
pub fn month_day(d: Date) -> String {
    d.strftime("%b %-d").to_string()
}

/// Dollar formatting is always US style (`$1,234`) whatever the system region,
/// since every amount here is in US dollars.
pub struct CurrencyFormat;

impl CurrencyFormat {
    /// `$4.21` under $100, `$1,234` above; `compact` turns ≥ $10k into `$12.3k`.
    pub fn string(value: f64, compact: bool) -> String {
        let v = if value.is_finite() { value } else { 0.0 };
        if compact && v.abs() >= 10_000.0 {
            return format!("${:.1}k", v / 1000.0);
        }
        if v.abs() >= 100.0 {
            return usd(v, 0, 0);
        }
        usd(v, 2, 2)
    }

    /// 2 to 4 decimals, for tooltips.
    pub fn exact(value: f64) -> String {
        usd(value, 2, 4)
    }

    /// `957.2M`, `1.27B`, `24.1K`.
    pub fn tokens(n: u64) -> String {
        let d = n as f64;
        if d >= 1e9 {
            format!("{:.2}B", d / 1e9)
        } else if d >= 1e6 {
            format!("{:.1}M", d / 1e6)
        } else if d >= 1e3 {
            format!("{:.1}K", d / 1e3)
        } else {
            n.to_string()
        }
    }
}

/// `-$1,234.50`-style formatting with `min..=max` fraction digits (trailing zeros trimmed down to `min`).
fn usd(v: f64, min_frac: usize, max_frac: usize) -> String {
    let neg = v < 0.0;
    let s = format!("{:.*}", max_frac, v.abs());
    let (int_part, mut frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (s.clone(), String::new()),
    };
    while frac.len() > min_frac && frac.ends_with('0') {
        frac.pop();
    }
    let mut grouped = String::new();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let body = if frac.is_empty() { grouped } else { format!("{grouped}.{frac}") };
    // A value that rounds to zero shouldn't show a minus sign.
    let is_zero = body.chars().all(|c| c == '0' || c == ',' || c == '.');
    if neg && !is_zero { format!("-${body}") } else { format!("${body}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollars_are_us_style() {
        assert_eq!(CurrencyFormat::string(521.4, false), "$521");
        assert_eq!(CurrencyFormat::string(8.5, false), "$8.50");
        assert_eq!(CurrencyFormat::string(1234.0, false), "$1,234");
        assert_eq!(CurrencyFormat::string(-5.0, false), "-$5.00");
        assert_eq!(CurrencyFormat::string(12_345.0, true), "$12.3k");
        assert_eq!(CurrencyFormat::exact(524.1987), "$524.1987");
        assert_eq!(CurrencyFormat::exact(3.5), "$3.50");
        assert_eq!(CurrencyFormat::tokens(957_200_000), "957.2M");
        assert_eq!(CurrencyFormat::tokens(1_270_000_000), "1.27B");
        assert_eq!(CurrencyFormat::tokens(26), "26");
    }
}
