//! Plan limit parsing and pace projections (`limits.rs`).

mod common;

use jiff::tz::{self, TimeZone};
use jiff::{SignedDuration, Timestamp};
use nacut_core::limits::label_for;
use nacut_core::{Cal, PlanLimit};

use common::ts;

/// Wednesday 2026-11-11 12:00 UTC.
fn now() -> Timestamp {
    ts("2026-11-11T12:00:00Z")
}

fn parse(json: &str) -> Vec<PlanLimit> {
    PlanLimit::parse(json.as_bytes())
}

fn ids(limits: &[PlanLimit]) -> Vec<&str> {
    limits.iter().map(|l| l.id.as_str()).collect()
}

fn limit(id: &str, utilization: f64, resets_at: Option<Timestamp>) -> PlanLimit {
    PlanLimit { id: id.into(), label: label_for(id), utilization, resets_at }
}

fn hours(h: i64) -> SignedDuration {
    SignedDuration::from_hours(h)
}

fn minutes(m: i64) -> SignedDuration {
    SignedDuration::from_mins(m)
}

// MARK: parse

#[test]
fn anything_but_a_json_object_has_no_limits() {
    for body in ["", "not json", "[]", r#"[{"five_hour":{"utilization":5}}]"#, "42", "\"text\"", "null", "{}"] {
        assert!(parse(body).is_empty(), "{body:?}");
    }
}

#[test]
fn top_level_limits_are_read_when_there_is_no_list() {
    let l = parse(r#"{"five_hour":{"utilization":42,"resets_at":"2026-11-11T15:00:00Z"},"seven_day":{"utilization":7.5,"resets_at":null}}"#);
    assert_eq!(ids(&l), ["five_hour", "seven_day"]);
    assert!((l[0].utilization - 0.42).abs() < 1e-12);
    assert_eq!(l[0].resets_at, Some(ts("2026-11-11T15:00:00Z")));
    assert_eq!(l[1].resets_at, None);
    assert_eq!(l[1].label, "Weekly · all models");
}

#[test]
fn an_empty_limits_list_falls_back_to_top_level_keys() {
    let l = parse(r#"{"limits":[],"five_hour":{"utilization":42}}"#);
    assert_eq!(ids(&l), ["five_hour"]);
}

#[test]
fn a_limits_list_with_no_usable_items_falls_back_to_top_level_keys() {
    let l = parse(r#"{"limits":[{"kind":"session"},{"percent":3},"junk"],"seven_day":{"utilization":15}}"#);
    assert_eq!(ids(&l), ["seven_day"]);
    assert!((l[0].utilization - 0.15).abs() < 1e-12);
}

#[test]
fn top_level_entries_without_a_numeric_utilization_are_skipped() {
    let l = parse(
        r#"{"five_hour":{"utilization":null},"extra_usage":{"is_enabled":false},"seven_day":5,"seven_day_opus":{"utilization":"abc"},"seven_day_sonnet":{"utilization":10}}"#,
    );
    assert_eq!(ids(&l), ["seven_day_sonnet"]);
    assert_eq!(l[0].label, "Weekly · Sonnet");
}

#[test]
fn utilization_may_be_a_numeric_string() {
    let l = parse(r#"{"five_hour":{"utilization":"42.5"}}"#);
    assert!((l[0].utilization - 0.425).abs() < 1e-12);
    let l = parse(r#"{"limits":[{"kind":"session","percent":"7"}]}"#);
    assert!((l[0].utilization - 0.07).abs() < 1e-12);
}

#[test]
fn utilization_is_clamped_between_zero_and_150_percent() {
    let l = parse(r#"{"five_hour":{"utilization":200},"seven_day":{"utilization":-5}}"#);
    assert_eq!(l[0].utilization, 1.5);
    assert_eq!(l[1].utilization, 0.0);
    let l = parse(r#"{"limits":[{"kind":"session","percent":300}]}"#);
    assert_eq!(l[0].utilization, 1.5);
}

#[test]
fn non_finite_utilization_strings_are_skipped() {
    let l = parse(r#"{"five_hour":{"utilization":"NaN"},"seven_day":{"utilization":"inf"},"seven_day_opus":{"utilization":"12"}}"#);
    assert_eq!(ids(&l), ["seven_day_opus"]);
    let l = parse(r#"{"limits":[{"kind":"session","percent":"NaN"},{"kind":"weekly_all","percent":"-inf"},{"kind":"weekly_scoped","percent":4,"scope":{"model":{"display_name":"Fable"}}}]}"#);
    assert_eq!(ids(&l), ["seven_day_fable"]);
}

#[test]
fn an_unparseable_reset_time_is_none() {
    let l = parse(r#"{"five_hour":{"utilization":10,"resets_at":"tomorrow"}}"#);
    assert_eq!(l[0].resets_at, None);
}

// MARK: parse: limits list

#[test]
fn list_maps_kinds_to_stable_ids_and_labels() {
    let l = parse(
        r#"{"limits":[
            {"kind":"session","percent":2,"resets_at":"2026-11-11T15:00:00Z"},
            {"kind":"weekly_all","percent":15},
            {"kind":"weekly_scoped","percent":30,"scope":{"model":{"display_name":"Fable"}}}
        ]}"#,
    );
    assert_eq!(ids(&l), ["five_hour", "seven_day", "seven_day_fable"]);
    assert_eq!(l.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), ["5-hour session", "Weekly · all models", "Weekly · Fable"]);
    assert_eq!(l[0].resets_at, Some(ts("2026-11-11T15:00:00Z")));
}

#[test]
fn list_items_missing_kind_or_percent_are_skipped() {
    let l = parse(r#"{"limits":[{"kind":"session"},{"percent":5},{"kind":7,"percent":5},{"kind":"weekly_all","percent":15}]}"#);
    assert_eq!(ids(&l), ["seven_day"]);
}

#[test]
fn list_keeps_the_first_of_duplicate_ids() {
    let l = parse(r#"{"limits":[{"kind":"session","percent":10},{"kind":"session","percent":90}]}"#);
    assert_eq!(l.len(), 1);
    assert!((l[0].utilization - 0.1).abs() < 1e-12);
}

#[test]
fn scoped_limit_with_a_multi_word_model_name_gets_a_snake_case_id() {
    let l = parse(r#"{"limits":[{"kind":"weekly_scoped","percent":3,"scope":{"model":{"display_name":"Opus 5"}}}]}"#);
    assert_eq!(ids(&l), ["seven_day_opus_5"]);
    assert_eq!(l[0].label, "Weekly · Opus 5");
    assert_eq!(l[0].short_label(), "Opus 5");
}

#[test]
fn scoped_limit_falls_back_to_the_surface_name() {
    let l = parse(r#"{"limits":[{"kind":"weekly_scoped","percent":3,"scope":{"surface":"Claude apps"}}]}"#);
    assert_eq!(ids(&l), ["seven_day_claude_apps"]);
    assert_eq!(l[0].label, "Weekly · Claude apps");
}

#[test]
fn scoped_limit_prefers_the_model_over_the_surface() {
    let l = parse(r#"{"limits":[{"kind":"weekly_scoped","percent":3,"scope":{"model":{"display_name":"Fable"},"surface":"apps"}}]}"#);
    assert_eq!(ids(&l), ["seven_day_fable"]);
}

#[test]
fn scoped_limit_without_a_scope_keeps_its_kind_as_id() {
    let l = parse(r#"{"limits":[{"kind":"weekly_scoped","percent":3}]}"#);
    assert_eq!(ids(&l), ["weekly_scoped"]);
    assert_eq!(l[0].label, "Weekly Scoped");
}

#[test]
fn unknown_kind_with_a_name_gets_a_snake_case_id() {
    let l = parse(r#"{"limits":[{"kind":"daily","percent":5,"scope":{"model":{"display_name":"Opus 5"}}}]}"#);
    assert_eq!(ids(&l), ["daily_opus_5"]);
    assert_eq!(l[0].label, "Daily Opus 5");
    assert_eq!(l[0].short_label(), "Daily Opus 5");
}

#[test]
fn unknown_kind_without_a_name_uses_the_kind() {
    let l = parse(r#"{"limits":[{"kind":"monthly_tokens","percent":5}]}"#);
    assert_eq!(ids(&l), ["monthly_tokens"]);
    assert_eq!(l[0].label, "Monthly Tokens");
}

#[test]
fn limits_sort_session_then_week_then_by_id() {
    let l = parse(
        r#"{"limits":[
            {"kind":"weekly_scoped","percent":1,"scope":{"model":{"display_name":"Sonnet"}}},
            {"kind":"weekly_scoped","percent":1,"scope":{"model":{"display_name":"Fable"}}},
            {"kind":"weekly_all","percent":1},
            {"kind":"daily","percent":1},
            {"kind":"session","percent":1}
        ]}"#,
    );
    assert_eq!(ids(&l), ["five_hour", "seven_day", "daily", "seven_day_fable", "seven_day_sonnet"]);
    let top = parse(r#"{"seven_day_sonnet":{"utilization":1},"seven_day":{"utilization":1},"a_limit":{"utilization":1},"five_hour":{"utilization":1}}"#);
    assert_eq!(ids(&top), ["five_hour", "seven_day", "a_limit", "seven_day_sonnet"]);
}

// MARK: labels

#[test]
fn known_ids_have_fixed_labels() {
    assert_eq!(label_for("five_hour"), "5-hour session");
    assert_eq!(label_for("seven_day"), "Weekly · all models");
    assert_eq!(label_for("seven_day_opus"), "Weekly · Opus");
    assert_eq!(label_for("seven_day_sonnet"), "Weekly · Sonnet");
    assert_eq!(label_for("seven_day_fable"), "Weekly · Fable");
    assert_eq!(label_for("seven_day_oauth_apps"), "Weekly · apps");
}

#[test]
fn other_ids_are_title_cased_words() {
    assert_eq!(label_for("seven_day_custom_thing"), "Seven Day Custom Thing");
    assert_eq!(label_for("__odd__key_"), "Odd Key");
    assert_eq!(label_for(""), "");
}

#[test]
fn short_labels() {
    assert_eq!(limit("five_hour", 0.1, None).short_label(), "5h");
    assert_eq!(limit("seven_day", 0.1, None).short_label(), "Week");
    assert_eq!(limit("seven_day_opus", 0.1, None).short_label(), "Opus");
    assert_eq!(limit("seven_day_custom", 0.1, None).short_label(), "Seven Day Custom");
}

// MARK: utilization and windows

#[test]
fn utilization_drops_to_zero_at_the_reset_instant() {
    let l = limit("five_hour", 0.6, Some(now()));
    assert_eq!(l.effective_utilization(now()), 0.0);
    assert_eq!(l.effective_utilization(now() - SignedDuration::from_millis(1)), 0.6);
}

#[test]
fn utilization_without_a_reset_time_never_drops() {
    assert_eq!(limit("five_hour", 0.6, None).effective_utilization(now()), 0.6);
}

#[test]
fn window_length_follows_the_id() {
    assert_eq!(limit("five_hour", 0.0, None).window(), hours(5));
    assert_eq!(limit("seven_day", 0.0, None).window(), hours(168));
    assert_eq!(limit("seven_day_fable", 0.0, None).window(), hours(168));
    assert_eq!(limit("daily", 0.0, None).window(), hours(168));
}

// MARK: projections

#[test]
fn projection_at_reset_needs_a_tenth_of_the_window_to_have_passed() {
    // The five-hour window started 30 minutes ago: exactly a tenth.
    let at_tenth = limit("five_hour", 0.05, Some(now() + minutes(270)));
    assert_eq!(at_tenth.projected_at_reset(now()), None);
    let after = limit("five_hour", 0.05, Some(now() + minutes(240)));
    assert!((after.projected_at_reset(now()).unwrap() - 0.25).abs() < 1e-12);
}

#[test]
fn projection_at_reset_is_none_once_the_window_is_over_or_unknown() {
    assert_eq!(limit("five_hour", 0.5, Some(now())).projected_at_reset(now()), None);
    assert_eq!(limit("five_hour", 0.5, Some(now() - hours(1))).projected_at_reset(now()), None);
    assert_eq!(limit("five_hour", 0.5, None).projected_at_reset(now()), None);
}

#[test]
fn projection_at_reset_scales_a_weekly_limit() {
    // 30% used two days into the week.
    let l = limit("seven_day", 0.3, Some(now() + hours(120)));
    assert!((l.projected_at_reset(now()).unwrap() - 1.05).abs() < 1e-12);
}

#[test]
fn projected_hit_needs_a_partly_used_limit() {
    let resets = Some(now() + hours(3));
    assert_eq!(limit("five_hour", 0.0, resets).projected_hit(now()), None);
    assert_eq!(limit("five_hour", 1.0, resets).projected_hit(now()), None);
    assert_eq!(limit("five_hour", 1.2, resets).projected_hit(now()), None);
    assert_eq!(limit("five_hour", 0.5, None).projected_hit(now()), None);
}

#[test]
fn projected_hit_waits_for_a_tenth_of_the_window() {
    assert_eq!(limit("five_hour", 0.09, Some(now() + minutes(270))).projected_hit(now()), None);
}

#[test]
fn projected_hit_is_none_when_the_reset_comes_first() {
    // 10% used in two hours: full in 18 more hours, long after the reset.
    assert_eq!(limit("five_hour", 0.1, Some(now() + hours(3))).projected_hit(now()), None);
}

#[test]
fn projected_hit_extrapolates_the_current_pace() {
    let five = limit("five_hour", 0.5, Some(now() + hours(3)));
    assert_eq!(five.projected_hit(now()), Some(now() + hours(2)));
    // 50% used two days into the week: full in two more days.
    let week = limit("seven_day", 0.5, Some(now() + hours(120)));
    assert_eq!(week.projected_hit(now()), Some(now() + hours(48)));
}

// MARK: reset text

#[test]
fn reset_text_is_empty_without_a_reset_time() {
    assert_eq!(limit("five_hour", 0.5, None).reset_text(now(), &Cal::utc()), "");
}

#[test]
fn reset_text_in_the_past_reads_zero_minutes() {
    assert_eq!(limit("five_hour", 0.5, Some(now() - hours(2))).reset_text(now(), &Cal::utc()), "resets in 0m");
}

#[test]
fn reset_text_counts_hours_and_minutes_under_a_day() {
    let text = |d: SignedDuration| limit("five_hour", 0.5, Some(now() + d)).reset_text(now(), &Cal::utc());
    assert_eq!(text(hours(1)), "resets in 1h 0m");
    assert_eq!(text(minutes(59) + SignedDuration::from_secs(59)), "resets in 59m");
    assert_eq!(text(minutes(275)), "resets in 4h 35m");
    assert_eq!(text(hours(24) - SignedDuration::from_secs(1)), "resets in 23h 59m");
}

#[test]
fn reset_text_a_day_or_more_away_names_the_weekday() {
    let l = limit("seven_day", 0.5, Some(now() + hours(24)));
    assert_eq!(l.reset_text(now(), &Cal::utc()), "resets Thu 12 PM");
    let l = limit("seven_day", 0.5, Some(ts("2026-11-15T19:00:00Z")));
    assert_eq!(l.reset_text(now(), &Cal::utc()), "resets Sun 7 PM");
    assert_eq!(l.reset_text(now(), &Cal::new(TimeZone::fixed(tz::offset(-5)))), "resets Sun 2 PM");
}
