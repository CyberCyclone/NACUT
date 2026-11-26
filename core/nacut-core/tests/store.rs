//! Calendar helpers and the in-memory record index (`store.rs`).

mod common;

use std::collections::HashMap;

use jiff::civil::date;
use jiff::tz::{self, TimeZone};
use jiff::SignedDuration;
use nacut_core::pricing::PriceBook;
use nacut_core::store::{add_days, add_months, days_between, Cal, Interval, ModelStat, SessionStat, UsageStore, TOKEN_CLASSES};
use nacut_core::UsageRecord;

use common::{record, ts};

const OPUS: &str = "claude-opus-5-5";
const SONNET: &str = "claude-sonnet-5-5";
const HAIKU: &str = "claude-haiku-5-5";

/// US Eastern as a POSIX rule: DST from the second Sunday of March to the first Sunday of November.
fn eastern() -> Cal {
    Cal::new(TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap())
}

/// Chile as a POSIX rule: clocks jump from 00:00 to 01:00 on the first Sunday of September,
/// so that day has no midnight.
fn chile() -> Cal {
    Cal::new(TimeZone::posix("<-04>4<-03>,M9.1.6/24,M4.1.6/24").unwrap())
}

fn fixed(hours: i8) -> Cal {
    Cal::new(TimeZone::fixed(tz::offset(hours)))
}

fn store_in(records: Vec<UsageRecord>, cal: Cal) -> UsageStore {
    UsageStore::new(records, PriceBook::bundled().clone(), cal)
}

fn store(records: Vec<UsageRecord>) -> UsageStore {
    store_in(records, Cal::utc())
}

/// $4: one million Opus 5.5 input tokens.
fn opus(id: &str, at: &str) -> UsageRecord {
    record(id, at, OPUS, 1_000_000, 0)
}

/// $2: one million Sonnet 5.5 input tokens.
fn sonnet(id: &str, at: &str) -> UsageRecord {
    record(id, at, SONNET, 1_000_000, 0)
}

fn with(mut r: UsageRecord, f: impl FnOnce(&mut UsageRecord)) -> UsageRecord {
    f(&mut r);
    r
}

fn all() -> Interval {
    Interval::new(ts("2026-01-01T00:00:00Z"), ts("2027-01-01T00:00:00Z"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// MARK: Cal

#[test]
fn day_of_uses_the_calendar_offset() {
    let west = fixed(-5);
    assert_eq!(west.day_of(ts("2026-11-20T04:59:59Z")), date(2026, 11, 19));
    assert_eq!(west.day_of(ts("2026-11-20T05:00:00Z")), date(2026, 11, 20));
    let east = fixed(10);
    assert_eq!(east.day_of(ts("2026-11-19T13:59:59Z")), date(2026, 11, 19));
    assert_eq!(east.day_of(ts("2026-11-19T14:00:00Z")), date(2026, 11, 20));
}

#[test]
fn start_of_a_day_without_midnight_is_the_first_valid_instant() {
    let cal = chile();
    assert_eq!(cal.start_of(date(2026, 9, 5)), ts("2026-09-05T04:00:00Z"));
    assert_eq!(cal.start_of(date(2026, 9, 6)), ts("2026-09-06T04:00:00Z"));
    assert_eq!(cal.day_of(ts("2026-09-06T03:59:59Z")), date(2026, 9, 5));
    assert_eq!(cal.day_of(ts("2026-09-06T04:00:00Z")), date(2026, 9, 6));
    let i = cal.day_interval(date(2026, 9, 6));
    assert_eq!(i.end.duration_since(i.start), SignedDuration::from_hours(23));
}

#[test]
fn dst_days_are_23_and_25_hours_long() {
    let cal = eastern();
    let spring = cal.day_interval(date(2026, 3, 8));
    assert_eq!((spring.start, spring.end), (ts("2026-03-08T05:00:00Z"), ts("2026-03-09T04:00:00Z")));
    let fall = cal.day_interval(date(2026, 11, 1));
    assert_eq!((fall.start, fall.end), (ts("2026-11-01T04:00:00Z"), ts("2026-11-02T05:00:00Z")));
    assert_eq!(fall.end.duration_since(fall.start), SignedDuration::from_hours(25));
}

#[test]
fn dates_interval_spans_whole_local_days() {
    let i = fixed(-5).dates_interval(date(2026, 11, 1), date(2026, 11, 3));
    assert_eq!((i.start, i.end), (ts("2026-11-01T05:00:00Z"), ts("2026-11-03T05:00:00Z")));
}

#[test]
fn interval_is_half_open() {
    let i = Interval::new(ts("2026-11-01T00:00:00Z"), ts("2026-11-02T00:00:00Z"));
    assert!(i.contains(ts("2026-11-01T00:00:00Z")));
    assert!(i.contains(ts("2026-11-01T23:59:59Z")));
    assert!(!i.contains(ts("2026-11-02T00:00:00Z")));
}

#[test]
fn weekday_rows_run_monday_to_sunday() {
    let cal = Cal::utc();
    assert_eq!(cal.weekday_row(ts("2026-11-23T12:00:00Z")), 0);
    assert_eq!(cal.weekday_row(ts("2026-11-26T12:00:00Z")), 3);
    assert_eq!(cal.weekday_row(ts("2026-11-29T12:00:00Z")), 6);
    // Monday 03:00 UTC is still Sunday evening at UTC−5.
    assert_eq!(fixed(-5).weekday_row(ts("2026-11-23T03:00:00Z")), 6);
    assert_eq!(fixed(-5).hour_of(ts("2026-11-23T03:00:00Z")), 22);
}

#[test]
fn adding_months_clamps_to_the_month_end() {
    assert_eq!(add_months(date(2026, 1, 31), 1), date(2026, 2, 28));
    assert_eq!(add_months(date(2028, 1, 31), 1), date(2028, 2, 29));
    assert_eq!(add_months(date(2026, 3, 31), -1), date(2026, 2, 28));
    assert_eq!(add_months(date(2026, 12, 15), 1), date(2027, 1, 15));
}

#[test]
fn adding_past_the_calendar_limits_leaves_the_date_unchanged() {
    assert_eq!(add_days(date(9999, 12, 31), 1), date(9999, 12, 31));
    assert_eq!(add_days(date(-9999, 1, 1), -1), date(-9999, 1, 1));
    assert_eq!(add_months(date(9999, 12, 1), 1), date(9999, 12, 1));
    assert_eq!(add_days(date(2026, 11, 1), i64::MAX), date(2026, 11, 1));
    assert_eq!(add_days(date(2026, 11, 1), i64::MIN), date(2026, 11, 1));
    assert_eq!(add_months(date(2026, 11, 1), i64::MAX), date(2026, 11, 1));
}

#[test]
fn days_between_counts_whole_days_either_way() {
    assert_eq!(days_between(date(2026, 11, 1), date(2026, 12, 1)), 30);
    assert_eq!(days_between(date(2026, 12, 1), date(2026, 11, 1)), -30);
    assert_eq!(days_between(date(2026, 11, 1), date(2026, 11, 1)), 0);
    assert_eq!(days_between(date(2026, 1, 1), date(2027, 1, 1)), 365);
}

// MARK: Store basics

#[test]
fn records_with_the_same_time_sort_by_id() {
    let s = store(vec![sonnet("b", "2026-11-20T10:00:00Z"), sonnet("c", "2026-11-20T09:00:00Z"), sonnet("a", "2026-11-20T10:00:00Z")]);
    assert_eq!(s.records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
    assert_eq!(s.first_date(), Some(ts("2026-11-20T09:00:00Z")));
    assert_eq!(s.last_date(), Some(ts("2026-11-20T10:00:00Z")));
}

#[test]
fn records_in_is_half_open() {
    let s = store(vec![sonnet("a", "2026-11-20T10:00:00Z"), sonnet("b", "2026-11-20T11:00:00Z"), sonnet("c", "2026-11-20T12:00:00Z")]);
    let ids = |a: &str, b: &str| s.records_in(Interval::new(ts(a), ts(b))).iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids("2026-11-20T10:00:00Z", "2026-11-20T12:00:00Z"), ["a", "b"]);
    assert_eq!(ids("2026-11-20T10:00:00.001Z", "2026-11-20T12:00:00.001Z"), ["b", "c"]);
    assert!(ids("2026-11-20T10:00:00Z", "2026-11-20T10:00:00Z").is_empty());
    assert!(ids("2026-11-20T12:00:00Z", "2026-11-20T10:00:00Z").is_empty());
    assert!(close(s.cost_in(Interval::new(ts("2026-11-20T10:00:00Z"), ts("2026-11-20T12:00:00Z"))), 4.0));
}

#[test]
fn an_empty_store_answers_every_query_with_nothing() {
    let s = store(vec![]);
    assert_eq!(s.first_date(), None);
    assert_eq!(s.last_date(), None);
    assert_eq!(s.busiest_hour(all()), None);
    assert!(s.model_stats(all()).is_empty());
    assert!(s.project_stats(all()).is_empty());
    assert!(s.sessions(all(), 10).is_empty());
    assert!(s.entrypoint_stats(all()).is_empty());
    assert!(s.hourly(date(2026, 11, 20)).is_empty());
    assert!(s.log_series.is_empty());
    assert_eq!(s.agent_split(all()), (0.0, 0.0));
    assert_eq!(s.cache_savings(all()), 0.0);
    assert_eq!(s.cost_in(all()), 0.0);
    let classes = s.token_classes(all());
    assert_eq!(classes.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), TOKEN_CLASSES);
    assert!(classes.iter().all(|c| c.value == 0.0 && c.tokens == 0));
    let heat = s.weekday_hour_heatmap(all());
    assert_eq!((heat.len(), heat[0].len()), (7, 24));
    assert!(heat.iter().flatten().all(|v| *v == 0.0));
}

#[test]
fn log_series_groups_by_local_day() {
    let records = vec![
        opus("a", "2026-11-20T03:00:00Z"),
        record("b", "2026-11-20T06:00:00Z", "claude-opus-5-5-20261001", 1_000_000, 500),
        sonnet("c", "2026-11-20T07:00:00Z"),
    ];
    let s = store_in(records, fixed(-5));
    assert_eq!(s.log_series.keys().copied().collect::<Vec<_>>(), [date(2026, 11, 19), date(2026, 11, 20)]);
    let day = &s.log_series[&date(2026, 11, 20)];
    assert!(close(day.cost, 4.01 + 2.0));
    assert_eq!(day.messages, 2);
    assert_eq!(day.tokens, 2_000_500);
    assert_eq!(day.by_model.len(), 2);
    assert!(close(day.by_model[OPUS], 4.01));
    assert!(close(day.by_model[SONNET], 2.0));
}

// MARK: Breakdowns

#[test]
fn busiest_hour_needs_some_cost() {
    let s = store(vec![record("free", "2026-11-20T10:00:00Z", "mystery-model", 1_000, 0)]);
    assert_eq!(s.busiest_hour(all()), None);
}

#[test]
fn busiest_hour_ties_go_to_the_earliest_hour() {
    let mut records = vec![sonnet("a", "2026-11-20T15:20:00Z"), sonnet("b", "2026-11-21T09:10:00Z")];
    assert_eq!(store(records.clone()).busiest_hour(all()), Some(9));
    records.push(sonnet("c", "2026-11-22T15:59:00Z"));
    assert_eq!(store(records).busiest_hour(all()), Some(15));
}

#[test]
fn busiest_hour_uses_local_hours() {
    let s = store_in(vec![sonnet("a", "2026-11-20T03:30:00Z")], fixed(-5));
    assert_eq!(s.busiest_hour(all()), Some(22));
}

#[test]
fn model_stats_tally_tokens_and_cost_per_model() {
    let records = vec![
        with(record("a", "2026-11-20T10:00:00Z", "claude-opus-5-5-20261001", 1_000_000, 100), |r| {
            r.cache_write_5m = 10;
            r.cache_write_1h = 20;
            r.cache_read = 30;
        }),
        with(opus("b", "2026-11-20T11:00:00Z"), |r| r.fast = true),
    ];
    let m = store(records).model_stats(all());
    assert_eq!(m.len(), 1);
    let o = &m[0];
    assert_eq!((o.id.as_str(), o.name.as_str()), (OPUS, "Opus 5.5"));
    assert_eq!(o.messages, 2);
    assert_eq!((o.input, o.output, o.cache_write, o.cache_read), (2_000_000, 100, 30, 30));
    assert_eq!(o.tokens(), 2_000_160);
    assert!(close(o.fast_cost, 8.0));
    assert!(close(o.cost, 4.0 + 0.002 + 0.00021 + 0.000006 + 8.0));
    assert!(!o.estimated);
    assert_eq!(o.share, 1.0);
}

#[test]
fn model_stats_sort_by_cost_then_id() {
    let records = vec![
        record("h", "2026-11-20T09:00:00Z", HAIKU, 0, 1_000_000),
        record("s", "2026-11-20T10:00:00Z", SONNET, 2_000_000, 0),
        opus("o", "2026-11-20T11:00:00Z"),
    ];
    let m = store(records).model_stats(all());
    assert_eq!(m.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), [OPUS, SONNET, HAIKU]);
    assert!(close(m[0].share, 4.0 / 8.5));
    assert!(close(m[2].share, 0.5 / 8.5));
}

#[test]
fn model_stats_flag_guessed_prices() {
    let m = store(vec![record("x", "2026-11-20T10:00:00Z", "claude-opus-9-9", 1_000_000, 0)]).model_stats(all());
    assert_eq!((m[0].id.as_str(), m[0].name.as_str()), ("claude-opus-9-9", "Opus 9.9"));
    assert!(m[0].estimated);
    assert!(close(m[0].cost, 4.0));
}

#[test]
fn model_stats_shares_are_zero_when_nothing_costs_anything() {
    let m = store(vec![record("x", "2026-11-20T10:00:00Z", "mystery-model", 1_000, 0)]).model_stats(all());
    assert_eq!(m.len(), 1);
    assert_eq!((m[0].cost, m[0].share), (0.0, 0.0));
    assert!(m[0].estimated);
}

#[test]
fn model_stat_per_message_cost_guards_against_no_messages() {
    let m = ModelStat {
        id: OPUS.into(),
        name: "Opus 5.5".into(),
        cost: 6.0,
        messages: 0,
        input: 1,
        output: 2,
        cache_write: 3,
        cache_read: 4,
        fast_cost: 0.0,
        estimated: false,
        share: 1.0,
    };
    assert_eq!(m.cost_per_message(), 0.0);
    assert_eq!(m.tokens(), 10);
    assert_eq!(ModelStat { messages: 4, ..m }.cost_per_message(), 1.5);
}

#[test]
fn project_stats_fold_worktrees_and_name_unknown_folders() {
    let records = vec![
        with(sonnet("a", "2026-11-20T10:00:00Z"), |r| {
            r.cwd = "/u/app".into();
            r.session_id = "s1".into();
        }),
        with(sonnet("b", "2026-11-20T11:00:00Z"), |r| {
            r.cwd = "/u/app/.claude/worktrees/feature".into();
            r.session_id = "s2".into();
        }),
        opus("c", "2026-11-20T09:00:00Z"),
        opus("d", "2026-11-20T12:00:00Z"),
    ];
    let p = store(records).project_stats(all());
    assert_eq!(p.len(), 2);
    assert_eq!((p[0].path.as_str(), p[0].name.as_str(), p[0].messages), ("", "Unknown", 2));
    assert!(close(p[0].cost, 8.0));
    assert!(close(p[0].share, 8.0 / 12.0));
    assert_eq!(p[0].last_active, ts("2026-11-20T12:00:00Z"));
    // Records without a session id count as one session here (the session list splits them per record).
    assert_eq!(p[0].sessions, 1);
    assert_eq!((p[1].path.as_str(), p[1].name.as_str(), p[1].messages, p[1].sessions), ("/u/app", "app", 2, 2));
    assert_eq!(p[1].last_active, ts("2026-11-20T11:00:00Z"));
}

#[test]
fn project_stats_with_equal_cost_sort_by_path() {
    let records = vec![
        with(sonnet("a", "2026-11-20T10:00:00Z"), |r| r.cwd = "/u/zeta".into()),
        with(sonnet("b", "2026-11-20T11:00:00Z"), |r| r.cwd = "/u/alpha".into()),
    ];
    let p = store(records).project_stats(all());
    assert_eq!(p.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["alpha", "zeta"]);
}

fn session_records() -> Vec<UsageRecord> {
    let in_s1 = |r: UsageRecord, slug: Option<&str>, entrypoint: &str| {
        with(r, |r| {
            r.session_id = "s1".into();
            r.cwd = "/u/app".into();
            r.slug = slug.map(String::from);
            r.entrypoint = entrypoint.into();
        })
    };
    let in_s2 = |r: UsageRecord, slug: &str| {
        with(r, |r| {
            r.session_id = "s2".into();
            r.slug = Some(slug.into());
        })
    };
    vec![
        in_s1(opus("r1", "2026-11-20T10:00:00Z"), None, "claude-desktop"),
        in_s1(sonnet("r2", "2026-11-20T10:30:00Z"), Some("fix-the-login-bug-of-46b41c9d"), "cli"),
        in_s1(opus("r3", "2026-11-20T11:00:00Z"), Some("other-title"), "cli"),
        record("lonely", "2026-11-20T09:00:00Z", HAIKU, 0, 1_000_000),
        in_s2(sonnet("r5", "2026-11-20T12:00:00Z"), "46b41c9d"),
        in_s2(sonnet("r6", "2026-11-20T12:05:00Z"), "real-title"),
    ]
}

#[test]
fn sessions_sort_by_cost() {
    let s = store(session_records()).sessions(all(), 10);
    assert_eq!(s.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["s1", "s2", "lonely"]);
}

#[test]
fn session_summary_comes_from_its_records() {
    let s = store(session_records()).sessions(all(), 10);
    let s1 = &s[0];
    assert_eq!(s1.title, "Fix the login bug");
    assert_eq!(s1.models, ["Opus 5.5", "Sonnet 5.5"]);
    assert_eq!(s1.entrypoint, "Desktop app");
    assert_eq!(s1.project, "app");
    assert_eq!((s1.start, s1.end), (ts("2026-11-20T10:00:00Z"), ts("2026-11-20T11:00:00Z")));
    assert_eq!(s1.duration_secs(), 3_600.0);
    assert_eq!(s1.messages, 3);
    assert!(close(s1.cost, 10.0));
}

#[test]
fn a_record_without_a_session_id_is_its_own_session() {
    let s = store(session_records()).sessions(all(), 10);
    let lonely = &s[2];
    assert_eq!(lonely.id, "lonely");
    assert_eq!(lonely.title, SessionStat::UNTITLED);
    assert_eq!(lonely.project, "Unknown");
    assert_eq!(lonely.entrypoint, "Terminal CLI");
    assert_eq!(lonely.duration_secs(), 0.0);
}

#[test]
fn an_untitled_slug_is_replaced_by_a_later_one() {
    let s = store(session_records()).sessions(all(), 10);
    assert_eq!(s[1].title, "Real title");
}

#[test]
fn sessions_are_limited() {
    let st = store(session_records());
    assert_eq!(st.sessions(all(), 2).iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["s1", "s2"]);
    assert!(st.sessions(all(), 0).is_empty());
}

#[test]
fn entrypoint_stats_group_by_display_name() {
    let records = vec![
        with(opus("a", "2026-11-20T10:00:00Z"), |r| r.entrypoint = "claude-desktop".into()),
        with(sonnet("b", "2026-11-20T10:01:00Z"), |r| r.entrypoint = "cli".into()),
        with(sonnet("c", "2026-11-20T10:02:00Z"), |r| r.entrypoint = String::new()),
        with(opus("d", "2026-11-20T10:03:00Z"), |r| r.entrypoint = "claude-vscode".into()),
        with(record("e", "2026-11-20T10:04:00Z", HAIKU, 0, 1_000_000), |r| r.entrypoint = "sdk-ts".into()),
    ];
    let e = store(records).entrypoint_stats(all());
    assert_eq!(e.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Desktop app", "Terminal CLI", "VS Code", "Agent SDK"]);
    assert!(close(e[1].value, 4.0));
    assert!(close(e[3].value, 0.5));
    assert!(e.iter().all(|x| x.tokens == 0));
}

#[test]
fn agent_split_separates_subagents() {
    let records = vec![with(opus("a", "2026-11-20T10:00:00Z"), |r| r.is_subagent = true), sonnet("b", "2026-11-20T11:00:00Z")];
    let (main, sub) = store(records).agent_split(all());
    assert!(close(main, 2.0));
    assert!(close(sub, 4.0));
}

#[test]
fn token_classes_apply_the_fast_multiplier_but_not_web_search() {
    let r = with(record("a", "2026-11-20T10:00:00Z", OPUS, 1_000_000, 1_000_000), |r| {
        r.cache_write_5m = 1_000_000;
        r.cache_write_1h = 1_000_000;
        r.cache_read = 1_000_000;
        r.web_searches = 5;
        r.fast = true;
    });
    let s = store(vec![r]);
    let c = s.token_classes(all());
    assert_eq!(c.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Input", "Output", "Cache write", "Cache read"]);
    assert_eq!(c.iter().map(|x| x.tokens).collect::<Vec<_>>(), [1_000_000, 1_000_000, 2_000_000, 1_000_000]);
    for (got, want) in c.iter().map(|x| x.value).zip([8.0, 40.0, 26.0, 0.4]) {
        assert!(close(got, want), "{got} != {want}");
    }
    assert!(close(s.records[0].cost, 74.4 + 0.05));
}

#[test]
fn cache_savings_compare_reads_with_the_input_price() {
    let read = |id: &str, fast: bool| {
        with(record(id, "2026-11-20T10:00:00Z", OPUS, 0, 0), |r| {
            r.cache_read = 1_000_000;
            r.fast = fast;
        })
    };
    assert!(close(store(vec![read("a", false)]).cache_savings(all()), 3.8));
    assert!(close(store(vec![read("a", false), read("b", true)]).cache_savings(all()), 3.8 + 7.6));
}

#[test]
fn heatmap_places_cost_by_local_weekday_and_hour() {
    let s = store_in(vec![opus("a", "2026-11-23T03:30:00Z"), sonnet("b", "2026-11-23T15:00:00Z")], fixed(-5));
    let grid = s.weekday_hour_heatmap(all());
    assert!(close(grid[6][22], 4.0));
    assert!(close(grid[0][10], 2.0));
    assert!(close(grid.iter().flatten().sum::<f64>(), 6.0));
}

#[test]
fn hourly_splits_one_day_by_hour_and_model() {
    let records = vec![
        opus("a", "2026-11-20T01:10:00Z"),
        sonnet("b", "2026-11-20T01:50:00Z"),
        opus("c", "2026-11-20T01:55:00Z"),
        sonnet("d", "2026-11-20T05:00:00Z"),
        opus("next-day", "2026-11-21T00:30:00Z"),
    ];
    let h = store(records).hourly(date(2026, 11, 20));
    let rows: Vec<(u32, &str)> = h.iter().map(|x| (x.hour, x.model.as_str())).collect();
    assert_eq!(rows, [(1, "Opus 5.5"), (1, "Sonnet 5.5"), (5, "Sonnet 5.5")]);
    assert!(close(h[0].cost, 8.0));
}

#[test]
fn hourly_follows_the_local_day() {
    let s = store_in(vec![opus("a", "2026-11-20T03:00:00Z"), opus("b", "2026-11-20T05:00:00Z")], fixed(-5));
    let h = s.hourly(date(2026, 11, 20));
    assert_eq!(h.iter().map(|x| x.hour).collect::<Vec<_>>(), [0]);
}

fn day_after_records() -> UsageStore {
    store(vec![
        opus("a", "2026-11-20T08:00:00Z"),
        sonnet("b", "2026-11-20T12:00:00Z"),
        record("c", "2026-11-20T23:59:00Z", "claude-opus-5-5-20261001", 1_000_000, 0),
        sonnet("next-day", "2026-11-21T00:00:00Z"),
    ])
}

#[test]
fn day_cost_after_counts_from_the_given_moment_to_the_end_of_the_day() {
    let d = day_after_records().day_cost_after(date(2026, 11, 20), ts("2026-11-20T12:00:00Z"));
    assert!(close(d.cost, 6.0));
    assert_eq!((d.messages, d.tokens), (2, 2_000_000));
    assert_eq!(d.by_model.len(), 2);
    assert!(close(d.by_model[OPUS], 4.0));
    assert!(close(d.by_model[SONNET], 2.0));
}

#[test]
fn day_cost_after_a_moment_before_the_day_counts_the_whole_day() {
    let d = day_after_records().day_cost_after(date(2026, 11, 20), ts("2026-11-19T00:00:00Z"));
    assert!(close(d.cost, 10.0));
    assert_eq!(d.messages, 3);
}

#[test]
fn day_cost_after_the_day_has_ended_is_empty() {
    let d = day_after_records().day_cost_after(date(2026, 11, 20), ts("2026-11-21T00:00:00Z"));
    assert_eq!((d.cost, d.messages), (0.0, 0));
    assert_eq!(d.by_model, HashMap::new());
}

// MARK: Session titles

#[test]
fn prettify_drops_one_trailing_stopword() {
    assert_eq!(SessionStat::prettify("fix-the-bug-of-the"), "Fix the bug of");
}

#[test]
fn prettify_keeps_hex_looking_words_without_digits() {
    assert_eq!(SessionStat::prettify("deadbeef-cafe"), "Deadbeef cafe");
    assert_eq!(SessionStat::prettify("add-face-check"), "Add face check");
}

#[test]
fn prettify_cuts_at_the_first_short_hash_with_a_digit() {
    assert_eq!(SessionStat::prettify("tidy-up-ab12-later"), "Tidy up");
    assert_eq!(SessionStat::prettify("tidy-up-a1b"), "Tidy up a1b");
}

#[test]
fn prettify_without_words_is_untitled() {
    assert_eq!(SessionStat::prettify(""), SessionStat::UNTITLED);
    assert_eq!(SessionStat::prettify("---"), SessionStat::UNTITLED);
    assert_eq!(SessionStat::prettify("the"), SessionStat::UNTITLED);
    assert_eq!(SessionStat::prettify("a1b2-word"), SessionStat::UNTITLED);
}

#[test]
fn prettify_capitalises_only_the_first_letter() {
    assert_eq!(SessionStat::prettify("fix--double-dash"), "Fix double dash");
    assert_eq!(SessionStat::prettify("éclair-recipe"), "Éclair recipe");
    assert_eq!(SessionStat::prettify("Already-Capital"), "Already Capital");
}

#[test]
fn untitled_session_has_a_fixed_name() {
    assert_eq!(SessionStat::UNTITLED, "Untitled session");
}
