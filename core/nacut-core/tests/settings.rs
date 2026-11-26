//! Settings: sanitizing, reading and first-run loading, JSON names, and account detection.

use std::path::{Path, PathBuf};

use nacut_core::merge::DataSourceMode;
use nacut_core::settings::{detect_account, AccountMode, SecondFigure, UsageDisplay};
use nacut_core::{Rates, Settings};
use serde_json::json;

mod common;

fn sanitized(mut s: Settings) -> Settings {
    s.sanitize();
    s
}

fn write(dir: &Path, name: &str, data: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p
}

// MARK: sanitize

#[test]
fn cycle_day_is_clamped_to_1_through_28() {
    assert_eq!(sanitized(Settings { cycle_start_day: 0, ..Settings::default() }).cycle_start_day, 1);
    assert_eq!(sanitized(Settings { cycle_start_day: 31, ..Settings::default() }).cycle_start_day, 28);
    assert_eq!(sanitized(Settings { cycle_start_day: 15, ..Settings::default() }).cycle_start_day, 15);
}

#[test]
fn thresholds_outside_1_to_1000_are_dropped_then_sorted_and_deduped() {
    let s = sanitized(Settings { alert_thresholds: vec![100, 0, 50, 1001, 1000, 50, 1], ..Settings::default() });
    assert_eq!(s.alert_thresholds, [1, 50, 100, 1000]);
}

#[test]
fn negative_money_becomes_zero() {
    let s = sanitized(Settings { plan_price: -5.0, monthly_budget: -1.0, ..Settings::default() });
    assert_eq!((s.plan_price, s.monthly_budget), (0.0, 0.0));
}

#[test]
fn infinite_plan_price_becomes_zero() {
    assert_eq!(sanitized(Settings { plan_price: f64::INFINITY, ..Settings::default() }).plan_price, 0.0);
}

#[test]
fn unsanitized_nan_is_written_as_null_and_the_file_no_longer_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    Settings { monthly_budget: f64::NAN, ..Settings::default() }.save(&path).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["monthlyBudget"], serde_json::Value::Null);
    assert_eq!(Settings::read(&path), None);
    // Sanitizing first keeps the file readable.
    sanitized(Settings { monthly_budget: f64::NAN, ..Settings::default() }).save(&path).unwrap();
    assert_eq!(Settings::read(&path).unwrap().monthly_budget, 0.0);
}

// MARK: read / load

#[test]
fn read_of_a_missing_file_is_none_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Settings::read(&dir.path().join("settings.json")), None);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn read_of_an_invalid_file_is_none_and_leaves_it_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "settings.json", "{ nope");
    assert_eq!(Settings::read(&path), None);
    assert_eq!(std::fs::read(&path).unwrap(), b"{ nope");
    assert!(!dir.path().join("settings.invalid.json").exists());
}

#[test]
fn read_sanitizes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "settings.json", r#"{"cycleStartDay":40,"alertThresholds":[5000,80]}"#);
    let s = Settings::read(&path).unwrap();
    assert_eq!((s.cycle_start_day, s.alert_thresholds), (28, vec![80]));
}

#[test]
fn first_load_saves_detected_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let path = dir.path().join("cfg").join("settings.json");
    let s = Settings::load_in(&path, &home);
    assert_eq!(s, Settings::default());
    assert_eq!(Settings::read(&path), Some(Settings::default()));
}

#[test]
fn first_load_detects_a_subscription_from_home() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".claude.json", r#"{"oauthAccount":{"organizationType":"claude_max","organizationRateLimitTier":"default_claude_max_20x"}}"#);
    let path = dir.path().join("settings.json");
    let s = Settings::load_in(&path, dir.path());
    assert_eq!((s.account_mode, s.plan_price, s.limits_enabled), (AccountMode::Subscription, 200.0, true));
    assert_eq!(Settings::read(&path).unwrap(), s);
}

#[test]
fn load_sanitizes_an_existing_file_without_rewriting_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "settings.json", r#"{"cycleStartDay":0}"#);
    assert_eq!(Settings::load_in(&path, dir.path()).cycle_start_day, 1);
    assert_eq!(std::fs::read(&path).unwrap(), br#"{"cycleStartDay":0}"#);
}

#[test]
fn partial_json_fills_defaults_and_ignores_unknown_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "settings.json", r#"{"monthlyBudget":75,"someFutureSetting":true}"#);
    let s = Settings::read(&path).unwrap();
    assert_eq!(s, Settings { monthly_budget: 75.0, ..Settings::default() });
}

#[test]
fn wrong_types_make_the_whole_file_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "settings.json", r#"{"monthlyBudget":"75"}"#);
    assert_eq!(Settings::read(&path), None);
}

#[test]
fn save_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut s = Settings { account_mode: AccountMode::Subscription, api_actor_id: Some("u1".into()), extra_roots: vec!["/x".into()], ..Settings::default() };
    s.price_overrides.insert("claude-opus-5-5".into(), Rates { input: 1.0, output: 2.0, write5m: 3.0, write1h: 4.0, read: 5.0 });
    s.save(&path).unwrap();
    assert_eq!(Settings::read(&path), Some(s));
}

// MARK: JSON names

#[test]
fn json_field_and_enum_names_are_camel_case() {
    let s = Settings {
        account_mode: AccountMode::Subscription,
        second_figure: SecondFigure::AllTime,
        usage_display: UsageDisplay::Cost,
        data_source: DataSourceMode::Hybrid,
        ..Settings::default()
    };
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v["accountMode"], "subscription");
    assert_eq!(v["secondFigure"], "allTime");
    assert_eq!(v["usageDisplay"], "cost");
    assert_eq!(v["dataSource"], "hybrid");
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    for k in [
        "planPrice", "monthlyBudget", "cycleStartDay", "compactMenuBar", "apiActorId", "apiActorName", "limitsEnabled",
        "autoRefreshSignIn", "alertsEnabled", "alertThresholds", "forecastAlert", "limitAlerts", "extraRoots", "includeWsl",
        "priceOverrides",
    ] {
        assert!(keys.contains(&k), "{k} missing from {keys:?}");
    }
    assert_eq!(keys.len(), 19);
}

#[test]
fn enum_values_parse_from_their_json_names() {
    assert_eq!(serde_json::from_value::<AccountMode>(json!("api")).unwrap(), AccountMode::Api);
    assert_eq!(serde_json::from_value::<SecondFigure>(json!("none")).unwrap(), SecondFigure::None);
    assert_eq!(serde_json::from_value::<SecondFigure>(json!("cycle")).unwrap(), SecondFigure::Cycle);
    assert_eq!(serde_json::from_value::<UsageDisplay>(json!("limits")).unwrap(), UsageDisplay::Limits);
    assert_eq!(serde_json::from_value::<DataSourceMode>(json!("logs")).unwrap(), DataSourceMode::Logs);
    assert_eq!(serde_json::from_value::<DataSourceMode>(json!("api")).unwrap(), DataSourceMode::Api);
    assert!(serde_json::from_value::<AccountMode>(json!("Api")).is_err());
}

// MARK: Derived

#[test]
fn shows_limits_needs_subscription_limits_display_and_limits_enabled() {
    let on = Settings { account_mode: AccountMode::Subscription, usage_display: UsageDisplay::Limits, limits_enabled: true, ..Settings::default() };
    assert!(on.is_subscription());
    assert!(on.shows_limits());
    assert!(!Settings { account_mode: AccountMode::Api, ..on.clone() }.shows_limits());
    assert!(!Settings { usage_display: UsageDisplay::Cost, ..on.clone() }.shows_limits());
    assert!(!Settings { limits_enabled: false, ..on }.shows_limits());
}

#[test]
fn plan_names_round_the_price() {
    let name = |p: f64| Settings { plan_price: p, ..Settings::default() }.plan_name();
    assert_eq!(name(200.0), "Max 20x");
    assert_eq!(name(99.6), "Max 5x");
    assert_eq!(name(100.4), "Max 5x");
    assert_eq!(name(20.0), "Pro");
    assert_eq!(name(0.0), "Subscription");
    assert_eq!(name(150.0), "Subscription");
}

#[test]
fn extra_root_paths_trim_and_skip_blanks() {
    let s = Settings { extra_roots: vec!["  /a/b  ".into(), "".into(), "   ".into(), "/c".into()], ..Settings::default() };
    assert_eq!(s.extra_root_paths(), [PathBuf::from("/a/b"), PathBuf::from("/c")]);
}

// MARK: Account detection

fn detect(json: &str) -> Option<(AccountMode, Option<f64>)> {
    let dir = tempfile::tempdir().unwrap();
    detect_account(&write(dir.path(), ".claude.json", json))
}

#[test]
fn max_20x_is_a_200_dollar_subscription() {
    assert_eq!(
        detect(r#"{"oauthAccount":{"organizationType":"claude_max","organizationRateLimitTier":"default_claude_max_20x"}}"#),
        Some((AccountMode::Subscription, Some(200.0)))
    );
}

#[test]
fn max_without_a_tier_is_100_dollars() {
    assert_eq!(detect(r#"{"oauthAccount":{"organizationType":"claude_max"}}"#), Some((AccountMode::Subscription, Some(100.0))));
    assert_eq!(
        detect(r#"{"oauthAccount":{"organizationType":"claude_max","organizationRateLimitTier":"default_claude_max_5x"}}"#),
        Some((AccountMode::Subscription, Some(100.0)))
    );
}

#[test]
fn pro_is_20_dollars() {
    assert_eq!(detect(r#"{"oauthAccount":{"organizationType":"claude_pro"}}"#), Some((AccountMode::Subscription, Some(20.0))));
}

#[test]
fn other_organizations_are_api_accounts() {
    assert_eq!(detect(r#"{"oauthAccount":{"organizationType":"claude_enterprise"}}"#), Some((AccountMode::Api, None)));
    assert_eq!(detect(r#"{"oauthAccount":{}}"#), Some((AccountMode::Api, None)));
}

#[test]
fn no_account_or_invalid_json_detects_nothing() {
    assert_eq!(detect(r#"{"numStartups":3}"#), None);
    assert_eq!(detect("{ nope"), None);
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(detect_account(&dir.path().join(".claude.json")), None);
}

#[test]
fn detected_in_home_turns_limits_on_only_for_subscriptions() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".claude.json", r#"{"oauthAccount":{"organizationType":"claude_pro"}}"#);
    let s = Settings::detected_in(dir.path());
    assert_eq!((s.account_mode, s.plan_price, s.limits_enabled), (AccountMode::Subscription, 20.0, true));

    write(dir.path(), ".claude.json", r#"{"oauthAccount":{"organizationType":"claude_enterprise"}}"#);
    let s = Settings::detected_in(dir.path());
    assert_eq!((s.account_mode, s.plan_price, s.limits_enabled), (AccountMode::Api, 100.0, false));

    let empty = tempfile::tempdir().unwrap();
    assert_eq!(Settings::detected_in(empty.path()), Settings::default());
}
