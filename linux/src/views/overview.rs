//! Stat tiles, cumulative spend, the last 30 days by model, caching and insights (OverviewTab.swift).

use gtk::prelude::*;
use nacut_core::engine::StatRange;
use nacut_core::forecast::BudgetStatus;
use nacut_core::forecast::PointKind;

use crate::charts::{self, LinePoint, Stack};
use crate::model::{self, fmt, pct, usd, usd_compact, AppModel};
use crate::theme;
use crate::ui::{self, BoxAdd};

pub fn build(m: &AppModel) -> gtk::Widget {
    let snap = m.snap();
    let f = &snap.forecast;
    let i = &snap.insights;
    let today_key = model::key_of(model::today());
    let today_msgs = snap.series.iter().find(|d| d.date == today_key).map(|d| d.messages).unwrap_or(0);
    let status_tint = format!("status-{}", theme::status_class(f.status));
    let vs = i.vs_previous_cycle.map(|v| format!("{}{}", if v >= 0.0 { "+" } else { "" }, pct(v))).unwrap_or("—".into());

    let tiles = ui::columns(
        3,
        8,
        vec![
            ui::stat_tile("Today", &usd(f.today), Some(&if today_msgs > 0 { format!("{today_msgs} messages") } else { "No usage yet".into() }), "weather-clear-symbolic", "coral", None),
            ui::stat_tile("Daily average", &usd(f.daily_average), Some(&format!("7-day {}", usd(f.trailing7_average))), "document-open-recent-symbolic", "coral",
                Some("Cycle-to-date spend divided by days elapsed. The 7-day figure is the recent run rate.")),
            ui::stat_tile("Projected", &usd(f.projected), Some(&format!("by {}", fmt(f.cycle_end_ms - 86_400_000, "%b %-d"))), "go-up-symbolic", &status_tint,
                Some("Spend so far plus the blended daily rate × days remaining")),
            ui::stat_tile("Last 7 days", &usd(i.last7_days), Some(&format!("30 days {}", usd_compact(i.last30_days))), "x-office-calendar-symbolic", "coral", None),
            ui::stat_tile("vs last cycle", &vs, Some(&format!("Last cycle {}", usd_compact(i.previous_cycle_total))),
                if i.vs_previous_cycle.unwrap_or(0.0) >= 0.0 { "go-up-symbolic" } else { "go-down-symbolic" }, "coral",
                Some("This cycle so far compared with the same point in the previous cycle")),
            ui::stat_tile("All time", &usd_compact(i.all_time), snap.first_record_ms.map(|ms| format!("since {}", fmt(ms, "%b %Y"))).as_deref(), "accessories-calculator-symbolic", "coral", None),
        ],
    );

    let v = ui::vbox(12);
    v.add(&tiles);
    if let Some(through) = snap.api.billed_through_ms {
        let ratio = snap.api.calibration.ratio;
        let cal = if ratio != 1.0 { format!(" (×{ratio:.2} calibrated)") } else { String::new() };
        v.add(&ui::para(&format!("Billed data through {}. Later usage is a live estimate{cal}.", fmt(through, "%b %-d, %-I:%M %p")), &["dim-label", "caption"]));
    }
    drop(snap);
    v.add(&cumulative_card(m)).add(&daily_bars_card(m)).add(&cache_card(m)).add(&insights_row(m));
    v.upcast()
}

fn cumulative_card(m: &AppModel) -> gtk::Widget {
    let snap = m.snap();
    let f = snap.forecast.clone();
    let points: Vec<LinePoint> = f
        .points
        .iter()
        .map(|p| LinePoint { day_ms: p.start_ms, value: p.cumulative, series: match p.kind { PointKind::Actual => 0, PointKind::Projected => 1, PointKind::Pace => 2 } })
        .collect();
    let pts = f.points.clone();
    let tooltip = move |day_ms: i64| {
        let same = |k: PointKind| pts.iter().find(|p| p.kind == k && (p.start_ms - day_ms).abs() < 43_200_000);
        let mut s = fmt(day_ms, "%a %b %-d");
        if let Some(a) = same(PointKind::Actual) {
            s.push_str(&format!("\nSpent {}", usd(a.cumulative)));
        } else if let Some(p) = same(PointKind::Projected) {
            s.push_str(&format!("\nProjected {}", usd(p.cumulative)));
        }
        if let Some(p) = same(PointKind::Pace) {
            s.push_str(&format!("\nPace {}", usd(p.cumulative)));
        }
        s
    };
    let legend = ui::hbox(10);
    legend.add(&charts::legend_line("Actual", theme::coral(), false, 1.0)).add(&charts::legend_line("Projected", theme::coral(), true, 0.6));
    if f.budget > 0.0 {
        legend.add(&charts::legend_line("Pace", theme::ink(), true, 0.4));
    }
    let chart = charts::cumulative(points, f.cycle_start_ms, f.cycle_end_ms, f.budget, 160, tooltip);
    ui::card(Some("Cycle spend"), Some(legend.upcast_ref()), &[chart.upcast_ref()]).upcast()
}

fn daily_bars_card(m: &AppModel) -> gtk::Widget {
    let days = m.engine.last_days(30);
    let mut ids: Vec<String> = days.iter().flat_map(|d| d.by_model.keys().cloned()).collect();
    ids.sort_by(|a, b| (theme::slot_order(a), a.as_str()).cmp(&(theme::slot_order(b), b.as_str())));
    ids.dedup();
    let mut has_other = false;
    let stacks: Vec<Stack> = days
        .iter()
        .map(|d| {
            let mut parts: Vec<(String, f64, theme::Rgb)> =
                ids.iter().filter_map(|id| d.by_model.get(id).map(|c| (m.engine.display_name(id), *c, theme::for_model(id)))).collect();
            let rest = d.cost - d.by_model.values().sum::<f64>();
            if rest > 0.005 {
                parts.push(("Other".into(), rest, theme::other()));
                has_other = true;
            }
            Stack { day_ms: d.start_ms, parts }
        })
        .collect();
    let avg = days.iter().map(|d| d.cost).sum::<f64>() / 30.0;
    if stacks.iter().all(|s| s.parts.is_empty()) {
        return ui::card(Some("Last 30 days"), None, &[ui::empty("No usage in the last 30 days").upcast_ref()]).upcast();
    }
    let n = stacks.len();
    let labels: Vec<i64> = stacks.iter().map(|s| s.day_ms).collect();
    let chart = charts::stacked_bars(
        stacks,
        170,
        Some(avg),
        move |i| ((n - 1 - i) % 7 == 0).then(|| fmt(labels[i], "%b %-d")),
        |s| {
            let mut parts = s.parts.clone();
            parts.sort_by(|a, b| b.1.total_cmp(&a.1));
            let mut t = format!("{}\nTotal {}", fmt(s.day_ms, "%a %b %-d"), usd(parts.iter().map(|p| p.1).sum()));
            for p in parts {
                t.push_str(&format!("\n{} {}", p.0, usd(p.1)));
            }
            t
        },
        0.0,
    );
    let mut legend: Vec<(String, theme::Rgb)> = ids.iter().map(|id| (m.engine.display_name(id), theme::for_model(id))).collect();
    if has_other {
        legend.push(("Other".into(), theme::other()));
    }
    let trailing = ui::dim(&format!("avg {}/day", usd(avg)));
    ui::card(Some("Last 30 days"), Some(trailing.upcast_ref()), &[chart.upcast_ref(), charts::legend(legend).upcast_ref()]).upcast()
}

fn cache_card(m: &AppModel) -> gtk::Widget {
    let savings = m.engine.cache_savings(StatRange::Cycle);
    let classes = m.engine.token_classes(StatRange::Cycle);
    let total = classes.iter().map(|c| c.tokens as f64).sum::<f64>().max(1.0);
    let read = classes.iter().find(|c| c.name == "Cache read").map(|c| c.tokens as f64).unwrap_or(0.0) / total;
    let left = ui::vbox(2);
    let s = ui::figure(&usd(savings), 17);
    s.add_css_class(&format!("status-{}", theme::status_class(BudgetStatus::UnderBudget)));
    left.add(&s).add(&ui::dim("saved this cycle by cache reads"));
    let right = ui::vbox(2);
    let r = ui::figure(&pct(read), 14);
    r.set_halign(gtk::Align::End);
    let d = ui::dim("of tokens were cache hits");
    d.set_halign(gtk::Align::End);
    right.add(&r).add(&d);
    let c = ui::card(Some("Prompt caching"), None, &[ui::spread(&left, Some(right.upcast_ref())).upcast_ref()]);
    c.set_tooltip_text(Some("What cache-read tokens would have cost at the normal input price, minus what they actually cost"));
    c.upcast()
}

fn insights_row(m: &AppModel) -> gtk::Widget {
    let snap = m.snap();
    let i = &snap.insights;
    let messages: u64 = m.engine.model_stats(StatRange::Cycle).iter().map(|s| s.messages).sum();
    let peak = i.peak_day.as_ref().map(|d| model::date_of(d).strftime("%a %b %-d, %Y").to_string());
    ui::columns(
        2,
        8,
        vec![
            ui::stat_tile("Peak day", &if i.peak_day.is_none() { "—".into() } else { usd(i.peak_cost) }, peak.as_deref(), "starred-symbolic", "coral", None),
            ui::stat_tile("Active streak", &format!("{} day{}", i.active_streak, if i.active_streak == 1 { "" } else { "s" }),
                Some(&format!("{} active days this cycle", i.active_days_this_cycle)), "media-playlist-repeat-symbolic", "coral", None),
            ui::stat_tile("Busiest hour", &snap.busiest_hour_last30.map(|h| model::hour_label(h as u32)).unwrap_or("—".into()), Some("last 30 days"), "alarm-symbolic", "coral", None),
            ui::stat_tile("Messages", &format!("{messages}"), Some("this cycle"), "mail-unread-symbolic", "coral", None),
        ],
    )
    .upcast()
}
