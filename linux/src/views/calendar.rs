//! Month heatmap with swipe paging (AdwCarousel), a month summary and per-day detail (CalendarTab.swift).

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use jiff::civil::Date;
use nacut_core::store::CostSource;
use nacut_core::views::DayEntry;
use nacut_core::CurrencyFormat;

use crate::charts::{self, Stack};
use crate::model::{self, fmt, usd, usd_exact, AppModel};
use crate::theme;
use crate::ui::{self, BoxAdd};

thread_local! {
    // Survive the panel rebuilding on every data change.
    static VISIBLE_MONTH: Cell<Option<Date>> = const { Cell::new(None) };
    static SELECTED_DAY: Cell<Option<Date>> = const { Cell::new(None) };
}

pub fn build(m: &AppModel, rebuild: Rc<dyn Fn()>) -> gtk::Widget {
    let snap = m.snap();
    let series: Vec<(Date, DayEntry)> = snap.series.iter().map(|d| (model::date_of(&d.date), d.clone())).collect();
    drop(snap);
    let today = model::today();
    let months = months_until(series.iter().map(|(d, _)| *d).min(), today);
    let current = VISIBLE_MONTH.get().filter(|v| months.contains(v)).unwrap_or(*months.last().unwrap());
    let scale_max = scale_max(series.iter().map(|(_, d)| d.cost));

    let title = ui::label(&current.strftime("%B %Y").to_string(), &["title-3"]);
    let today_btn = gtk::Button::with_label("Today");
    today_btn.set_sensitive(current != *months.last().unwrap());

    let carousel = adw::Carousel::builder().allow_scroll_wheel(true).allow_long_swipes(false).spacing(16).build();
    carousel.set_height_request(290);
    for mo in &months {
        carousel.append(&month_grid(*mo, scale_max, &series, rebuild.clone()));
    }
    let summary_host = adw::Bin::new();
    summary_host.set_child(Some(&month_summary(&series, current)));
    {
        let (months, title, today_btn, summary_host, series) = (months.clone(), title.clone(), today_btn.clone(), summary_host.clone(), series.clone());
        carousel.connect_page_changed(move |_, i| {
            let mo = months[i as usize];
            VISIBLE_MONTH.set(Some(mo));
            title.set_label(&mo.strftime("%B %Y").to_string());
            today_btn.set_sensitive(i as usize != months.len() - 1);
            summary_host.set_child(Some(&month_summary(&series, mo)));
        });
    }
    {
        let carousel = carousel.clone();
        let last = months.len() as u32 - 1;
        today_btn.connect_clicked(move |_| carousel.scroll_to(&carousel.nth_page(last), true));
    }
    // ← → page through months.
    let keys = gtk::EventControllerKey::new();
    {
        let carousel = carousel.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            let pos = carousel.position().round() as i32;
            let n = carousel.n_pages() as i32;
            let to = match key {
                gtk::gdk::Key::Left => pos - 1,
                gtk::gdk::Key::Right => pos + 1,
                _ => return gtk::glib::Propagation::Proceed,
            };
            if (0..n).contains(&to) {
                carousel.scroll_to(&carousel.nth_page(to as u32), true);
            }
            gtk::glib::Propagation::Stop
        });
    }
    carousel.add_controller(keys);
    carousel.set_focusable(true);
    let index = months.iter().position(|m| *m == current).unwrap_or(0) as u32;
    {
        let carousel = carousel.clone();
        gtk::glib::idle_add_local_once(move || carousel.scroll_to(&carousel.nth_page(index), false));
    }

    let v = ui::vbox(12);
    v.add(&ui::spread(&title, Some(today_btn.upcast_ref()))).add(&carousel).add(&summary_host);
    match SELECTED_DAY.get() {
        Some(day) => {
            let rb = rebuild.clone();
            v.add(&day_detail(m, day, Rc::new(move || {
                SELECTED_DAY.set(None);
                rb();
            })));
        }
        None => {
            v.add(&ui::para("Swipe or use ← → to change month. Click a day for details.", &["dim-label", "caption"]));
        }
    }
    v.upcast()
}

fn month_grid(month: Date, scale_max: f64, series: &[(Date, DayEntry)], rebuild: Rc<dyn Fn()>) -> gtk::Widget {
    let g = gtk::Grid::builder().column_spacing(4).row_spacing(4).column_homogeneous(true).hexpand(true).valign(gtk::Align::Start).build();
    // Monday-first weeks (ISO); GTK has no locale first-weekday API.
    for (c, name) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        let l = ui::label(name, &["dim-label", "caption-heading"]);
        l.set_xalign(0.5);
        g.attach(&l, c as i32, 0, 1, 1);
    }
    let today = model::today();
    let selected = SELECTED_DAY.get();
    for d in 0..month.days_in_month() as i32 {
        let day = month.checked_add(jiff::Span::new().days(d as i64)).unwrap();
        let entry = series.iter().find(|(k, _)| *k == day).map(|(_, e)| e);
        let cell = day_cell(day, entry, scale_max, selected == Some(day), day == today, day > today);
        let click = gtk::GestureClick::new();
        let rb = rebuild.clone();
        click.connect_released(move |_, _, _, _| {
            SELECTED_DAY.set(if SELECTED_DAY.get() == Some(day) { None } else { Some(day) });
            VISIBLE_MONTH.set(Some(month));
            rb();
        });
        cell.add_controller(click);
        let (column, row) = grid_position(day);
        g.attach(&cell, column, row, 1, 1);
    }
    g.upcast()
}

/// Every month from the one holding the first day with data (or today's) through today's.
fn months_until(first_day: Option<Date>, today: Date) -> Vec<Date> {
    let this_month = today.first_of_month();
    let mut months = vec![];
    let mut mo = first_day.unwrap_or(this_month).first_of_month().min(this_month);
    while mo <= this_month {
        months.push(mo);
        mo = mo.checked_add(jiff::Span::new().months(1)).unwrap();
    }
    months
}

/// Color scale shared by all months so they're comparable: the 95th percentile active day (at least 1¢), 1 with no data.
fn scale_max(costs: impl Iterator<Item = f64>) -> f64 {
    let mut costs: Vec<f64> = costs.filter(|c| *c > 0.0).collect();
    costs.sort_by(f64::total_cmp);
    if costs.is_empty() {
        return 1.0;
    }
    costs[((costs.len() as f64 * 0.95) as usize).min(costs.len() - 1)].max(0.01)
}

/// (column, row) of a day in its month's Monday-first grid; row 0 holds the weekday names.
fn grid_position(day: Date) -> (i32, i32) {
    let slot = day.first_of_month().weekday().to_monday_zero_offset() as i32 + day.day() as i32 - 1;
    (slot % 7, 1 + slot / 7)
}

/// CSS class for a day's fill: a ramp step relative to `scale_max`, or `empty` without spend.
fn fill_class(value: f64, scale_max: f64) -> String {
    match theme::ramp_index((value / scale_max).min(1.0)) {
        Some(i) if value > 0.0 => format!("ramp-{i}"),
        _ => "empty".into(),
    }
}

fn day_cell(day: Date, entry: Option<&DayEntry>, scale_max: f64, selected: bool, is_today: bool, future: bool) -> gtk::Box {
    let value = entry.map(|e| e.cost).unwrap_or(0.0);
    let b = ui::vbox(0);
    b.add_css_class("day-cell");
    b.set_size_request(-1, 40);
    b.add_css_class(&fill_class(value, scale_max));
    if selected {
        b.add_css_class("selected");
    } else if is_today {
        b.add_css_class("today");
    }
    let top = ui::hbox(2);
    let n = ui::label(&day.day().to_string(), &["caption"]);
    if is_today {
        n.add_css_class("heading");
    }
    n.set_opacity(if future { 0.45 } else { 0.8 });
    top.add(&n);
    if entry.map(|e| e.source == CostSource::Estimated).unwrap_or(false) {
        let d = ui::dot(theme::ink(), 3);
        d.set_tooltip_text(Some("Live estimate (not yet billed)"));
        top.add(&d);
    }
    b.add(&top);
    if value > 0.0 {
        let a = ui::label(&CurrencyFormat::axis(value), &["caption-heading", "numeric"]);
        a.set_vexpand(true);
        a.set_valign(gtk::Align::End);
        b.add(&a);
    }
    let tip = format!("{}: {}{}", day.strftime("%A, %B %-d"), usd_exact(value), entry.map(|e| format!(" · {} messages", e.messages)).unwrap_or_default());
    b.set_tooltip_text(Some(&tip));
    b
}

/// The costliest day; a tie goes to the earliest day, as on the Mac and Windows.
fn busiest<'a>(days: &[&'a (Date, DayEntry)]) -> Option<&'a (Date, DayEntry)> {
    days.iter().copied().max_by(|a, b| a.1.cost.total_cmp(&b.1.cost).then(b.0.cmp(&a.0)))
}

fn month_summary(series: &[(Date, DayEntry)], month: Date) -> gtk::Widget {
    let days: Vec<&(Date, DayEntry)> = series.iter().filter(|(d, e)| d.year() == month.year() && d.month() == month.month() && e.cost > 0.0).collect();
    let total: f64 = days.iter().map(|(_, e)| e.cost).sum();
    let busiest = busiest(&days);
    let item = |title: &str, value: &str| -> gtk::Widget {
        let v = ui::vbox(2);
        v.add(&ui::dim(title)).add(&ui::label(value, &["heading", "numeric"]));
        ui::subtle(&v, 8).upcast()
    };
    ui::columns(
        4,
        8,
        vec![
            item("Month total", &usd(total)),
            item("Avg / active day", &if days.is_empty() { "—".into() } else { usd(total / days.len() as f64) }),
            item("Active days", &days.len().to_string()),
            item("Busiest", &busiest.map(|(d, e)| format!("{} · {}", d.day(), CurrencyFormat::axis(e.cost))).unwrap_or("—".into())),
        ],
    )
    .upcast()
}

fn day_detail(m: &AppModel, day: Date, close: Rc<dyn Fn()>) -> gtk::Widget {
    let key = model::key_of(day);
    let models = m.engine.day_models(key.clone());
    let hourly = m.engine.hourly(key.clone());
    let sessions = m.engine.day_sessions(key.clone(), 5);
    let total = m.snap().series.iter().find(|d| d.date == key).map(|d| d.cost).unwrap_or(0.0);
    let trailing = ui::hbox(4);
    trailing.add(&ui::label(&usd(total), &["heading", "numeric"])).add(&ui::icon_button("window-close-symbolic", "Close", move || close()));
    let title = day.strftime("%A, %B %-d").to_string();
    if models.is_empty() {
        let text = if total > 0.0 { "Billed via API · no local detail for this day" } else { "No usage this day" };
        return ui::card(Some(&title), Some(trailing.upcast_ref()), &[ui::empty(text).upcast_ref()]).upcast();
    }
    let color_of = |name: &str| models.iter().find(|x| x.name == name).map(|x| theme::for_model(&x.id)).unwrap_or(theme::other());
    let midnight = day.to_zoned(jiff::tz::TimeZone::system()).map(|z| z.timestamp().as_millisecond()).unwrap_or(0);
    let stacks: Vec<Stack> = (0..24)
        .map(|h| Stack { day_ms: midnight + h as i64 * 3_600_000, parts: hourly.iter().filter(|x| x.hour == h).map(|x| (x.model.clone(), x.cost, color_of(&x.model))).collect() })
        .collect();
    let chart = charts::stacked_bars(
        stacks,
        110,
        None,
        |i| (i % 6 == 0).then(|| model::hour_label(i as u32)),
        |s| {
            let mut t = fmt(s.day_ms, "%-I %p");
            for p in &s.parts {
                t.push_str(&format!("\n{} {}", p.0, usd(p.1)));
            }
            t
        },
        9.0,
    );
    let v = ui::vbox(8);
    v.add(&chart);
    for x in &models {
        let left = ui::hbox(6);
        left.add(&ui::dot(theme::for_model(&x.id), 8)).add(&ui::label(&x.name, &[]));
        let right = ui::hbox(10);
        let cost = ui::label(&usd(x.cost), &["numeric"]);
        cost.set_width_chars(8);
        cost.set_xalign(1.0);
        right.add(&ui::dim(&format!("{} msgs", x.messages))).add(&cost);
        v.add(&ui::spread(&left, Some(right.upcast_ref())));
    }
    if !sessions.is_empty() {
        v.add(&ui::separator()).add(&ui::label("Sessions", &["dim-label", "caption-heading"]));
        for s in &sessions {
            v.add(&super::projects::session_row(s));
        }
    }
    ui::card(Some(&title), Some(trailing.upcast_ref()), &[v.upcast_ref()]).upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::{date, Weekday};

    #[test]
    fn grid_starts_on_monday() {
        // November 2026 starts on a Sunday and needs six week rows.
        assert_eq!(grid_position(date(2026, 11, 1)), (6, 1));
        assert_eq!(grid_position(date(2026, 11, 2)), (0, 2));
        assert_eq!(grid_position(date(2026, 11, 26)), (3, 5));
        assert_eq!(grid_position(date(2026, 11, 30)), (0, 6));
        // February 2021 starts on a Monday and fits in four.
        assert_eq!(grid_position(date(2021, 2, 1)), (0, 1));
        assert_eq!(grid_position(date(2021, 2, 28)), (6, 4));
    }

    #[test]
    fn grid_columns_match_weekdays_and_cells_are_unique() {
        let mut month = date(2023, 1, 1);
        while month < date(2028, 1, 1) {
            let mut seen = std::collections::HashSet::new();
            for d in 0..month.days_in_month() {
                let day = month.checked_add(jiff::Span::new().days(d as i64)).unwrap();
                let (column, row) = grid_position(day);
                assert_eq!(column, day.weekday().to_monday_zero_offset() as i32, "{day}");
                assert!((1..=6).contains(&row), "{day}");
                assert!(seen.insert((column, row)), "{day}");
            }
            assert_eq!(grid_position(month).1, 1);
            month = month.checked_add(jiff::Span::new().months(1)).unwrap();
        }
        assert_eq!(date(2026, 11, 2).weekday(), Weekday::Monday);
    }

    #[test]
    fn months_from_first_data_to_today() {
        let today = date(2026, 11, 26);
        assert_eq!(months_until(None, today), [date(2026, 11, 1)]);
        assert_eq!(months_until(Some(date(2026, 11, 3)), today), [date(2026, 11, 1)]);
        assert_eq!(months_until(Some(date(2026, 9, 30)), today), [date(2026, 9, 1), date(2026, 10, 1), date(2026, 11, 1)]);
        assert_eq!(months_until(Some(date(2025, 12, 31)), date(2026, 1, 1)), [date(2025, 12, 1), date(2026, 1, 1)]);
        // Data dated after today (clock changes) still shows this month only.
        assert_eq!(months_until(Some(date(2027, 2, 1)), today), [date(2026, 11, 1)]);
        assert_eq!(months_until(Some(date(2024, 11, 26)), today).len(), 25);
    }

    #[test]
    fn scale_is_the_95th_percentile() {
        assert_eq!(scale_max(std::iter::empty()), 1.0);
        assert_eq!(scale_max([0.0, -3.0, f64::NAN].into_iter()), 1.0);
        assert_eq!(scale_max([4.0].into_iter()), 4.0);
        assert_eq!(scale_max([0.001].into_iter()), 0.01);
        let costs: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert_eq!(scale_max(costs.iter().rev().copied()), 96.0);
        assert_eq!(scale_max([1.0, 2.0, 3.0, 1000.0].into_iter()), 1000.0);
    }

    #[test]
    fn fill_classes() {
        assert_eq!(fill_class(0.0, 10.0), "empty");
        assert_eq!(fill_class(-1.0, 10.0), "empty");
        assert_eq!(fill_class(f64::NAN, 10.0), "empty");
        assert_eq!(fill_class(0.001, 10.0), "ramp-0");
        assert_eq!(fill_class(2.0, 10.0), "ramp-0");
        assert_eq!(fill_class(5.0, 10.0), "ramp-2");
        assert_eq!(fill_class(10.0, 10.0), "ramp-4");
        assert_eq!(fill_class(500.0, 10.0), "ramp-4");
        assert_eq!(fill_class(f64::INFINITY, 10.0), "ramp-4");
    }

    #[test]
    fn busiest_tie_goes_to_the_earliest_day() {
        let day = |d: Date, cost: f64| {
            let entry = DayEntry { date: d.into(), start_ms: 0, cost, messages: 1, tokens: 1, by_model: Default::default(), source: Default::default() };
            (d, entry)
        };
        let days = [day(date(2026, 11, 4), 7.0), day(date(2026, 11, 12), 3.0), day(date(2026, 11, 20), 7.0)];
        let refs: Vec<&(Date, DayEntry)> = days.iter().collect();
        assert_eq!(busiest(&refs).map(|(d, _)| *d), Some(date(2026, 11, 4)));
        // Order in the slice doesn't matter.
        let reversed: Vec<&(Date, DayEntry)> = days.iter().rev().collect();
        assert_eq!(busiest(&reversed).map(|(d, _)| *d), Some(date(2026, 11, 4)));
        assert_eq!(busiest(&refs[1..2]).map(|(d, _)| *d), Some(date(2026, 11, 12)));
        assert!(busiest(&[]).is_none());
    }
}
