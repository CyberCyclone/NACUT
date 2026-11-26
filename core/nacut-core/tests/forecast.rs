//! Billing cycles, the budget forecast and dollar formatting (`forecast.rs`).

mod common;

use jiff::civil::date;
use jiff::tz::TimeZone;
use nacut_core::forecast::{month_day, BillingCycle, BudgetStatus, CurrencyFormat, Forecast, PointKind};
use nacut_core::store::{Cal, DailySeries};

use common::{series, ts};

/// US Eastern as a POSIX rule: DST from the second Sunday of March to the first Sunday of November.
fn eastern() -> Cal {
    Cal::new(TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap())
}

/// $50 a day for Nov 1–10 and $25 so far on Nov 11 at noon: $525 spent at $50/day, $1,500 projected.
fn fifty_a_day() -> DailySeries {
    let mut pairs: Vec<_> = (1..=10).map(|d| (date(2026, 11, d), 50.0)).collect();
    pairs.push((date(2026, 11, 11), 25.0));
    series(&pairs)
}

fn fifty_a_day_forecast(budget: f64) -> Forecast {
    Forecast::new(&fifty_a_day(), budget, 1, ts("2026-11-11T12:00:00Z"), &Cal::utc())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// MARK: BillingCycle

#[test]
fn start_day_zero_means_the_first() {
    let c = BillingCycle::containing_date(date(2026, 11, 10), 0);
    assert_eq!((c.start, c.end), (date(2026, 11, 1), date(2026, 12, 1)));
}

#[test]
fn a_january_date_before_the_start_day_belongs_to_decembers_cycle() {
    let c = BillingCycle::containing_date(date(2027, 1, 10), 15);
    assert_eq!((c.start, c.end), (date(2026, 12, 15), date(2027, 1, 15)));
}

#[test]
fn previous_cycle_ends_where_this_one_starts() {
    let c = BillingCycle::containing_date(date(2026, 11, 10), 1);
    let p = c.previous();
    assert_eq!((p.start, p.end), (date(2026, 10, 1), date(2026, 11, 1)));
    let late = BillingCycle::containing_date(date(2026, 3, 30), 31);
    assert_eq!((late.start, late.end), (date(2026, 3, 28), date(2026, 4, 28)));
    assert_eq!((late.previous().start, late.previous().end), (date(2026, 2, 28), date(2026, 3, 28)));
}

#[test]
fn cycle_days_follow_the_month_length() {
    let len = |d| BillingCycle::containing_date(d, 1).days().len();
    assert_eq!(len(date(2026, 2, 10)), 28);
    assert_eq!(len(date(2028, 2, 10)), 29);
    assert_eq!(len(date(2026, 11, 10)), 30);
    assert_eq!(len(date(2026, 12, 10)), 31);
    let days = BillingCycle::containing_date(date(2026, 2, 3), 28).days();
    assert_eq!((days.len(), days[0], days[30]), (31, date(2026, 1, 28), date(2026, 2, 27)));
}

#[test]
fn cycle_contains_its_start_but_not_its_end() {
    let c = BillingCycle::containing_date(date(2026, 11, 10), 1);
    assert!(c.contains(date(2026, 11, 1)));
    assert!(c.contains(date(2026, 11, 30)));
    assert!(!c.contains(date(2026, 12, 1)));
    assert!(!c.contains(date(2026, 10, 31)));
}

#[test]
fn cycle_interval_spans_local_midnights_across_a_dst_change() {
    let cal = eastern();
    let nov = BillingCycle::containing_date(date(2026, 11, 10), 1).interval(&cal);
    assert_eq!((nov.start, nov.end), (ts("2026-11-01T04:00:00Z"), ts("2026-12-01T05:00:00Z")));
    let mar = BillingCycle::containing_date(date(2026, 3, 10), 1).interval(&cal);
    assert_eq!((mar.start, mar.end), (ts("2026-03-01T05:00:00Z"), ts("2026-04-01T04:00:00Z")));
}

#[test]
fn cycle_containing_a_moment_uses_the_local_date() {
    // 02:00 UTC on Nov 1 is still Oct 31 in New York.
    let c = BillingCycle::containing(ts("2026-11-01T02:00:00Z"), 1, &eastern());
    assert_eq!(c.start, date(2026, 10, 1));
}

// MARK: Forecast status and headline

#[test]
fn projection_within_ten_percent_of_the_budget_is_tight() {
    let f = fifty_a_day_forecast(1600.0);
    assert_eq!(f.status, BudgetStatus::Tight);
    assert_eq!(f.exhaustion_date, None);
    assert_eq!(f.headline(&Cal::utc()), "Tight · projected $1,500 of $1,600");
}

#[test]
fn projection_exactly_at_the_budget_is_tight_not_over() {
    assert_eq!(fifty_a_day_forecast(1500.0).status, BudgetStatus::Tight);
}

#[test]
fn projection_well_under_the_budget_is_on_track() {
    let f = fifty_a_day_forecast(1700.0);
    assert_eq!(f.status, BudgetStatus::UnderBudget);
    assert_eq!(f.headline(&Cal::utc()), "On track · projected $1,500 of $1,700");
}

#[test]
fn no_budget_headline_states_the_projection() {
    let f = fifty_a_day_forecast(0.0);
    assert_eq!(f.status, BudgetStatus::NoBudget);
    assert_eq!(f.safe_daily_spend, None);
    assert_eq!(f.exhaustion_date, None);
    assert_eq!(f.headline(&Cal::utc()), "Projected $1,500 this cycle");
}

#[test]
fn exceeded_headline_states_the_overrun() {
    let f = Forecast::new(&series(&[(date(2026, 11, 1), 600.0), (date(2026, 11, 2), 600.0)]), 1000.0, 1, ts("2026-11-03T12:00:00Z"), &Cal::utc());
    assert_eq!(f.status, BudgetStatus::Exceeded);
    assert_eq!(f.headline(&Cal::utc()), "Over budget by $200");
    assert_eq!(f.safe_daily_spend, Some(0.0));
    assert!(close(f.budget_used_fraction(), 1.2));
}

#[test]
fn will_exceed_headline_without_a_date_is_generic() {
    let mut f = fifty_a_day_forecast(1000.0);
    assert_eq!(f.status, BudgetStatus::WillExceed);
    f.exhaustion_date = None;
    assert_eq!(f.headline(&Cal::utc()), "Projected to exceed budget");
}

#[test]
fn exhaustion_headline_uses_the_local_date() {
    // Nov 21 00:00 UTC is still Nov 20 in New York.
    assert_eq!(fifty_a_day_forecast(1000.0).headline(&eastern()), "Budget runs out ~Nov 20");
}

#[test]
fn fractions_are_zero_without_a_budget() {
    let f = fifty_a_day_forecast(0.0);
    assert_eq!(f.budget_used_fraction(), 0.0);
    assert_eq!(f.projected_fraction(), 0.0);
    let g = fifty_a_day_forecast(2000.0);
    assert_eq!(g.budget_used_fraction(), 0.2625);
    assert!(close(g.projected_fraction(), 0.75));
}

// MARK: Forecast rates

#[test]
fn early_in_a_cycle_the_higher_of_the_two_averages_is_used() {
    let f = Forecast::new(&series(&[(date(2026, 11, 1), 10.0), (date(2026, 11, 2), 5.0)]), 0.0, 1, ts("2026-11-02T12:00:00Z"), &Cal::utc());
    assert!(close(f.elapsed_days, 1.5));
    assert!(close(f.daily_average, 10.0));
    assert!(close(f.trailing7_average, 2.0));
    assert!(close(f.projected_rate, 10.0));
    assert!(close(f.remaining_days, 28.5));
    assert!(close(f.projected, 300.0));
}

#[test]
fn early_in_a_cycle_last_weeks_pace_carries_over() {
    let f = Forecast::new(&series(&[(date(2026, 10, 31), 70.0)]), 0.0, 1, ts("2026-11-01T12:00:00Z"), &Cal::utc());
    assert_eq!(f.spent, 0.0);
    assert_eq!(f.daily_average, 0.0);
    assert!(close(f.projected_rate, 70.0 / 7.5));
    assert!(close(f.projected, 70.0 / 7.5 * 29.5));
}

#[test]
fn at_midnight_on_day_one_a_quarter_day_has_elapsed() {
    let f = Forecast::new(&series(&[(date(2026, 11, 1), 2.0)]), 0.0, 1, ts("2026-11-01T00:00:00Z"), &Cal::utc());
    assert!(close(f.elapsed_days, 0.25));
    assert!(close(f.daily_average, 8.0));
    assert!(close(f.trailing7_average, 2.0 / 7.25));
    assert!(close(f.remaining_days, 30.0));
    assert!(close(f.projected, 242.0));
}

#[test]
fn an_empty_series_projects_nothing() {
    let f = Forecast::new(&DailySeries::new(), 100.0, 1, ts("2026-11-11T12:00:00Z"), &Cal::utc());
    assert_eq!((f.spent, f.today, f.projected, f.projected_rate), (0.0, 0.0, 0.0, 0.0));
    assert_eq!(f.status, BudgetStatus::UnderBudget);
    assert!(close(f.safe_daily_spend.unwrap(), 100.0 / 19.5));
    assert_eq!(f.headline(&Cal::utc()), "On track · projected $0.00 of $100");
}

#[test]
fn spend_dated_after_today_is_not_counted() {
    let f = Forecast::new(&series(&[(date(2026, 11, 1), 10.0), (date(2026, 11, 20), 999.0)]), 0.0, 1, ts("2026-11-11T12:00:00Z"), &Cal::utc());
    assert_eq!(f.spent, 10.0);
    assert_eq!(f.today, 0.0);
    let last_actual = f.points.iter().rfind(|p| p.kind == PointKind::Actual).unwrap();
    assert_eq!((last_actual.date, last_actual.cumulative), (date(2026, 11, 11), 10.0));
}

#[test]
fn safe_daily_spend_spreads_the_rest_of_the_budget() {
    let f = fifty_a_day_forecast(1000.0);
    assert!(close(f.safe_daily_spend.unwrap(), 475.0 / 19.5));
}

// MARK: Forecast points

#[test]
fn points_are_actual_then_projected_then_pace() {
    let f = fifty_a_day_forecast(1000.0);
    let of = |k| f.points.iter().filter(|p| p.kind == k).collect::<Vec<_>>();
    let actual = of(PointKind::Actual);
    let projected = of(PointKind::Projected);
    let pace = of(PointKind::Pace);
    assert_eq!((actual.len(), projected.len(), pace.len()), (11, 20, 30));
    assert_eq!(f.points.len(), 61);

    assert_eq!((actual[0].date, actual[0].cumulative), (date(2026, 11, 1), 50.0));
    assert_eq!((actual[10].date, actual[10].cumulative), (date(2026, 11, 11), 525.0));
    // The projection starts from today's actual total, then adds the projected rate per remaining day.
    assert_eq!((projected[0].date, projected[0].cumulative), (date(2026, 11, 11), 525.0));
    assert_eq!(projected[1].date, date(2026, 11, 12));
    assert!(close(projected[1].cumulative, 575.0));
    assert_eq!(projected[19].date, date(2026, 11, 30));
    assert!(close(projected[19].cumulative, 1475.0));
    assert!(close(pace[0].cumulative, 1000.0 / 30.0));
    assert_eq!((pace[29].date, pace[29].cumulative), (date(2026, 11, 30), 1000.0));
}

#[test]
fn points_have_no_pace_line_without_a_budget() {
    let f = fifty_a_day_forecast(0.0);
    assert!(f.points.iter().all(|p| p.kind != PointKind::Pace));
    assert_eq!(f.points.len(), 31);
}

#[test]
fn on_the_last_day_only_the_bridge_point_is_projected() {
    let f = Forecast::new(&series(&[(date(2026, 11, 30), 5.0)]), 0.0, 1, ts("2026-11-30T12:00:00Z"), &Cal::utc());
    let projected: Vec<_> = f.points.iter().filter(|p| p.kind == PointKind::Projected).collect();
    assert_eq!(projected.len(), 1);
    assert_eq!((projected[0].date, projected[0].cumulative), (date(2026, 11, 30), 5.0));
}

// MARK: Formatting

#[test]
fn month_day_has_no_leading_zero() {
    assert_eq!(month_day(date(2026, 1, 5)), "Jan 5");
    assert_eq!(month_day(date(2026, 12, 31)), "Dec 31");
}

#[test]
fn dollars_that_round_to_zero_have_no_sign() {
    assert_eq!(CurrencyFormat::string(-0.004, false), "$0.00");
    assert_eq!(CurrencyFormat::string(-0.0, false), "$0.00");
    assert_eq!(CurrencyFormat::exact(-0.00001), "$0.00");
}

#[test]
fn non_finite_dollars_read_zero() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(CurrencyFormat::string(v, false), "$0.00");
        assert_eq!(CurrencyFormat::string(v, true), "$0.00");
    }
}

#[test]
fn cents_are_shown_below_a_hundred_dollars() {
    assert_eq!(CurrencyFormat::string(0.0, false), "$0.00");
    assert_eq!(CurrencyFormat::string(99.994, false), "$99.99");
    assert_eq!(CurrencyFormat::string(-42.5, false), "-$42.50");
}

#[test]
fn dollars_that_round_up_to_a_hundred_drop_the_cents() {
    assert_eq!(CurrencyFormat::string(99.995, false), "$100");
    assert_eq!(CurrencyFormat::string(99.999, false), "$100");
    assert_eq!(CurrencyFormat::string(-99.999, false), "-$100");
}

#[test]
fn large_dollars_are_grouped_by_thousands() {
    assert_eq!(CurrencyFormat::string(100.0, false), "$100");
    assert_eq!(CurrencyFormat::string(999.6, false), "$1,000");
    assert_eq!(CurrencyFormat::string(1_000_000.0, false), "$1,000,000");
    assert_eq!(CurrencyFormat::string(1_234_567.89, false), "$1,234,568");
    assert_eq!(CurrencyFormat::string(-1_234_567.0, false), "-$1,234,567");
}

#[test]
fn compact_dollars_switch_to_thousands_at_ten_thousand() {
    assert_eq!(CurrencyFormat::string(9_999.0, true), "$9,999");
    assert_eq!(CurrencyFormat::string(10_000.0, true), "$10.0k");
    assert_eq!(CurrencyFormat::string(9_999.995, true), "$10.0k");
    assert_eq!(CurrencyFormat::string(9_999.995, false), "$10,000");
    assert_eq!(CurrencyFormat::string(42.0, true), "$42.00");
}

#[test]
fn compact_dollars_stay_in_thousands_above_a_million() {
    assert_eq!(CurrencyFormat::string(1_000_000.0, true), "$1000.0k");
}

#[test]
fn exact_dollars_keep_up_to_four_decimals() {
    assert_eq!(CurrencyFormat::exact(0.00001), "$0.00");
    assert_eq!(CurrencyFormat::exact(0.00005), "$0.0001");
    assert_eq!(CurrencyFormat::exact(0.1), "$0.10");
    assert_eq!(CurrencyFormat::exact(0.123), "$0.123");
    assert_eq!(CurrencyFormat::exact(1234.5), "$1,234.50");
    assert_eq!(CurrencyFormat::exact(-2.0), "-$2.00");
}

#[test]
fn axis_labels() {
    assert_eq!(CurrencyFormat::axis(0.0), "$0");
    assert_eq!(CurrencyFormat::axis(-0.0), "$0");
    assert_eq!(CurrencyFormat::axis(4.5), "$4.50");
    assert_eq!(CurrencyFormat::axis(10.0), "$10");
    assert_eq!(CurrencyFormat::axis(120.0), "$120");
    assert_eq!(CurrencyFormat::axis(999.4), "$999");
    assert_eq!(CurrencyFormat::axis(999.6), "$1.0k");
    assert_eq!(CurrencyFormat::axis(1_500.0), "$1.5k");
    assert_eq!(CurrencyFormat::axis(999_949.0), "$999.9k");
    assert_eq!(CurrencyFormat::axis(999_950.0), "$1.0M");
    assert_eq!(CurrencyFormat::axis(2_300_000.0), "$2.3M");
}

#[test]
fn axis_labels_that_round_up_to_ten_dollars_drop_the_cents() {
    assert_eq!(CurrencyFormat::axis(9.996), "$10");
    assert_eq!(CurrencyFormat::axis(9.994), "$9.99");
}

#[test]
fn axis_labels_keep_the_sign() {
    assert_eq!(CurrencyFormat::axis(-5.0), "-$5.00");
    assert_eq!(CurrencyFormat::axis(-50.0), "-$50");
    assert_eq!(CurrencyFormat::axis(-1_500.0), "-$1.5k");
    assert_eq!(CurrencyFormat::axis(-2_300_000.0), "-$2.3M");
}

#[test]
fn axis_labels_of_non_finite_values_are_zero() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(CurrencyFormat::axis(v), "$0");
    }
}

#[test]
fn token_counts_change_unit_at_each_rounding_boundary() {
    assert_eq!(CurrencyFormat::tokens(0), "0");
    assert_eq!(CurrencyFormat::tokens(999), "999");
    assert_eq!(CurrencyFormat::tokens(1_000), "1.0K");
    assert_eq!(CurrencyFormat::tokens(24_149), "24.1K");
    assert_eq!(CurrencyFormat::tokens(1_000_000), "1.0M");
    assert_eq!(CurrencyFormat::tokens(999_949_999), "999.9M");
    assert_eq!(CurrencyFormat::tokens(1_000_000_000), "1.00B");
}
