//! View models and shared UI wording (`views.rs`), built directly from forecasts, limits and settings.

mod common;

use std::collections::HashMap;

use jiff::civil::date;
use jiff::tz::{self, TimeZone};
use jiff::Timestamp;
use nacut_core::forecast::{BudgetStatus, Forecast, PointKind};
use nacut_core::insights::Insights;
use nacut_core::settings::{AccountMode, SecondFigure, Settings, UsageDisplay};
use nacut_core::store::{Cal, CostSource, DailySeries, DayCost, ModelStat, ProjectStat, SessionStat};
use nacut_core::views::{
    chart_ticks, limits_verdict, menu_bar_text, model_slot, model_slot_order, percent, status_view, DayEntry, DayKey,
    ForecastView, InsightsView, Level, LimitView, ModelView, ProjectView, SessionView, StatusInputs, MODEL_SLOTS,
};
use nacut_core::{BillingCycle, PlanLimit};

use common::{series, ts};

/// Wednesday 2026-11-11 12:00 UTC.
const NOW: &str = "2026-11-11T12:00:00Z";

fn now() -> Timestamp {
    ts(NOW)
}

fn limit(id: &str, label: &str, utilization: f64, resets_at: Option<&str>) -> PlanLimit {
    PlanLimit { id: id.into(), label: label.into(), utilization, resets_at: resets_at.map(ts) }
}

/// 50% used two hours into a five-hour window: full at 14:00, two hours before the 15:00 reset.
fn five_on_pace_to_fill() -> PlanLimit {
    limit("five_hour", "5-hour session", 0.5, Some("2026-11-11T15:00:00Z"))
}

/// 10% used three days into the week; resets Sunday noon.
fn week_light() -> PlanLimit {
    limit("seven_day", "Weekly · all models", 0.1, Some("2026-11-15T12:00:00Z"))
}

/// $50 a day for Nov 1–10 and $25 so far today: $525 spent, $1,500 projected.
fn cycle_series() -> DailySeries {
    let mut pairs: Vec<_> = (1..=10).map(|d| (date(2026, 11, d), 50.0)).collect();
    pairs.push((date(2026, 11, 11), 25.0));
    series(&pairs)
}

fn forecast(budget: f64) -> Forecast {
    Forecast::new(&cycle_series(), budget, 1, now(), &Cal::utc())
}

fn subscription() -> Settings {
    Settings { account_mode: AccountMode::Subscription, limits_enabled: true, usage_display: UsageDisplay::Limits, ..Settings::default() }
}

fn inputs<'a>(settings: &'a Settings, forecast: &'a Forecast, limits: &'a [PlanLimit], cal: &'a Cal) -> StatusInputs<'a> {
    StatusInputs { settings, forecast, all_time: 12_345.6, limits, now: now(), cal }
}

fn minus_five() -> Cal {
    Cal::new(TimeZone::fixed(tz::offset(-5)))
}

// MARK: percent and levels

#[test]
fn percent_rounds_to_whole_numbers() {
    assert_eq!(percent(0.0), "0%");
    assert_eq!(percent(0.005), "1%");
    assert_eq!(percent(0.004), "0%");
    assert_eq!(percent(0.155), "16%");
    assert_eq!(percent(1.0), "100%");
    assert_eq!(percent(1.234), "123%");
}

#[test]
fn percent_of_a_tiny_negative_is_zero_without_a_sign() {
    assert_eq!(percent(-0.004), "0%");
    assert_eq!(percent(-0.0), "0%");
}

#[test]
fn percent_keeps_real_negatives() {
    assert_eq!(percent(-0.25), "-25%");
}

#[test]
fn percent_of_a_non_finite_value_is_zero() {
    assert_eq!(percent(f64::NAN), "0%");
    assert_eq!(percent(f64::INFINITY), "0%");
    assert_eq!(percent(f64::NEG_INFINITY), "0%");
}

#[test]
fn utilization_levels_switch_at_70_and_90_percent() {
    assert_eq!(Level::for_utilization(0.0), Level::Normal);
    assert_eq!(Level::for_utilization(0.699_999), Level::Normal);
    assert_eq!(Level::for_utilization(0.7), Level::Warning);
    assert_eq!(Level::for_utilization(0.899_999), Level::Warning);
    assert_eq!(Level::for_utilization(0.9), Level::Critical);
    assert_eq!(Level::for_utilization(1.5), Level::Critical);
    assert_eq!(Level::for_utilization(f64::NAN), Level::Normal);
}

#[test]
fn budget_levels_follow_status() {
    assert_eq!(Level::for_budget(BudgetStatus::NoBudget), Level::Normal);
    assert_eq!(Level::for_budget(BudgetStatus::UnderBudget), Level::Normal);
    assert_eq!(Level::for_budget(BudgetStatus::Tight), Level::Warning);
    assert_eq!(Level::for_budget(BudgetStatus::WillExceed), Level::Warning);
    assert_eq!(Level::for_budget(BudgetStatus::Exceeded), Level::Critical);
}

// MARK: DayKey and DayEntry

#[test]
fn day_key_round_trips_a_date() {
    let k = DayKey::from(date(2028, 2, 29));
    assert_eq!(k, DayKey { year: 2028, month: 2, day: 29 });
    assert_eq!(k.to_date(), Some(date(2028, 2, 29)));
}

#[test]
fn day_key_rejects_impossible_dates() {
    assert_eq!(DayKey { year: 2026, month: 13, day: 1 }.to_date(), None);
    assert_eq!(DayKey { year: 2026, month: 0, day: 1 }.to_date(), None);
    assert_eq!(DayKey { year: 2026, month: 2, day: 30 }.to_date(), None);
    assert_eq!(DayKey { year: 2027, month: 2, day: 29 }.to_date(), None);
    assert_eq!(DayKey { year: 2026, month: 1, day: 0 }.to_date(), None);
    assert_eq!(DayKey { year: 2026, month: 200, day: 1 }.to_date(), None);
}

#[test]
fn day_key_rejects_a_year_outside_the_calendar_instead_of_wrapping() {
    // 67_562 truncated to i16 is 2026.
    assert_eq!(DayKey { year: 67_562, month: 11, day: 20 }.to_date(), None);
    assert_eq!(DayKey { year: -70_000, month: 1, day: 1 }.to_date(), None);
}

#[test]
fn day_entry_copies_the_day_and_starts_at_local_midnight() {
    let d = DayCost {
        cost: 3.5,
        by_model: HashMap::from([("claude-opus-5-5".to_string(), 3.5)]),
        messages: 4,
        tokens: 1_000,
        source: CostSource::Estimated,
    };
    let e = DayEntry::new(date(2026, 11, 20), &d, &minus_five());
    assert_eq!(e.date, DayKey { year: 2026, month: 11, day: 20 });
    assert_eq!(e.start_ms, ts("2026-11-20T05:00:00Z").as_millisecond());
    assert_eq!(e.cost, 3.5);
    assert_eq!(e.messages, 4);
    assert_eq!(e.tokens, 1_000);
    assert_eq!(e.by_model, d.by_model);
    assert_eq!(e.source, CostSource::Estimated);
}

// MARK: LimitView

#[test]
fn limit_view_subtitle_shows_when_it_fills_up() {
    let v = LimitView::new(&five_on_pace_to_fill(), now(), &Cal::utc());
    assert_eq!(v.id, "five_hour");
    assert_eq!(v.title, "5-hour session");
    assert_eq!(v.short_label, "5h");
    assert_eq!(v.utilization, 0.5);
    assert_eq!(v.level, Level::Normal);
    assert_eq!(v.reset_text, "resets in 3h 0m");
    assert_eq!(v.resets_at_ms, Some(ts("2026-11-11T15:00:00Z").as_millisecond()));
    assert_eq!(v.projected_hit_ms, Some(ts("2026-11-11T14:00:00Z").as_millisecond()));
    assert!((v.projected_at_reset.unwrap() - 1.25).abs() < 1e-9);
    assert_eq!(v.subtitle, "full ~Wed 2 PM");
}

#[test]
fn limit_view_subtitle_uses_the_calendar_time_zone() {
    let v = LimitView::new(&five_on_pace_to_fill(), now(), &minus_five());
    assert_eq!(v.subtitle, "full ~Wed 9 AM");
}

#[test]
fn limit_view_subtitle_is_the_reset_text_without_a_projected_hit() {
    let v = LimitView::new(&week_light(), now(), &Cal::utc());
    assert_eq!(v.title, "Weekly");
    assert_eq!(v.short_label, "Week");
    assert_eq!(v.projected_hit_ms, None);
    assert_eq!(v.reset_text, "resets Sun 12 PM");
    assert_eq!(v.subtitle, "resets Sun 12 PM");
    assert!((v.projected_at_reset.unwrap() - 0.1 / (3.0 / 7.0)).abs() < 1e-9);
}

#[test]
fn limit_view_reads_zero_once_the_window_has_reset() {
    let l = limit("five_hour", "5-hour session", 0.95, Some("2026-11-11T11:00:00Z"));
    let v = LimitView::new(&l, now(), &Cal::utc());
    assert_eq!(v.utilization, 0.0);
    assert_eq!(v.level, Level::Normal);
    assert_eq!(v.projected_at_reset, None);
    assert_eq!(v.projected_hit_ms, None);
    assert_eq!(v.reset_text, "resets in 0m");
    assert_eq!(v.subtitle, "resets in 0m");
}

#[test]
fn limit_view_titles_a_scoped_limit_by_its_model() {
    let l = limit("seven_day_fable", "Weekly · Fable", 0.75, Some("2026-11-15T12:00:00Z"));
    let v = LimitView::new(&l, now(), &Cal::utc());
    assert_eq!(v.title, "Fable weekly");
    assert_eq!(v.short_label, "Fable");
    assert_eq!(v.label, "Weekly · Fable");
    assert_eq!(v.level, Level::Warning);
}

// MARK: limits_verdict

#[test]
fn verdict_reports_a_full_five_hour_limit() {
    let l = limit("five_hour", "5-hour session", 1.0, Some("2026-11-11T13:00:00Z"));
    let v = limits_verdict(&[l], now(), &Cal::utc());
    assert_eq!(v.status, BudgetStatus::Exceeded);
    assert_eq!(v.level, Level::Critical);
    assert_eq!(v.text, "5-hour limit reached · resets in 1h 0m");
}

#[test]
fn verdict_capitalises_the_weekly_limit() {
    let l = limit("seven_day", "Weekly · all models", 1.2, Some("2026-11-13T12:00:00Z"));
    assert_eq!(limits_verdict(&[l], now(), &Cal::utc()).text, "Weekly limit reached · resets Fri 12 PM");
}

#[test]
fn verdict_names_a_full_scoped_limit_by_its_model() {
    let l = limit("seven_day_fable", "Weekly · Fable", 1.0, Some("2026-11-11T14:30:00Z"));
    assert_eq!(limits_verdict(&[l], now(), &Cal::utc()).text, "Fable weekly limit reached · resets in 2h 30m");
}

#[test]
fn verdict_prefers_a_full_limit_over_a_projected_one() {
    let full = limit("seven_day", "Weekly · all models", 1.0, Some("2026-11-13T12:00:00Z"));
    let v = limits_verdict(&[five_on_pace_to_fill(), full], now(), &Cal::utc());
    assert_eq!(v.status, BudgetStatus::Exceeded);
}

#[test]
fn verdict_ignores_a_full_limit_whose_window_has_reset() {
    let l = limit("five_hour", "5-hour session", 1.0, Some("2026-11-11T11:00:00Z"));
    let v = limits_verdict(&[l], now(), &Cal::utc());
    assert_eq!(v.status, BudgetStatus::UnderBudget);
    assert_eq!(v.text, "Plenty of room in your plan limits");
}

#[test]
fn verdict_warns_about_the_limit_that_runs_out_first() {
    // 99% used six days into the week: full about 1h27m from now, before the five-hour limit at 14:00.
    let week = limit("seven_day", "Weekly · all models", 0.99, Some("2026-11-12T12:00:00Z"));
    let v = limits_verdict(&[five_on_pace_to_fill(), week], now(), &Cal::utc());
    assert_eq!(v.status, BudgetStatus::WillExceed);
    assert_eq!(v.level, Level::Warning);
    assert_eq!(v.text, "At this pace the weekly limit runs out Wed 1:27 PM");
}

#[test]
fn verdict_warns_about_the_five_hour_limit() {
    let v = limits_verdict(&[five_on_pace_to_fill(), week_light()], now(), &Cal::utc());
    assert_eq!(v.text, "At this pace the 5-hour limit runs out Wed 2:00 PM");
}

#[test]
fn verdict_is_tight_when_the_week_is_on_pace_for_80_percent() {
    // 40% used halfway through the week.
    let week = limit("seven_day", "Weekly · all models", 0.4, Some("2026-11-15T00:00:00Z"));
    let v = limits_verdict(&[week], now(), &Cal::utc());
    assert_eq!(v.status, BudgetStatus::Tight);
    assert_eq!(v.level, Level::Warning);
    assert_eq!(v.text, "On pace for ~80% of your weekly limit by reset");
}

#[test]
fn verdict_is_under_budget_below_80_percent_of_the_week() {
    let week = limit("seven_day", "Weekly · all models", 0.2, Some("2026-11-15T00:00:00Z"));
    let v = limits_verdict(&[week], now(), &Cal::utc());
    assert_eq!(v.status, BudgetStatus::UnderBudget);
    assert_eq!(v.level, Level::Normal);
    assert_eq!(v.text, "On pace for ~40% of your weekly limit by reset");
}

#[test]
fn verdict_has_plenty_of_room_without_a_weekly_projection() {
    let five = limit("five_hour", "5-hour session", 0.1, Some("2026-11-11T15:00:00Z"));
    assert_eq!(limits_verdict(&[five], now(), &Cal::utc()).text, "Plenty of room in your plan limits");
    assert_eq!(limits_verdict(&[], now(), &Cal::utc()).text, "Plenty of room in your plan limits");
    // Too early in the week to project.
    let fresh_week = limit("seven_day", "Weekly · all models", 0.05, Some("2026-11-18T06:00:00Z"));
    let v = limits_verdict(&[fresh_week], now(), &Cal::utc());
    assert_eq!((v.status, v.text.as_str()), (BudgetStatus::UnderBudget, "Plenty of room in your plan limits"));
}

// MARK: ForecastView

#[test]
fn forecast_view_labels_the_cycle_by_account_mode() {
    let f = forecast(0.0);
    let api = ForecastView::new(&f, &Settings::default(), &Cal::utc());
    assert_eq!(api.cycle_label, "Spend · Nov 1 – Nov 30");
    let sub = ForecastView::new(&f, &subscription(), &Cal::utc());
    assert_eq!(sub.cycle_label, "API value · Nov 1 – Nov 30");
}

#[test]
fn forecast_view_labels_a_mid_month_cycle_by_its_last_day() {
    let f = Forecast::new(&cycle_series(), 0.0, 15, now(), &Cal::utc());
    let v = ForecastView::new(&f, &Settings::default(), &Cal::utc());
    assert_eq!(v.cycle_label, "Spend · Oct 15 – Nov 14");
    assert_eq!(v.cycle_start, DayKey { year: 2026, month: 10, day: 15 });
    assert_eq!(v.cycle_end, DayKey { year: 2026, month: 11, day: 15 });
}

#[test]
fn forecast_view_copies_the_forecast() {
    let f = forecast(1000.0);
    let v = ForecastView::new(&f, &Settings::default(), &minus_five());
    assert_eq!(v.cycle_start_ms, ts("2026-11-01T05:00:00Z").as_millisecond());
    assert_eq!(v.cycle_end_ms, ts("2026-12-01T05:00:00Z").as_millisecond());
    assert_eq!((v.budget, v.spent, v.today), (1000.0, 525.0, 25.0));
    assert_eq!(v.status, BudgetStatus::WillExceed);
    assert_eq!(v.level, Level::Warning);
    assert_eq!(v.headline, f.headline(&minus_five()));
    assert_eq!(v.exhaustion_ms, Some(ts("2026-11-21T00:00:00Z").as_millisecond()));
    assert_eq!(v.budget_used_fraction, 0.525);
    assert!((v.projected_fraction - 1.5).abs() < 1e-9);
    assert_eq!(v.safe_daily_spend, f.safe_daily_spend);
}

#[test]
fn forecast_view_maps_each_point_to_its_local_midnight() {
    let f = forecast(1000.0);
    let v = ForecastView::new(&f, &Settings::default(), &minus_five());
    assert_eq!(v.points.len(), f.points.len());
    for (view, point) in v.points.iter().zip(&f.points) {
        assert_eq!(view.date, DayKey::from(point.date));
        assert_eq!(view.cumulative, point.cumulative);
        assert_eq!(view.kind, point.kind);
    }
    let first = &v.points[0];
    assert_eq!(first.date, DayKey { year: 2026, month: 11, day: 1 });
    assert_eq!(first.start_ms, ts("2026-11-01T05:00:00Z").as_millisecond());
    assert_eq!(first.kind, PointKind::Actual);
}

// MARK: Insights, models, projects, sessions

#[test]
fn insights_view_without_a_peak_day_has_zero_peak_cost() {
    let cal = Cal::utc();
    let cycle = BillingCycle::containing(now(), 1, &cal);
    let v = InsightsView::from(&Insights::new(&DailySeries::new(), &cycle, now(), &cal));
    assert_eq!(v.peak_day, None);
    assert_eq!(v.peak_cost, 0.0);
    assert_eq!(v.vs_previous_cycle, None);
}

#[test]
fn insights_view_copies_the_peak_day() {
    let cal = Cal::utc();
    let cycle = BillingCycle::containing(now(), 1, &cal);
    let s = series(&[(date(2026, 11, 3), 7.0), (date(2026, 11, 10), 2.0), (date(2026, 11, 11), 1.0)]);
    let v = InsightsView::from(&Insights::new(&s, &cycle, now(), &cal));
    assert_eq!(v.peak_day, Some(DayKey { year: 2026, month: 11, day: 3 }));
    assert_eq!(v.peak_cost, 7.0);
    assert_eq!(v.all_time, 10.0);
    assert_eq!(v.last7_days, 3.0);
    assert_eq!(v.active_streak, 2);
    assert_eq!(v.active_days_this_cycle, 3);
}

fn model_stat(cost: f64, fast_cost: f64, messages: u64) -> ModelStat {
    ModelStat {
        id: "claude-opus-5-5".into(),
        name: "Opus 5.5".into(),
        cost,
        messages,
        input: 1,
        output: 20,
        cache_write: 300,
        cache_read: 4_000,
        fast_cost,
        estimated: true,
        share: 0.25,
    }
}

#[test]
fn model_view_derives_tokens_share_and_per_message_cost() {
    let v = ModelView::from(&model_stat(10.0, 2.5, 4));
    assert_eq!(v.tokens, 4_321);
    assert_eq!(v.cost_per_message, 2.5);
    assert_eq!(v.fast_share, 0.25);
    assert_eq!((v.id.as_str(), v.name.as_str()), ("claude-opus-5-5", "Opus 5.5"));
    assert_eq!((v.input, v.output, v.cache_write, v.cache_read), (1, 20, 300, 4_000));
    assert!(v.estimated);
    assert_eq!(v.share, 0.25);
}

#[test]
fn model_view_with_no_cost_or_messages_has_zero_ratios() {
    let v = ModelView::from(&model_stat(0.0, 0.0, 0));
    assert_eq!(v.fast_share, 0.0);
    assert_eq!(v.cost_per_message, 0.0);
}

#[test]
fn project_view_converts_last_active_to_milliseconds() {
    let p = ProjectStat {
        path: "/u/app".into(),
        name: "app".into(),
        cost: 3.0,
        messages: 5,
        sessions: 2,
        last_active: ts("2026-11-10T08:30:00Z"),
        share: 0.6,
    };
    let v = ProjectView::from(&p);
    assert_eq!(v.last_active_ms, 1_794_299_400_000);
    assert_eq!((v.path.as_str(), v.name.as_str(), v.cost, v.messages, v.sessions, v.share), ("/u/app", "app", 3.0, 5, 2, 0.6));
}

#[test]
fn session_view_has_millisecond_bounds_and_duration() {
    let s = SessionStat {
        id: "s1".into(),
        title: "Fix tests".into(),
        project: "app".into(),
        start: ts("2026-11-10T08:00:00Z"),
        end: ts("2026-11-10T09:30:15Z"),
        cost: 1.5,
        messages: 3,
        models: vec!["Opus 5.5".into(), "Haiku 5.5".into()],
        entrypoint: "Terminal CLI".into(),
    };
    let v = SessionView::from(&s);
    assert_eq!(v.start_ms, ts("2026-11-10T08:00:00Z").as_millisecond());
    assert_eq!(v.end_ms, ts("2026-11-10T09:30:15Z").as_millisecond());
    assert_eq!(v.duration_secs, 5_415.0);
    assert_eq!(v.models, ["Opus 5.5", "Haiku 5.5"]);
    assert_eq!((v.id.as_str(), v.title.as_str(), v.project.as_str(), v.entrypoint.as_str()), ("s1", "Fix tests", "app", "Terminal CLI"));
}

// MARK: Menu bar text

#[test]
fn menu_bar_shows_five_hour_and_weekly_limits() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let limits = [five_on_pace_to_fill(), week_light()];
    assert_eq!(menu_bar_text(&inputs(&s, &f, &limits, &cal)), "5h 50% · Week 10%");
}

#[test]
fn compact_menu_bar_shows_only_the_first_limit() {
    let s = Settings { compact_menu_bar: true, ..subscription() };
    let (f, cal) = (forecast(0.0), Cal::utc());
    let limits = [five_on_pace_to_fill(), week_light()];
    assert_eq!(menu_bar_text(&inputs(&s, &f, &limits, &cal)), "5h 50%");
}

#[test]
fn menu_bar_without_a_five_hour_limit_leads_with_the_first_one() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let fable = limit("seven_day_fable", "Weekly · Fable", 0.3, Some("2026-11-15T12:00:00Z"));
    let limits = [fable, week_light()];
    assert_eq!(menu_bar_text(&inputs(&s, &f, &limits, &cal)), "Fable 30% · Week 10%");
}

#[test]
fn menu_bar_with_only_a_weekly_limit_shows_it_once() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let limits = [week_light()];
    assert_eq!(menu_bar_text(&inputs(&s, &f, &limits, &cal)), "Week 10%");
}

#[test]
fn menu_bar_shows_a_reset_limit_as_zero() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let limits = [limit("five_hour", "5-hour session", 0.8, Some("2026-11-11T11:00:00Z")), week_light()];
    assert_eq!(menu_bar_text(&inputs(&s, &f, &limits, &cal)), "5h 0% · Week 10%");
}

#[test]
fn menu_bar_falls_back_to_cost_when_there_are_no_limits_yet() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    assert_eq!(menu_bar_text(&inputs(&s, &f, &[], &cal)), "$25.00 · $525");
}

#[test]
fn menu_bar_shows_cost_when_the_subscription_prefers_it() {
    let s = Settings { usage_display: UsageDisplay::Cost, ..subscription() };
    let (f, cal) = (forecast(0.0), Cal::utc());
    let limits = [five_on_pace_to_fill()];
    assert_eq!(menu_bar_text(&inputs(&s, &f, &limits, &cal)), "$25.00 · $525");
}

#[test]
fn menu_bar_second_figure_follows_the_setting() {
    let (f, cal) = (forecast(0.0), Cal::utc());
    let text = |s: Settings| menu_bar_text(&inputs(&s, &f, &[], &cal));
    assert_eq!(text(Settings { second_figure: SecondFigure::Cycle, ..Settings::default() }), "$25.00 · $525");
    assert_eq!(text(Settings { second_figure: SecondFigure::AllTime, ..Settings::default() }), "$25.00 · $12.3k");
    assert_eq!(text(Settings { second_figure: SecondFigure::None, ..Settings::default() }), "$25.00");
    assert_eq!(text(Settings { compact_menu_bar: true, ..Settings::default() }), "$25.00");
}

// MARK: Status view

#[test]
fn status_with_limits_gauges_the_worst_limit() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let limits = [week_light(), five_on_pace_to_fill()];
    let v = status_view(&inputs(&s, &f, &limits, &cal), true, false, None, "local logs", None, None);
    assert!(v.shows_limits);
    assert_eq!(v.gauge_fraction, Some(0.5));
    assert_eq!(v.level, Level::Normal);
    assert_eq!(v.limits.len(), 2);
    assert_eq!(v.limits[1].id, "five_hour");
}

#[test]
fn status_level_turns_critical_near_a_limit() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let limits = [limit("five_hour", "5-hour session", 0.95, Some("2026-11-11T15:00:00Z")), week_light()];
    let v = status_view(&inputs(&s, &f, &limits, &cal), true, false, None, "local logs", None, None);
    assert_eq!(v.gauge_fraction, Some(0.95));
    assert_eq!(v.level, Level::Critical);
}

#[test]
fn status_tooltip_lists_each_limit() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let limits = [five_on_pace_to_fill(), week_light()];
    let v = status_view(&inputs(&s, &f, &limits, &cal), true, false, None, "local logs", None, None);
    assert_eq!(
        v.tooltip,
        "NACUT\n5-hour session: 50% (resets in 3h 0m)\nWeekly · all models: 10% (resets Sun 12 PM)\nToday $25.00 · this cycle $525 API value"
    );
}

#[test]
fn status_with_a_budget_gauges_the_budget_used() {
    let (s, f, cal) = (Settings::default(), forecast(2000.0), Cal::utc());
    let v = status_view(&inputs(&s, &f, &[], &cal), true, false, None, "local logs", None, None);
    assert!(!v.shows_limits);
    assert_eq!(v.gauge_fraction, Some(0.2625));
    assert_eq!(v.level, Level::Normal);
    assert_eq!(v.tooltip, "NACUT\nToday $25.00 · this cycle $525 spend\nOn track · projected $1,500 of $2,000");
    assert_eq!(v.menu_bar_text, "$25.00 · $525");
}

#[test]
fn status_level_follows_the_budget_forecast() {
    let (s, f, cal) = (Settings::default(), forecast(1000.0), Cal::utc());
    let v = status_view(&inputs(&s, &f, &[], &cal), true, false, None, "local logs", None, None);
    assert_eq!(v.gauge_fraction, Some(0.525));
    assert_eq!(v.level, Level::Warning);
}

#[test]
fn status_without_budget_or_limits_has_no_gauge() {
    let (s, f, cal) = (Settings::default(), forecast(0.0), Cal::utc());
    let v = status_view(&inputs(&s, &f, &[], &cal), true, false, None, "local logs", None, None);
    assert_eq!(v.gauge_fraction, None);
    assert_eq!(v.level, Level::Normal);
    assert_eq!(v.limits_verdict, None);
    assert_eq!(v.tooltip, "NACUT\nToday $25.00 · this cycle $525 spend\nProjected $1,500 this cycle");
}

#[test]
fn status_in_cost_mode_still_gauges_known_limits_without_a_budget() {
    let s = Settings { usage_display: UsageDisplay::Cost, ..subscription() };
    let (f, cal) = (forecast(0.0), Cal::utc());
    let limits = [limit("five_hour", "5-hour session", 0.95, Some("2026-11-11T15:00:00Z"))];
    let v = status_view(&inputs(&s, &f, &limits, &cal), true, false, None, "local logs", None, None);
    assert!(!v.shows_limits);
    assert_eq!(v.gauge_fraction, Some(0.95));
    // The level follows the (absent) budget, not the limit.
    assert_eq!(v.level, Level::Normal);
    assert_eq!(v.tooltip, "NACUT\nToday $25.00 · this cycle $525 API value\nProjected $1,500 this cycle");
    assert_eq!(v.limits_verdict.map(|x| x.status), Some(BudgetStatus::WillExceed));
}

#[test]
fn status_does_not_show_limits_until_some_arrive() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let v = status_view(&inputs(&s, &f, &[], &cal), false, true, None, "local logs", Some("Sign in".into()), None);
    assert!(!v.shows_limits);
    assert_eq!(v.gauge_fraction, None);
    assert_eq!(v.menu_bar_text, "$25.00 · $525");
    assert_eq!(v.limits_error.as_deref(), Some("Sign in"));
    assert!(!v.has_loaded);
    assert!(v.is_scanning);
}

#[test]
fn status_copies_totals_times_and_source() {
    let (s, f, cal) = (subscription(), forecast(0.0), Cal::utc());
    let limits = [five_on_pace_to_fill()];
    let v = status_view(
        &inputs(&s, &f, &limits, &cal),
        true,
        false,
        Some(ts("2026-11-11T11:59:00Z")),
        "hybrid",
        None,
        Some(ts("2026-11-11T11:58:30.5Z")),
    );
    assert_eq!((v.today, v.cycle_spent, v.all_time), (25.0, 525.0, 12_345.6));
    assert_eq!(v.last_updated_ms, Some(ts("2026-11-11T11:59:00Z").as_millisecond()));
    assert_eq!(v.limits_updated_ms, Some(1_794_398_310_500));
    assert_eq!(v.source_label, "hybrid");
    assert_eq!(v.limits_verdict.unwrap().text, "At this pace the 5-hour limit runs out Wed 2:00 PM");
}

// MARK: Model slots and chart ticks

#[test]
fn model_slots_are_unique_and_ordered() {
    let mut sorted = MODEL_SLOTS.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), MODEL_SLOTS.len());
    assert_eq!(model_slot("claude-opus-5-5"), Some(0));
    assert_eq!(model_slot("claude-opus-4-8"), Some(7));
    assert_eq!(model_slot_order("claude-sonnet-5-5"), 2);
}

#[test]
fn unknown_or_unnormalized_models_have_no_slot() {
    assert_eq!(model_slot("claude-opus-9-9"), None);
    assert_eq!(model_slot("claude-opus-5-5-20261001"), None);
    assert_eq!(model_slot(""), None);
    assert_eq!(model_slot_order("claude-opus-9-9"), 99);
}

#[test]
fn ticks_for_an_empty_or_invalid_maximum_are_zero_and_one() {
    for (max, count) in [(0.0, 4), (-5.0, 4), (f64::NAN, 4), (f64::INFINITY, 4), (f64::NEG_INFINITY, 4), (10.0, 0)] {
        assert_eq!(chart_ticks(max, count), [0.0, 1.0], "max {max} count {count}");
    }
}

#[test]
fn ticks_use_nice_steps() {
    assert_eq!(chart_ticks(100.0, 4), [0.0, 25.0, 50.0, 75.0, 100.0]);
    assert_eq!(chart_ticks(1.0, 4), [0.0, 0.25, 0.5, 0.75, 1.0]);
    assert_eq!(chart_ticks(7.0, 4), [0.0, 2.0, 4.0, 6.0, 8.0]);
    assert_eq!(chart_ticks(12.0, 4), [0.0, 5.0, 10.0, 15.0]);
    assert_eq!(chart_ticks(950.0, 5), [0.0, 200.0, 400.0, 600.0, 800.0, 1000.0]);
    assert_eq!(chart_ticks(90.0, 1), [0.0, 100.0]);
}

#[test]
fn ticks_are_exact_decimals() {
    assert_eq!(chart_ticks(0.3, 3), [0.0, 0.1, 0.2, 0.3]);
    assert_eq!(chart_ticks(0.7, 7), [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7]);
    assert_eq!(chart_ticks(0.06, 3), [0.0, 0.02, 0.04, 0.06]);
}

#[test]
fn ticks_reach_past_a_maximum_just_above_a_step() {
    assert_eq!(chart_ticks(1.0005, 4), [0.0, 0.5, 1.0, 1.5]);
    assert_eq!(chart_ticks(100.01, 4), [0.0, 50.0, 100.0, 150.0]);
}

#[test]
fn ticks_handle_tiny_and_huge_maxima() {
    assert_eq!(chart_ticks(1e-9, 4), [0.0, 2.5e-10, 5e-10, 7.5e-10, 1e-9]);
    assert_eq!(chart_ticks(1e15, 4), [0.0, 2.5e14, 5e14, 7.5e14, 1e15]);
    assert_eq!(chart_ticks(5e-324, 4), [0.0, 5e-324]);
    assert_eq!(chart_ticks(f64::MAX, 1), [0.0, f64::MAX]);
}

#[test]
fn ticks_always_cover_the_maximum_with_even_steps() {
    for max in [0.013, 0.37, 1.0, 3.3, 9.99, 10.0, 47.5, 123.0, 999.0, 1001.0, 52_345.0] {
        for count in 1..=8 {
            let t = chart_ticks(max, count);
            assert_eq!(t[0], 0.0);
            assert!(*t.last().unwrap() >= max, "max {max} count {count}: {t:?}");
            assert!(t.len() <= count as usize + 2, "max {max} count {count}: {t:?}");
            let step = t[1];
            for (i, v) in t.iter().enumerate() {
                assert!((v - step * i as f64).abs() <= step * 1e-9, "max {max} count {count}: {t:?}");
            }
        }
    }
}
