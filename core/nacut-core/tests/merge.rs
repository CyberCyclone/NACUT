//! Merging local-log estimates with billed API cost (`merge.rs`).

mod common;

use std::cell::RefCell;
use std::collections::HashMap;

use jiff::civil::{date, Date};
use jiff::tz::{self, TimeZone};
use jiff::{SignedDuration, Timestamp};
use nacut_core::analytics::{ApiCostData, ApiCostRow};
use nacut_core::merge::{Calibration, DataSourceMode, SourceMerger};
use nacut_core::store::{Cal, CostSource, DailySeries, DayCost};

use common::{series, ts};

const OPUS: &str = "claude-opus-5-5";

fn nov(d: i8) -> Date {
    date(2026, 11, d)
}

fn row(start: Timestamp, amount: f64, model: Option<&str>) -> ApiCostRow {
    ApiCostRow { start, end: start + SignedDuration::from_hours(1), model: model.map(String::from), amount, list_amount: amount }
}

/// One billed row at 01:00 UTC on each day.
fn daily_rows(days: impl IntoIterator<Item = i8>, amount: f64) -> Vec<ApiCostRow> {
    days.into_iter().map(|d| row(Cal::utc().start_of(nov(d)) + SignedDuration::from_hours(1), amount, Some(OPUS))).collect()
}

fn api(rows: Vec<ApiCostRow>, refreshed: Option<&str>, coverage_start: &str) -> ApiCostData {
    ApiCostData {
        rows,
        refreshed_at: refreshed.map(ts),
        coverage_start: ts(coverage_start),
        coverage_end: ts("2026-11-20T00:00:00Z"),
        fetched_at: ts("2026-11-20T00:00:00Z"),
    }
}

/// $100 a day of local logs on Nov 10–19.
fn logs() -> DailySeries {
    (10..=19).map(|d| (nov(d), DayCost { cost: 100.0, by_model: HashMap::from([(OPUS.to_string(), 100.0)]), ..Default::default() })).collect()
}

/// $80 billed on Nov 10–18, refreshed Nov 18 at noon, covering from Nov 10 at midnight.
fn billed() -> ApiCostData {
    api(daily_rows(10..=18, 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z")
}

fn utc() -> SourceMerger {
    SourceMerger::new(Cal::utc())
}

fn nothing_after(_: Date, _: Timestamp) -> DayCost {
    DayCost::default()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// MARK: Modes

#[test]
fn logs_mode_ignores_api_data() {
    let (s, c) = utc().merge(DataSourceMode::Logs, &logs(), Some(&billed()), nothing_after);
    assert_eq!(s, logs());
    assert_eq!(c, Calibration::IDENTITY);
}

#[test]
fn without_api_data_every_mode_shows_logs() {
    for mode in [DataSourceMode::Hybrid, DataSourceMode::Api] {
        let (s, c) = utc().merge(mode, &logs(), None, nothing_after);
        assert_eq!(s, logs());
        assert_eq!(c, Calibration::IDENTITY);
    }
}

#[test]
fn hybrid_shows_logs_until_the_api_reports_a_refresh_time() {
    let data = api(daily_rows(10..=18, 80.0), None, "2026-11-10T00:00:00Z");
    let (s, c) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&data), nothing_after);
    assert_eq!(s, logs());
    assert_eq!(c, Calibration::IDENTITY);
}

#[test]
fn mode_titles() {
    assert_eq!(DataSourceMode::default(), DataSourceMode::Logs);
    assert_eq!(DataSourceMode::Logs.title(), "Local logs");
    assert_eq!(DataSourceMode::Api.title(), "Anthropic API only");
    assert_eq!(DataSourceMode::Hybrid.title(), "Hybrid (API + live logs)");
}

// MARK: Calibration

#[test]
fn calibration_needs_three_days_but_reports_what_it_saw() {
    let data = api(daily_rows([10, 11], 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z");
    let c = utc().calibration(&logs(), &data, 14);
    assert_eq!(c, Calibration { ratio: 1.0, sample_days: 2, api_total: 160.0, log_total: 200.0 });
}

#[test]
fn calibration_ratio_is_clamped() {
    let high = api(daily_rows(10..=18, 1_000.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z");
    let low_logs = series(&(10..=18).map(|d| (nov(d), 1.0)).collect::<Vec<_>>());
    assert_eq!(utc().calibration(&low_logs, &high, 14).ratio, 5.0);
    let low = api(daily_rows(10..=18, 0.01), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z");
    assert_eq!(utc().calibration(&logs(), &low, 14).ratio, 0.2);
}

#[test]
fn calibration_uses_the_latest_complete_days_up_to_the_window() {
    let mut data = billed();
    data.rows.push(row(ts("2026-11-17T05:00:00Z"), 10.0, Some(OPUS)));
    let c = utc().calibration(&logs(), &data, 3);
    assert_eq!(c.sample_days, 3);
    assert!(close(c.api_total, 250.0));
    assert!(close(c.log_total, 300.0));
}

#[test]
fn calibration_skips_days_missing_from_either_side() {
    let mut l = logs();
    l.remove(&nov(12));
    let data = api(daily_rows([10, 11, 12, 14, 15, 16, 17], 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z");
    let c = utc().calibration(&l, &data, 14);
    assert_eq!(c.sample_days, 6);
    assert!(close(c.ratio, 0.8));
}

#[test]
fn calibration_excludes_the_refresh_day_and_a_partly_covered_first_day() {
    let data = api(daily_rows(10..=18, 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T06:00:00Z");
    assert_eq!(utc().calibration(&logs(), &data, 14).sample_days, 7);
}

// MARK: Hybrid

#[test]
fn hybrid_replaces_complete_days_with_billed_cost() {
    let (s, _) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&billed()), nothing_after);
    for d in 10..=17 {
        assert_eq!(s[&nov(d)].cost, 80.0, "Nov {d}");
        assert_eq!(s[&nov(d)].source, CostSource::Api);
    }
}

#[test]
fn hybrid_bills_a_complete_day_without_api_rows_as_zero() {
    let data = api(daily_rows([10, 11, 13, 14, 15, 16, 17, 18], 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z");
    let (s, _) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&data), nothing_after);
    assert_eq!(s[&nov(12)].cost, 0.0);
    assert_eq!(s[&nov(12)].source, CostSource::Api);
}

#[test]
fn hybrid_keeps_logs_before_the_api_coverage() {
    let mut l = logs();
    l.insert(nov(5), DayCost::with_cost(7.0));
    let (s, _) = utc().merge(DataSourceMode::Hybrid, &l, Some(&billed()), nothing_after);
    assert_eq!(s[&nov(5)], DayCost::with_cost(7.0));
}

#[test]
fn hybrid_bills_a_partly_covered_first_day_from_the_api_alone() {
    let data = api(daily_rows(10..=18, 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:30:00Z");
    let (s, _) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&data), nothing_after);
    assert_eq!(s[&nov(10)].cost, 80.0);
}

#[test]
fn hybrid_refresh_day_adds_the_scaled_live_estimate_to_the_billed_part() {
    let seen = RefCell::new(Vec::new());
    let live = |day: Date, after: Timestamp| {
        seen.borrow_mut().push((day, after));
        DayCost {
            cost: 50.0,
            by_model: HashMap::from([(OPUS.to_string(), 30.0), ("claude-sonnet-5-5".to_string(), 20.0)]),
            messages: 3,
            tokens: 900,
            source: CostSource::Logs,
        }
    };
    let (s, c) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&billed()), live);
    assert!(close(c.ratio, 0.8));
    assert_eq!(*seen.borrow(), [(nov(18), ts("2026-11-18T12:00:00Z"))]);
    let d = &s[&nov(18)];
    assert!(close(d.cost, 120.0));
    assert!(close(d.by_model[OPUS], 104.0));
    assert!(close(d.by_model["claude-sonnet-5-5"], 16.0));
    assert_eq!((d.messages, d.tokens), (3, 900));
    assert_eq!(d.source, CostSource::Estimated);
}

#[test]
fn hybrid_refresh_day_without_live_usage_is_billed_cost() {
    let (s, _) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&billed()), nothing_after);
    assert_eq!(s[&nov(18)].cost, 80.0);
    assert_eq!(s[&nov(18)].source, CostSource::Api);
}

#[test]
fn hybrid_scales_days_after_the_refresh() {
    let (s, _) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&billed()), nothing_after);
    let d = &s[&nov(19)];
    assert!(close(d.cost, 80.0));
    assert!(close(d.by_model[OPUS], 80.0));
    assert_eq!(d.source, CostSource::Estimated);
}

#[test]
fn hybrid_days_after_the_refresh_stay_logs_when_the_ratio_is_one() {
    let data = api(daily_rows(10..=18, 100.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z");
    let (s, c) = utc().merge(DataSourceMode::Hybrid, &logs(), Some(&data), nothing_after);
    assert_eq!(c.ratio, 1.0);
    assert_eq!(s[&nov(19)].cost, 100.0);
    assert_eq!(s[&nov(19)].source, CostSource::Logs);
}

// MARK: API only

#[test]
fn api_mode_shows_billed_days_and_logs_only_before_coverage() {
    let mut l = logs();
    l.insert(nov(5), DayCost::with_cost(7.0));
    let (s, c) = utc().merge(DataSourceMode::Api, &l, Some(&billed()), |_, _| DayCost::with_cost(1_000.0));
    assert!(close(c.ratio, 0.8));
    assert_eq!(s.keys().copied().collect::<Vec<_>>(), [5, 10, 11, 12, 13, 14, 15, 16, 17, 18].map(nov));
    assert_eq!(s[&nov(5)].cost, 7.0);
    assert_eq!(s[&nov(18)].cost, 80.0);
    assert!(s.values().skip(1).all(|d| d.source == CostSource::Api));
}

#[test]
fn api_mode_bills_a_partly_covered_first_day_from_the_api_alone() {
    let data = api(daily_rows(10..=18, 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:30:00Z");
    let (s, _) = utc().merge(DataSourceMode::Api, &logs(), Some(&data), nothing_after);
    assert_eq!(s[&nov(10)].cost, 80.0);
}

// MARK: API series and reconciliation

#[test]
fn api_series_folds_rows_into_days_by_normalized_model() {
    let rows = vec![
        row(ts("2026-11-10T01:00:00Z"), 5.0, Some("claude-opus-5-5-20261001")),
        row(ts("2026-11-10T02:00:00Z"), 3.0, Some(OPUS)),
        row(ts("2026-11-10T03:00:00Z"), 2.0, None),
        row(ts("2026-11-11T03:00:00Z"), 1.0, Some("us.anthropic.claude-sonnet-5-5-v1:0")),
    ];
    let s = utc().api_series(&api(rows, None, "2026-11-10T00:00:00Z"));
    assert_eq!(s.len(), 2);
    let d = &s[&nov(10)];
    assert_eq!(d.cost, 10.0);
    assert_eq!(d.by_model, HashMap::from([(OPUS.to_string(), 8.0)]));
    assert_eq!((d.messages, d.tokens, d.source), (0, 0, CostSource::Api));
    assert_eq!(s[&nov(11)].by_model, HashMap::from([("claude-sonnet-5-5".to_string(), 1.0)]));
}

#[test]
fn api_series_uses_the_local_day_of_each_row() {
    let merger = SourceMerger::new(Cal::new(TimeZone::fixed(tz::offset(10))));
    let rows = vec![row(ts("2026-11-10T13:00:00Z"), 1.0, Some(OPUS)), row(ts("2026-11-10T14:00:00Z"), 2.0, Some(OPUS))];
    let s = merger.api_series(&api(rows, None, "2026-11-10T00:00:00Z"));
    assert_eq!(s.keys().copied().collect::<Vec<_>>(), [nov(10), nov(11)]);
    assert_eq!((s[&nov(10)].cost, s[&nov(11)].cost), (1.0, 2.0));
}

#[test]
fn complete_days_follow_local_midnights() {
    // Coverage from 00:00 UTC is 10:00 local at UTC+10, so Nov 10 is only partly covered there.
    let merger = SourceMerger::new(Cal::new(TimeZone::fixed(tz::offset(10))));
    let r = merger.reconciliation(&logs(), &billed(), 30);
    assert_eq!(r.first().map(|d| d.date), Some(nov(11)));
    assert_eq!(r.last().map(|d| d.date), Some(nov(17)));
}

#[test]
fn reconciliation_lists_recent_complete_days_oldest_first() {
    let r = utc().reconciliation(&logs(), &billed(), 3);
    assert_eq!(r.iter().map(|d| d.date).collect::<Vec<_>>(), [nov(15), nov(16), nov(17)]);
    assert!(r.iter().all(|d| d.api == 80.0 && d.logs == 100.0));
}

#[test]
fn reconciliation_shows_zero_for_a_missing_side() {
    let mut l = logs();
    l.remove(&nov(16));
    let data = api(daily_rows([10, 11, 12, 13, 14, 15, 16], 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-10T00:00:00Z");
    let r = utc().reconciliation(&l, &data, 2);
    assert_eq!((r[0].date, r[0].api, r[0].logs), (nov(16), 80.0, 0.0));
    assert_eq!((r[1].date, r[1].api, r[1].logs), (nov(17), 0.0, 100.0));
}

#[test]
fn reconciliation_is_empty_without_complete_days() {
    assert!(utc().reconciliation(&logs(), &api(daily_rows(10..=18, 80.0), None, "2026-11-10T00:00:00Z"), 30).is_empty());
    let same_day = api(daily_rows([18], 80.0), Some("2026-11-18T12:00:00Z"), "2026-11-18T00:00:00Z");
    assert!(utc().reconciliation(&logs(), &same_day, 30).is_empty());
    assert!(utc().reconciliation(&logs(), &billed(), 0).is_empty());
}
