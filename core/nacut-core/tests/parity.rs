//! The Swift `UsageCoreTests` suite, ported: same fixtures (`shared/fixtures`), same expected values.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

use jiff::civil::date;
use jiff::Timestamp;
use nacut_core::analytics::{actor_from, AnalyticsClient, ApiCostData, ApiCostRow, ConnectionStatus, BETA_HEADER};
use nacut_core::forecast::{BillingCycle, BudgetStatus, CurrencyFormat, Forecast};
use nacut_core::insights::Insights;
use nacut_core::merge::{DataSourceMode, SourceMerger};
use nacut_core::pricing::{PriceBook, Rates};
use nacut_core::record::{project_path, UsageRecord};
use nacut_core::store::{Cal, CostSource, DailySeries, DayCost, SessionStat, UsageStore};
use nacut_core::{LogCache, LogScanner, PlanLimit};

mod common;

use common::{fixtures, scan_fixtures, series, ts, Pages, Shared};

// MARK: Parsing

#[test]
fn dedup_keeps_final_line_and_skips_junk() {
    let cache = scan_fixtures();
    let ids: HashSet<&str> = cache.records.keys().map(String::as_str).collect();
    assert_eq!(ids, HashSet::from(["msg_1", "msg_2", "msg_3", "msg_4", "msg_9"]));
    let r = |id: &str| &cache.records[id];
    assert_eq!(r("msg_1").output, 500);
    assert_eq!(r("msg_1").cache_write_1h, 1000);
    assert_eq!(r("msg_1").cache_write_5m, 0);
    assert!(r("msg_9").is_subagent);
    assert!(r("msg_2").fast);
    assert_eq!(r("msg_4").web_searches, 3);
    assert_eq!(r("msg_1").entrypoint, "claude-desktop");
}

#[test]
fn partial_last_line_is_read_later() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    let line1 = r#"{"type":"assistant","timestamp":"2026-11-20T01:00:00Z","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":1}}}"#;
    let line2 = r#"{"type":"assistant","timestamp":"2026-11-20T02:00:00Z","message":{"id":"m2","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":1}}}"#;
    std::fs::write(&file, format!("{line1}\n{}", &line2[..40])).unwrap();
    let r1 = LogScanner::scan_file(&file, None).unwrap();
    assert_eq!(r1.records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["m1"]);

    let mut h = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
    write!(h, "{}\n", &line2[40..]).unwrap();
    drop(h);
    let r2 = LogScanner::scan_file(&file, Some(r1.checkpoint)).unwrap();
    assert_eq!(r2.records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["m2"]);
    assert!(LogScanner::scan_file(&file, Some(r2.checkpoint)).is_none());
}

#[test]
fn worktrees_fold_into_repo() {
    assert_eq!(project_path("/Users/me/Code/app/.claude/worktrees/x"), "/Users/me/Code/app");
}

#[test]
fn cache_round_trip() {
    let cache = scan_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.json");
    cache.save(&path).unwrap();
    let loaded = LogCache::load(&path);
    assert_eq!(loaded.records.len(), cache.records.len());
    assert_eq!(loaded.checkpoints, cache.checkpoints);
    assert_eq!(loaded.records["msg_1"], cache.records["msg_1"]);
}

#[test]
fn default_roots_include_config_dirs() {
    let home = PathBuf::from("/home/me");
    let roots = LogScanner::default_roots_with(&home, Some("/opt/a, /opt/b"));
    assert_eq!(roots.len(), 4);
    assert_eq!(roots[0], home.join(".claude").join("projects"));
    assert_eq!(roots[3], PathBuf::from("/opt/b").join("projects"));
}

// MARK: Pricing

#[test]
fn normalization() {
    assert_eq!(PriceBook::normalize("claude-sonnet-5-5-20261001"), "claude-sonnet-5-5");
    assert_eq!(PriceBook::normalize("claude-opus-5-5[1m]"), "claude-opus-5-5");
    assert_eq!(PriceBook::normalize("claude-mythos-5-2"), "claude-fable-5-2");
    assert_eq!(PriceBook::normalize("anthropic.claude-opus-5-5"), "claude-opus-5-5");
    assert_eq!(PriceBook::normalize("us.anthropic.claude-opus-5-5-v1:0"), "claude-opus-5-5");
    assert_eq!(PriceBook::bundled().display_name("claude-opus-7-1"), "Opus 7.1");
}

#[test]
fn fixture_costs() {
    let cache = scan_fixtures();
    let mut records: Vec<UsageRecord> = cache.records.into_values().collect();
    PriceBook::bundled().price_all(&mut records);
    let p: HashMap<&str, &UsageRecord> = records.iter().map(|r| (r.id.as_str(), r)).collect();
    assert!((p["msg_1"].cost - 0.01844).abs() < 1e-9, "1h cache write at $8");
    assert!((p["msg_2"].cost - 8.0).abs() < 1e-9, "fast ×2");
    assert!((p["msg_3"].cost - 2.575).abs() < 1e-9, "Haiku 5.5 >100K tier");
    assert!((p["msg_4"].cost - 4.03).abs() < 1e-9, "unknown opus → newest opus + web search");
    assert!(p["msg_4"].estimated);
    assert!((p["msg_9"].cost - 12.1).abs() < 1e-9);
    assert!(!p["msg_9"].estimated);
}

#[test]
fn overrides() {
    let r = UsageRecord::new("x", Timestamp::now(), "claude-opus-5-5", 1_000_000, 0);
    let rates = Rates { input: 1.0, output: 1.0, write5m: 1.0, write1h: 1.0, read: 1.0 };
    let b = PriceBook::bundled().with_overrides(HashMap::from([("claude-opus-5-5".to_string(), rates)]));
    assert_eq!(b.cost(&r).0, 1.0);
}

// MARK: Store

#[test]
fn aggregates() {
    let cache = scan_fixtures();
    let store = UsageStore::new(cache.records.into_values().collect(), PriceBook::bundled().clone(), Cal::utc());
    let all = nacut_core::Interval::new(ts("2026-11-01T00:00:00Z"), ts("2026-12-01T00:00:00Z"));
    assert!((store.cost_in(all) - 26.72344).abs() < 1e-9);
    assert!((store.log_series[&date(2026, 11, 20)].cost - 14.62344).abs() < 1e-9);
    let models = store.model_stats(all);
    assert_eq!(models[0].id, "claude-sonnet-5-5");
    assert!((models.iter().map(|m| m.share).sum::<f64>() - 1.0).abs() < 1e-9);
    assert!((store.agent_split(all).1 - 12.1).abs() < 1e-9);
    assert_eq!(store.project_stats(all).len(), 1);
    let classes = store.token_classes(all);
    assert!((classes.iter().map(|c| c.value).sum::<f64>() + 0.03 - 26.72344).abs() < 1e-9);
    assert!(store.cache_savings(all) > 0.0);
    // Hourly and heatmap use the same calendar.
    assert_eq!(store.hourly(date(2026, 11, 20)).iter().map(|h| h.hour).collect::<Vec<_>>(), [1, 2, 4, 5]);
    let heat: f64 = store.weekday_hour_heatmap(all).iter().flatten().sum();
    assert!((heat - 26.72344).abs() < 1e-9);
}

// MARK: Forecast

#[test]
fn cycle_boundaries() {
    let cal = Cal::utc();
    let c = BillingCycle::containing(ts("2026-11-10T12:00:00Z"), 15, &cal);
    assert_eq!((c.start, c.end), (date(2026, 10, 15), date(2026, 11, 15)));
    let c2 = BillingCycle::containing(ts("2026-11-15T00:00:00Z"), 15, &cal);
    assert_eq!(c2.start, date(2026, 11, 15));
    let c3 = BillingCycle::containing(ts("2026-02-10T00:00:00Z"), 31, &cal);
    assert_eq!(c3.start, date(2026, 1, 28));
}

#[test]
fn projection_and_exhaustion() {
    // $50/day for 10 full days, now noon on day 11 with $25 so far.
    let mut pairs: Vec<_> = (1..=10).map(|d| (date(2026, 11, d), 50.0)).collect();
    pairs.push((date(2026, 11, 11), 25.0));
    let f = Forecast::new(&series(&pairs), 1000.0, 1, ts("2026-11-11T12:00:00Z"), &Cal::utc());
    assert_eq!(f.spent, 525.0);
    assert_eq!(f.today, 25.0);
    assert!((f.elapsed_days - 10.5).abs() < 1e-9);
    assert!((f.daily_average - 50.0).abs() < 1e-9);
    assert!((f.projected - (525.0 + 50.0 * 19.5)).abs() < 1e-6);
    assert_eq!(f.status, BudgetStatus::WillExceed);
    // (1000-525)/50 = 9.5 days after Nov 11 12:00 → Nov 21 00:00
    assert_eq!(f.exhaustion_date, Some(ts("2026-11-21T00:00:00Z")));
    assert!((f.safe_daily_spend.unwrap() - 475.0 / 19.5).abs() < 1e-9);
    assert_eq!(f.headline(&Cal::utc()), "Budget runs out ~Nov 21");
}

#[test]
fn under_budget_and_exceeded() {
    let cal = Cal::utc();
    let low = Forecast::new(&series(&[(date(2026, 11, 1), 1.0)]), 1000.0, 1, ts("2026-11-05T12:00:00Z"), &cal);
    assert_eq!(low.status, BudgetStatus::UnderBudget);
    let over = Forecast::new(&series(&[(date(2026, 11, 1), 600.0), (date(2026, 11, 2), 600.0)]), 1000.0, 1, ts("2026-11-03T12:00:00Z"), &cal);
    assert_eq!(over.status, BudgetStatus::Exceeded);
    assert_eq!(over.exhaustion_date, Some(ts("2026-11-02T00:00:00Z")));
    let none = Forecast::new(&DailySeries::new(), 0.0, 1, Timestamp::now(), &cal);
    assert_eq!(none.status, BudgetStatus::NoBudget);
}

#[test]
fn insights() {
    let cal = Cal::utc();
    let s = series(&[(date(2026, 11, 9), 5.0), (date(2026, 11, 10), 10.0), (date(2026, 11, 11), 3.0), (date(2026, 10, 10), 4.0)]);
    let now = ts("2026-11-11T12:00:00Z");
    let cycle = BillingCycle::containing(now, 1, &cal);
    let i = Insights::new(&s, &cycle, now, &cal);
    assert_eq!(i.active_streak, 3);
    assert_eq!(i.peak_day.unwrap().cost, 10.0);
    assert_eq!(i.all_time, 22.0);
    assert_eq!(i.previous_cycle_total, 4.0);
    assert_eq!(i.vs_previous_cycle, Some((18.0 - 4.0) / 4.0));
}

// MARK: API

#[test]
fn paging_cents_and_410_restart() {
    let p1 = r#"{"data":[{"starting_at":"2026-11-20T01:00:00Z","ending_at":"2026-11-20T02:00:00Z","group":{"model":"claude-opus-5-5"},"cost":{"amount":"41280.000000","list_amount":"51600.000000","currency":"USD"}}],"has_more":true,"next_page":"page_2","data_refreshed_at":"2026-11-24T16:02:11Z"}"#;
    let p2 = r#"{"data":[{"starting_at":"2026-11-20T02:00:00Z","ending_at":"2026-11-20T03:00:00Z","group":{"model":null},"cost":{"amount":"100.000000","list_amount":"100.000000","currency":"USD"}}],"has_more":false,"next_page":null,"data_refreshed_at":"2026-11-24T16:02:11Z"}"#;
    let pages = Pages::new(vec![(200, p1), (410, "{}"), (200, p1), (200, p2)]);
    let client = AnalyticsClient::with_transport("k", Box::new(Shared(pages.clone())));
    let data = client.hourly_cost(ts("2026-11-20T00:00:00Z"), ts("2026-11-25T00:00:00Z"), None).unwrap();
    assert_eq!(data.rows.len(), 2);
    assert_eq!(data.rows[0].amount, 412.80);
    assert_eq!(data.rows[0].list_amount, 516.00);
    assert_eq!(data.rows[1].model, None);
    assert_eq!(data.refreshed_at, Some(ts("2026-11-24T16:02:11Z")));
    let calls = pages.calls.lock().unwrap();
    assert_eq!(calls[0].header("anthropic-beta"), Some(BETA_HEADER));
    assert_eq!(calls[0].header("x-api-key"), Some("k"));
    assert!(calls[0].url.contains("time_bucket_width=1h"));
    assert!(calls[3].url.contains("page=page_2"));
}

#[test]
fn status_mapping() {
    let pages = Pages::new(vec![(403, r#"{"error":{"type":"permission_error","message":"missing read:analytics"}}"#)]);
    let s = AnalyticsClient::with_transport("k", Box::new(Shared(pages))).test_connection(Timestamp::now());
    assert_eq!(s, ConnectionStatus::Forbidden("missing read:analytics".into()));
    let empty = Pages::new(vec![(200, r#"{"data":[],"has_more":false,"data_refreshed_at":null}"#)]);
    let s2 = AnalyticsClient::with_transport("k", Box::new(Shared(empty))).test_connection(Timestamp::now());
    assert_eq!(s2, ConnectionStatus::NoDataYet);
}

#[test]
fn actor_parsing() {
    let v: serde_json::Value = serde_json::from_str(r#"{"type":"user","user_id":"user_1","email_address":"a@b.c"}"#).unwrap();
    let a = actor_from(Some(&v)).unwrap();
    assert_eq!(a.id, "user_1");
    assert_eq!(a.display, "a@b.c");
}

#[test]
fn hybrid_merge_and_calibration() {
    let cal = Cal::utc();
    let merger = SourceMerger::new(cal.clone());
    let mut logs = DailySeries::new();
    let mut rows = vec![];
    for d in 10..=19 {
        let day = date(2026, 11, d);
        logs.insert(day, DayCost { cost: 100.0, by_model: HashMap::from([("claude-opus-5-5".into(), 100.0)]), ..Default::default() });
        if d < 19 {
            let start = cal.start_of(day) + jiff::SignedDuration::from_hours(1);
            rows.push(ApiCostRow { start, end: start + jiff::SignedDuration::from_hours(1), model: Some("claude-opus-5-5".into()), amount: 80.0, list_amount: 100.0 });
        }
    }
    // API refreshed at Nov 18 12:00; it has 80 billed on Nov 18 before that.
    let api = ApiCostData {
        rows,
        refreshed_at: Some(ts("2026-11-18T12:00:00Z")),
        coverage_start: ts("2026-11-10T00:00:00Z"),
        coverage_end: ts("2026-11-20T00:00:00Z"),
        fetched_at: Timestamp::now(),
    };
    let c = merger.calibration(&logs, &api, 14);
    assert_eq!(c.sample_days, 8);
    assert!((c.ratio - 0.8).abs() < 1e-9);

    let (s, _) = merger.merge(DataSourceMode::Hybrid, &logs, Some(&api), |_, _| DayCost::with_cost(50.0));
    assert_eq!(s[&date(2026, 11, 12)].cost, 80.0);
    assert_eq!(s[&date(2026, 11, 12)].source, CostSource::Api);
    assert!((s[&date(2026, 11, 18)].cost - (80.0 + 40.0)).abs() < 1e-9);
    assert!((s[&date(2026, 11, 19)].cost - 80.0).abs() < 1e-9);
    assert_eq!(s[&date(2026, 11, 19)].source, CostSource::Estimated);

    let (api_only, _) = merger.merge(DataSourceMode::Api, &logs, Some(&api), |_, _| DayCost::default());
    assert!(!api_only.contains_key(&date(2026, 11, 19)));
    assert_eq!(merger.reconciliation(&logs, &api, 30).len(), 8);
}

// MARK: Limits

#[test]
fn limits_parsing() {
    let data = std::fs::read(fixtures().join("limits.json")).unwrap();
    let limits = PlanLimit::parse(&data);
    assert_eq!(limits.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["five_hour", "seven_day"]);
    assert!((limits[0].utilization - 0.42).abs() < 1e-9);
    assert_eq!(limits[0].resets_at, Some(ts("2026-11-25T08:00:00Z")));
}

/// Real response captured 2026-11-25 (no credentials in it): `limits` array, microsecond timestamps.
#[test]
fn live_limits_response() {
    let data = std::fs::read(fixtures().join("limits-live.json")).unwrap();
    let limits = PlanLimit::parse(&data);
    assert_eq!(limits.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["five_hour", "seven_day", "seven_day_fable"]);
    assert_eq!(limits.iter().map(|l| l.label.as_str()).collect::<Vec<_>>(), ["5-hour session", "Weekly · all models", "Weekly · Fable"]);
    assert_eq!(limits.iter().map(|l| l.short_label()).collect::<Vec<_>>(), ["5h", "Week", "Fable"]);
    assert!((limits[1].utilization - 0.15).abs() < 1e-9);
    let reset = limits[0].resets_at.unwrap();
    assert!((reset.duration_since(ts("2026-11-25T05:39:59Z")).as_secs_f64() - 0.95).abs() < 0.01);
    // Pace: 2% used, 5h window ends 05:39:59 → started 00:39:59; at 03:09:59 half the window is gone.
    let now = ts("2026-11-25T03:09:59.95238Z");
    assert!((limits[0].projected_at_reset(now).unwrap() - 0.04).abs() < 1e-6);
    assert_eq!(limits[0].effective_utilization(ts("2026-11-25T06:00:00Z")), 0.0);
    assert_eq!(limits[0].reset_text(now, &Cal::utc()), "resets in 2h 30m");
}

// MARK: Formatting

#[test]
fn session_titles() {
    assert_eq!(SessionStat::prettify("i-want-you-to-cached-sloth-of-46b41c9d-fd9b-47cc"), "I want you to cached sloth");
    assert_eq!(SessionStat::prettify("floating-bouncing-church"), "Floating bouncing church");
    assert_eq!(SessionStat::prettify("46b41c9d"), "Untitled session");
}

#[test]
fn dollars_are_us_style() {
    assert_eq!(CurrencyFormat::string(521.4, false), "$521");
    assert_eq!(CurrencyFormat::string(8.5, false), "$8.50");
    assert_eq!(CurrencyFormat::string(1234.0, false), "$1,234");
}

#[test]
fn imports_swift_v2_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log-cache.json");
    std::fs::write(&path, r#"{"version":2,"checkpoints":{"/x/a.jsonl":{"inode":7,"offset":99}},"records":{"m1":{"id":"m1","timestamp":1795429672.5,"model":"claude-opus-5-5","input":1,"output":2,"cacheWrite5m":3,"cacheWrite1h":4,"cacheRead":5,"webSearches":0,"fast":false,"cwd":"/x","sessionId":"s","entrypoint":"claude-desktop","isSubagent":false}}}"#).unwrap();
    let c = LogCache::import_swift_v2(&path).unwrap();
    let r = &c.records["m1"];
    assert_eq!((r.input, r.output, r.cache_write_5m, r.cache_write_1h, r.cache_read), (1, 2, 3, 4, 5));
    assert_eq!(r.timestamp.as_millisecond(), 1795429672500);
    assert_eq!(c.checkpoints["/x/a.jsonl"].offset, 99);
}
