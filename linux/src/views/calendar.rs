//! Month heatmap with swipe paging (AdwCarousel), a month summary and per-day detail (CalendarTab.swift).

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use jiff::civil::Date;
use nacut_core::store::CostSource;
use nacut_core::views::DayEntry;

use crate::charts::{self, Stack};
use crate::model::{self, fmt, usd, usd_compact, usd_exact, AppModel};
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
    let this_month = today.first_of_month();
    let first = series.iter().map(|(d, _)| *d).min().unwrap_or(this_month).first_of_month();
    let mut months = vec![];
    let mut mo = first.min(this_month);
    while mo <= this_month {
        months.push(mo);
        mo = mo.checked_add(jiff::Span::new().months(1)).unwrap();
    }
    let current = VISIBLE_MONTH.get().filter(|v| months.contains(v)).unwrap_or(*months.last().unwrap());

    // Scale shared by all months so colors are comparable: the 95th percentile active day.
    let mut costs: Vec<f64> = series.iter().map(|(_, d)| d.cost).filter(|c| *c > 0.0).collect();
    costs.sort_by(f64::total_cmp);
    let scale_max = if costs.is_empty() { 1.0 } else { costs[((costs.len() as f64 * 0.95) as usize).min(costs.len() - 1)].max(0.01) };

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
    let lead = month.weekday().to_monday_zero_offset() as i32;
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
        let slot = lead + d;
        g.attach(&cell, slot % 7, 1 + slot / 7, 1, 1);
    }
    g.upcast()
}

fn day_cell(day: Date, entry: Option<&DayEntry>, scale_max: f64, selected: bool, is_today: bool, future: bool) -> gtk::Box {
    let value = entry.map(|e| e.cost).unwrap_or(0.0);
    let fraction = (value / scale_max).min(1.0);
    let b = ui::vbox(0);
    b.add_css_class("day-cell");
    b.set_size_request(-1, 40);
    if value > 0.0 {
        let i = ((fraction * theme::RAMP_LEN as f64).ceil() as usize).saturating_sub(1).min(theme::RAMP_LEN - 1);
        b.add_css_class(&format!("ramp-{i}"));
    } else {
        b.add_css_class("empty");
    }
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
        let a = ui::label(&usd_compact(value), &["caption-heading", "numeric"]);
        a.set_vexpand(true);
        a.set_valign(gtk::Align::End);
        b.add(&a);
    }
    let tip = format!("{}: {}{}", day.strftime("%A, %B %-d"), usd_exact(value), entry.map(|e| format!(" · {} messages", e.messages)).unwrap_or_default());
    b.set_tooltip_text(Some(&tip));
    b
}

fn month_summary(series: &[(Date, DayEntry)], month: Date) -> gtk::Widget {
    let days: Vec<&(Date, DayEntry)> = series.iter().filter(|(d, e)| d.year() == month.year() && d.month() == month.month() && e.cost > 0.0).collect();
    let total: f64 = days.iter().map(|(_, e)| e.cost).sum();
    let busiest = days.iter().max_by(|a, b| a.1.cost.total_cmp(&b.1.cost));
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
            item("Busiest", &busiest.map(|(d, e)| format!("{} · {}", d.day(), usd_compact(e.cost))).unwrap_or("—".into())),
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
