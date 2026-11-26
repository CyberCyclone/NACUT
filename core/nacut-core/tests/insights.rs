//! Totals, streaks and cycle comparisons (`insights.rs`).

mod common;

use jiff::civil::{date, Date};
use jiff::Timestamp;
use nacut_core::insights::{days_ending_at, series_cost, Insights};
use nacut_core::store::{Cal, CostSource, DailySeries};
use nacut_core::BillingCycle;

use common::{series, ts};

fn now() -> Timestamp {
    ts("2026-11-11T12:00:00Z")
}

fn insights_at(s: &DailySeries, now: Timestamp) -> Insights {
    let cal = Cal::utc();
    let cycle = BillingCycle::containing(now, 1, &cal);
    Insights::new(s, &cycle, now, &cal)
}

fn insights(pairs: &[(Date, f64)]) -> Insights {
    insights_at(&series(pairs), now())
}

fn nov(d: i8) -> Date {
    date(2026, 11, d)
}

// MARK: Streak

#[test]
fn streak_counts_back_from_yesterday_when_today_is_idle() {
    assert_eq!(insights(&[(nov(8), 1.0), (nov(9), 1.0), (nov(10), 1.0)]).active_streak, 3);
}

#[test]
fn streak_stops_at_a_gap() {
    assert_eq!(insights(&[(nov(8), 1.0), (nov(10), 1.0), (nov(11), 1.0)]).active_streak, 2);
}

#[test]
fn a_zero_cost_day_breaks_the_streak() {
    assert_eq!(insights(&[(nov(9), 5.0), (nov(10), 0.0), (nov(11), 5.0)]).active_streak, 1);
}

#[test]
fn no_streak_without_spend_today_or_yesterday() {
    assert_eq!(insights(&[(nov(8), 1.0), (nov(9), 1.0)]).active_streak, 0);
    assert_eq!(insights(&[]).active_streak, 0);
}

#[test]
fn streak_crosses_month_boundaries() {
    let s = series(&[(date(2026, 10, 30), 1.0), (date(2026, 10, 31), 1.0), (nov(1), 1.0)]);
    assert_eq!(insights_at(&s, ts("2026-11-01T08:00:00Z")).active_streak, 3);
}

// MARK: Peak day

#[test]
fn no_peak_day_without_spend() {
    assert_eq!(insights(&[]).peak_day, None);
    assert_eq!(insights(&[(nov(3), 0.0), (nov(4), 0.0)]).peak_day, None);
}

#[test]
fn peak_day_is_the_most_expensive_day() {
    let p = insights(&[(nov(3), 4.0), (nov(4), 9.5), (nov(5), 2.0)]).peak_day.unwrap();
    assert_eq!((p.date, p.cost), (nov(4), 9.5));
}

#[test]
fn tied_peak_days_resolve_to_the_latest_date() {
    let p = insights(&[(nov(3), 10.0), (nov(7), 10.0), (nov(5), 1.0)]).peak_day.unwrap();
    assert_eq!(p.date, nov(7));
}

#[test]
fn peak_day_may_be_outside_the_cycle() {
    let p = insights(&[(date(2026, 9, 1), 50.0), (nov(4), 9.5)]).peak_day.unwrap();
    assert_eq!(p.date, date(2026, 9, 1));
}

// MARK: Windows

#[test]
fn trailing_windows_include_today_and_stop_before_their_length() {
    let i = insights(&[
        (nov(11), 1.0),
        (nov(5), 2.0),
        (nov(4), 4.0),
        (date(2026, 10, 13), 8.0),
        (date(2026, 10, 12), 16.0),
        (nov(12), 32.0),
    ]);
    assert_eq!(i.last7_days, 3.0);
    assert_eq!(i.last30_days, 15.0);
    assert_eq!(i.all_time, 63.0);
}

#[test]
fn active_days_count_only_spending_days_in_this_cycle() {
    let i = insights(&[(date(2026, 10, 31), 5.0), (nov(1), 1.0), (nov(2), 0.0), (nov(5), 3.0), (date(2026, 12, 1), 2.0)]);
    assert_eq!(i.active_days_this_cycle, 2);
}

// MARK: Previous cycle

#[test]
fn no_comparison_without_spend_in_the_previous_cycle() {
    let i = insights(&[(nov(5), 10.0), (date(2026, 10, 20), 0.0)]);
    assert_eq!(i.vs_previous_cycle, None);
    assert_eq!(i.previous_cycle_total, 0.0);
}

#[test]
fn previous_cycle_is_compared_up_to_the_same_point() {
    // 10.5 days into November; Oct 11 counts (it started before Oct 11 12:00), Oct 12 does not.
    let i = insights(&[(date(2026, 10, 11), 10.0), (date(2026, 10, 12), 100.0), (nov(5), 15.0)]);
    assert_eq!(i.previous_cycle_total, 110.0);
    assert_eq!(i.vs_previous_cycle, Some(0.5));
}

#[test]
fn this_cycle_to_date_ignores_future_days() {
    let i = insights(&[(date(2026, 10, 2), 10.0), (nov(2), 10.0), (nov(20), 100.0)]);
    assert_eq!(i.vs_previous_cycle, Some(0.0));
}

#[test]
fn a_longer_cycle_does_not_count_its_own_days_as_the_previous_cycle() {
    // March is longer than February: 30.5 days in reaches past Feb's end into March itself.
    let s = series(&[(date(2026, 2, 10), 10.0), (date(2026, 3, 2), 5.0)]);
    let i = insights_at(&s, ts("2026-03-31T12:00:00Z"));
    assert_eq!(i.previous_cycle_total, 10.0);
    assert_eq!(i.vs_previous_cycle, Some(-0.5));
}

#[test]
fn a_shorter_cycle_compares_against_the_start_of_a_longer_one() {
    // 27.5 days into February reaches Jan 28 12:00.
    let s = series(&[(date(2026, 1, 28), 10.0), (date(2026, 1, 29), 100.0), (date(2026, 2, 20), 20.0)]);
    let i = insights_at(&s, ts("2026-02-28T12:00:00Z"));
    assert_eq!(i.previous_cycle_total, 110.0);
    assert_eq!(i.vs_previous_cycle, Some(1.0));
}

// MARK: Helpers

#[test]
fn days_ending_at_zero_days_is_empty() {
    assert!(days_ending_at(&series(&[(nov(5), 1.0)]), nov(5), 0).is_empty());
}

#[test]
fn days_ending_at_runs_oldest_first_and_fills_gaps() {
    let days = days_ending_at(&series(&[(nov(3), 2.0), (nov(5), 7.0)]), nov(5), 4);
    assert_eq!(days.iter().map(|(d, _)| *d).collect::<Vec<_>>(), [nov(2), nov(3), nov(4), nov(5)]);
    assert_eq!(days.iter().map(|(_, c)| c.cost).collect::<Vec<_>>(), [0.0, 2.0, 0.0, 7.0]);
    assert_eq!(days[2].1.source, CostSource::Logs);
    assert_eq!(days[2].1.messages, 0);
}

#[test]
fn series_cost_is_half_open() {
    let s = series(&[(nov(1), 1.0), (nov(2), 2.0), (nov(3), 4.0), (nov(4), 8.0)]);
    assert_eq!(series_cost(&s, nov(2), nov(4)), 6.0);
    assert_eq!(series_cost(&s, nov(2), nov(2)), 0.0);
    assert_eq!(series_cost(&s, nov(1), date(2026, 12, 1)), 15.0);
}
