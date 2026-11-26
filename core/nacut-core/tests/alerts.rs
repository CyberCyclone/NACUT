//! Budget and plan-limit alerts and the fired-alert ledger (`alerts.rs`).

mod common;

use jiff::civil::date;
use jiff::Timestamp;
use nacut_core::alerts::{test_alert, Alert, AlertLedger};
use nacut_core::settings::AccountMode;
use nacut_core::store::{Cal, DailySeries};
use nacut_core::{Forecast, PlanLimit, Settings};

use common::{series, ts};

/// Wednesday 2026-11-11 12:00 UTC.
fn now() -> Timestamp {
    ts("2026-11-11T12:00:00Z")
}

/// $50 a day for Nov 1–10 and $25 so far today: $525 spent, $1,500 projected.
fn fifty_a_day() -> DailySeries {
    let mut pairs: Vec<_> = (1..=10).map(|d| (date(2026, 11, d), 50.0)).collect();
    pairs.push((date(2026, 11, 11), 25.0));
    series(&pairs)
}

fn forecast(s: &DailySeries, budget: f64) -> Forecast {
    Forecast::new(s, budget, 1, now(), &Cal::utc())
}

/// Spent exactly `spent` on Nov 1, viewed on Nov 11.
fn spent(spent: f64, budget: f64) -> Forecast {
    forecast(&series(&[(date(2026, 11, 1), spent)]), budget)
}

fn budget_only() -> Settings {
    Settings { forecast_alert: false, ..Settings::default() }
}

fn limits_on() -> Settings {
    Settings { limits_enabled: true, ..Settings::default() }
}

fn limit(id: &str, label: &str, utilization: f64, resets_at: Option<&str>) -> PlanLimit {
    PlanLimit { id: id.into(), label: label.into(), utilization, resets_at: resets_at.map(ts) }
}

fn five(utilization: f64) -> PlanLimit {
    limit("five_hour", "5-hour session", utilization, Some("2026-11-11T15:00:00Z"))
}

fn evaluate(f: &Forecast, limits: &[PlanLimit], settings: &Settings) -> Vec<Alert> {
    AlertLedger::in_memory().evaluate(f, limits, settings, now(), &Cal::utc())
}

fn titles(alerts: &[Alert]) -> Vec<&str> {
    alerts.iter().map(|a| a.title.as_str()).collect()
}

// MARK: Budget thresholds

#[test]
fn nothing_fires_while_alerts_are_off() {
    let settings = Settings { alerts_enabled: false, ..limits_on() };
    let mut ledger = AlertLedger::in_memory();
    assert!(ledger.evaluate(&spent(2_000.0, 1_000.0), &[five(0.95)], &settings, now(), &Cal::utc()).is_empty());
    // Nothing was recorded, so turning alerts on fires them.
    assert_eq!(ledger.evaluate(&spent(2_000.0, 1_000.0), &[five(0.95)], &limits_on(), now(), &Cal::utc()).len(), 4);
}

#[test]
fn every_crossed_threshold_fires_in_order() {
    let a = evaluate(&spent(1_200.0, 1_000.0), &[], &budget_only());
    assert_eq!(titles(&a), ["Claude budget 50% used", "Claude budget 80% used", "Claude budget reached"]);
    assert_eq!(a.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["budget-2026-11-01-50", "budget-2026-11-01-80", "budget-2026-11-01-100"]);
    assert!(a.iter().all(|x| x.body == "$1,200 of $1,000 spend this cycle."));
}

#[test]
fn a_threshold_fires_exactly_at_its_percentage() {
    let settings = Settings { alert_thresholds: vec![29], ..budget_only() };
    assert_eq!(titles(&evaluate(&spent(290.0, 1_000.0), &[], &settings)), ["Claude budget 29% used"]);
    assert!(evaluate(&spent(289.99, 1_000.0), &[], &settings).is_empty());
}

#[test]
fn thresholds_above_100_percent_read_as_reached() {
    let settings = Settings { alert_thresholds: vec![150], ..budget_only() };
    assert_eq!(titles(&evaluate(&spent(1_500.0, 1_000.0), &[], &settings)), ["Claude budget reached"]);
}

#[test]
fn subscriptions_call_it_api_equivalent_usage() {
    let settings = Settings { account_mode: AccountMode::Subscription, ..budget_only() };
    let a = evaluate(&forecast(&fifty_a_day(), 1_000.0), &[], &settings);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].body, "$525 of $1,000 API-equivalent usage this cycle.");
}

#[test]
fn no_budget_alerts_without_a_budget() {
    assert!(evaluate(&spent(5_000.0, 0.0), &[], &Settings::default()).is_empty());
}

#[test]
fn a_new_cycle_fires_the_thresholds_again() {
    let mut ledger = AlertLedger::in_memory();
    let cal = Cal::utc();
    let nov = spent(600.0, 1_000.0);
    assert_eq!(ledger.evaluate(&nov, &[], &budget_only(), now(), &cal).len(), 1);
    assert!(ledger.evaluate(&nov, &[], &budget_only(), now(), &cal).is_empty());
    let dec_now = ts("2026-12-05T12:00:00Z");
    let dec = Forecast::new(&series(&[(date(2026, 12, 1), 600.0)]), 1_000.0, 1, dec_now, &cal);
    let a = ledger.evaluate(&dec, &[], &budget_only(), dec_now, &cal);
    assert_eq!(a.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["budget-2026-12-01-50"]);
}

// MARK: Forecast alert

#[test]
fn forecast_alert_names_the_day_the_budget_runs_out() {
    let a = evaluate(&forecast(&fifty_a_day(), 1_000.0), &[], &Settings::default());
    assert_eq!(titles(&a), ["Claude budget 50% used", "On pace to exceed your Claude budget"]);
    assert_eq!(a[1].id, "forecast-2026-11-01");
    assert_eq!(a[1].body, "At the current rate you'll hit $1,000 around Saturday, November 21. Projected: $1,500.");
}

#[test]
fn forecast_alert_follows_its_setting() {
    assert_eq!(titles(&evaluate(&forecast(&fifty_a_day(), 1_000.0), &[], &budget_only())), ["Claude budget 50% used"]);
}

#[test]
fn forecast_alert_needs_an_exhaustion_date() {
    let mut f = forecast(&fifty_a_day(), 1_000.0);
    f.exhaustion_date = None;
    assert_eq!(titles(&evaluate(&f, &[], &Settings::default())), ["Claude budget 50% used"]);
}

#[test]
fn forecast_alert_only_fires_when_the_budget_will_be_exceeded() {
    assert!(evaluate(&forecast(&fifty_a_day(), 1_600.0), &[], &Settings { alert_thresholds: vec![], ..Settings::default() }).is_empty());
}

// MARK: Limit alerts

#[test]
fn limit_alerts_need_plan_limits_enabled() {
    let off = Settings { limits_enabled: false, ..Settings::default() };
    assert!(evaluate(&spent(0.0, 0.0), &[five(0.95)], &off).is_empty());
    let muted = Settings { limit_alerts: false, ..limits_on() };
    assert!(evaluate(&spent(0.0, 0.0), &[five(0.95)], &muted).is_empty());
}

#[test]
fn limit_alert_fires_from_90_percent() {
    assert!(evaluate(&spent(0.0, 0.0), &[five(0.89)], &limits_on()).is_empty());
    let a = evaluate(&spent(0.0, 0.0), &[five(0.9)], &limits_on());
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].id, "limit-five_hour-2026-11-11T15:00:00Z");
    assert_eq!(a[0].title, "5-hour session limit at 90%");
    assert_eq!(a[0].body, "Plan limit resets in 3h 0m.");
}

#[test]
fn limit_alert_is_silent_once_the_window_has_reset() {
    let reset = limit("five_hour", "5-hour session", 0.95, Some("2026-11-11T11:00:00Z"));
    assert!(evaluate(&spent(0.0, 0.0), &[reset], &limits_on()).is_empty());
}

#[test]
fn limit_alert_without_a_reset_time_has_a_generic_body() {
    let a = evaluate(&spent(0.0, 0.0), &[limit("seven_day", "Weekly · all models", 0.92, None)], &limits_on());
    assert_eq!(a[0].id, "limit-seven_day-none");
    assert_eq!(a[0].body, "You're close to your plan limit.");
}

#[test]
fn limit_alert_title_rounds_the_percentage() {
    let a = evaluate(&spent(0.0, 0.0), &[limit("seven_day", "Weekly · all models", 1.13, Some("2026-11-13T12:00:00Z"))], &limits_on());
    assert_eq!(a[0].title, "Weekly · all models limit at 113%");
    assert_eq!(a[0].body, "Plan limit resets Fri 12 PM.");
}

#[test]
fn limit_alert_fires_again_in_a_new_window() {
    let mut ledger = AlertLedger::in_memory();
    let cal = Cal::utc();
    let f = spent(0.0, 0.0);
    assert_eq!(ledger.evaluate(&f, &[five(0.95)], &limits_on(), now(), &cal).len(), 1);
    assert!(ledger.evaluate(&f, &[five(0.97)], &limits_on(), now(), &cal).is_empty());
    let next = limit("five_hour", "5-hour session", 0.95, Some("2026-11-11T20:00:00Z"));
    let later = ts("2026-11-11T16:00:00Z");
    let a = ledger.evaluate(&f, &[next], &limits_on(), later, &cal);
    assert_eq!(a.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["limit-five_hour-2026-11-11T20:00:00Z"]);
}

#[test]
fn budget_alerts_come_before_limit_alerts() {
    let a = evaluate(&spent(600.0, 1_000.0), &[five(0.95)], &Settings { forecast_alert: false, ..limits_on() });
    assert_eq!(titles(&a), ["Claude budget 50% used", "5-hour session limit at 95%"]);
}

// MARK: Ledger file

#[test]
fn fired_alerts_survive_a_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alerts-fired.json");
    let f = spent(600.0, 1_000.0);
    let mut first = AlertLedger::load(&path);
    assert_eq!(first.evaluate(&f, &[], &budget_only(), now(), &Cal::utc()).len(), 1);
    let saved: Vec<String> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved, ["budget-2026-11-01-50"]);
    let mut reloaded = AlertLedger::load(&path);
    assert!(reloaded.evaluate(&f, &[], &budget_only(), now(), &Cal::utc()).is_empty());
}

#[test]
fn a_corrupt_ledger_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alerts-fired.json");
    std::fs::write(&path, b"{ not json").unwrap();
    let mut ledger = AlertLedger::load(&path);
    assert_eq!(ledger.evaluate(&spent(600.0, 1_000.0), &[], &budget_only(), now(), &Cal::utc()).len(), 1);
    let saved: Vec<String> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved, ["budget-2026-11-01-50"]);
}

#[test]
fn the_ledger_is_only_written_when_something_fires() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("alerts-fired.json");
    let mut ledger = AlertLedger::load(&path);
    assert!(ledger.evaluate(&spent(100.0, 1_000.0), &[], &budget_only(), now(), &Cal::utc()).is_empty());
    assert!(!path.exists());
    assert_eq!(ledger.evaluate(&spent(600.0, 1_000.0), &[], &budget_only(), now(), &Cal::utc()).len(), 1);
    assert!(path.exists());
}

#[test]
fn test_alert_has_a_unique_test_id() {
    let a = test_alert();
    assert!(a.id.starts_with("test-"), "{}", a.id);
    assert!(a.id["test-".len()..].parse::<i64>().is_ok(), "{}", a.id);
    assert_eq!(a.title, "NACUT alerts are on");
    assert_eq!(a.body, "You'll be notified at your budget thresholds.");
}
