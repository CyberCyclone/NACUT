//! Panel header: plan limits on a subscription, cost and budget otherwise (HeaderView.swift).

use gtk::prelude::*;
use nacut_core::forecast::BudgetStatus;
use nacut_core::views::{ForecastView, LimitView};

use crate::charts;
use crate::model::{pct, usd, usd_compact, usd_exact, AppModel};
use crate::theme;
use crate::ui::{self, BoxAdd};

pub fn build(m: &AppModel, open_settings: &std::rc::Rc<dyn Fn(&str)>) -> gtk::Widget {
    let snap = m.snap();
    let st = &snap.status;
    // Fall back to the cost header if limits can't be loaded, so the panel is never empty.
    let limits_failed = st.limits.is_empty() && st.limits_error.is_some();
    if st.shows_limits && !limits_failed {
        limits_header(m)
    } else {
        cost_header(m, open_settings)
    }
}

fn cost_header(m: &AppModel, open_settings: &std::rc::Rc<dyn Fn(&str)>) -> gtk::Widget {
    let snap = m.snap();
    let f = &snap.forecast;
    let left = ui::vbox(4);
    left.add(&ui::label(&f.cycle_label.to_uppercase(), &["dim-label", "small-caps"]));
    let big = ui::figure(&usd(f.spent), 26);
    big.set_tooltip_text(Some(&usd_exact(f.spent)));
    left.add(&big);
    let line = ui::hbox(10);
    let today = ui::hbox(4);
    today.add(&ui::icon("weather-clear-symbolic", &["coral"])).add(&ui::runs(&[("Today ", false), (&usd(f.today), true)]));
    line.add(&today).add(&ui::label("·", &["dim-label"])).add(&ui::runs(&[("Avg ", false), (&usd(f.daily_average), true), ("/day", false)]));
    left.add(&line);

    let right: gtk::Widget = if f.budget > 0.0 {
        let center = ui::vbox(0);
        let p = ui::figure(&pct(f.budget_used_fraction), 13);
        p.set_halign(gtk::Align::Center);
        let of = ui::dim(&format!("of {}", usd_compact(f.budget)));
        of.set_halign(gtk::Align::Center);
        center.add(&p).add(&of);
        let color = if f.budget_used_fraction >= 1.0 { theme::status(BudgetStatus::Exceeded) } else { theme::coral() };
        let ring = charts::ring(f.budget_used_fraction, Some(f.projected_fraction), color, theme::status(f.status), 84, 9.0, &center);
        ring.set_tooltip_text(Some(&format!("Budget {} used, projected {}", pct(f.budget_used_fraction), pct(f.projected_fraction))));
        ring.upcast()
    } else if m.is_subscription() && snap.settings.plan_price > 0.0 {
        value_badge(f.spent / snap.settings.plan_price, snap.settings.plan_price)
    } else {
        let b = gtk::Button::with_label("Set budget…");
        let open = open_settings.clone();
        b.connect_clicked(move |_| open("budget"));
        b.set_valign(gtk::Align::Center);
        b.upcast()
    };

    let v = ui::vbox(12);
    v.add(&ui::spread(&left, Some(&right))).add(&forecast_line(f));
    if snap.settings.limits_enabled {
        v.add(&limits_row(m));
    }
    v.upcast()
}

pub fn forecast_line(f: &ForecastView) -> gtk::Widget {
    let row = ui::hbox(6);
    row.add(&ui::icon(theme::status_icon(f.status), &[&format!("status-{}", theme::status_class(f.status))]))
        .add(&ui::label(&f.headline, &[]));
    let safe: Option<gtk::Widget> = match f.safe_daily_spend {
        Some(s) if f.status != BudgetStatus::Exceeded => {
            let l = ui::dim(&format!("{}/day left", usd(s)));
            l.set_tooltip_text(Some("Spend at most this much per day for the rest of the cycle to stay within budget"));
            Some(l.upcast())
        }
        _ => None,
    };
    ui::pill(&ui::spread(&row, safe.as_ref()), theme::status_class(f.status)).upcast()
}

fn value_badge(multiple: f64, plan: f64) -> gtk::Widget {
    let v = ui::vbox(2);
    v.add_css_class("value-badge");
    let x = ui::figure(&format!("{multiple:.1}×"), 20);
    x.add_css_class("coral");
    x.set_halign(gtk::Align::Center);
    let p = ui::dim(&format!("your {} plan", usd_compact(plan)));
    p.set_halign(gtk::Align::Center);
    v.add(&x).add(&p);
    v.set_valign(gtk::Align::Center);
    v.set_tooltip_text(Some("API-equivalent value of this cycle's usage compared with your subscription price"));
    v.upcast()
}

pub fn limits_row(m: &AppModel) -> gtk::Widget {
    let snap = m.snap();
    let st = &snap.status;
    if let (Some(err), true) = (&st.limits_error, st.limits.is_empty()) {
        let row = ui::hbox(6);
        let i = ui::icon("dialog-information-symbolic", &["dim-label"]);
        i.set_valign(gtk::Align::Start);
        row.add(&i).add(&ui::para(err, &["dim-label", "caption"]));
        return row.upcast();
    }
    if st.limits.is_empty() {
        let row = ui::hbox(6);
        let s = gtk::Spinner::new();
        s.start();
        row.add(&s).add(&ui::dim("Loading plan limits…"));
        return row.upcast();
    }
    let n = st.limits.len().min(3) as i32;
    ui::columns(n, 8, st.limits.iter().take(3).map(limit_gauge).collect()).upcast()
}

fn limit_gauge(l: &LimitView) -> gtk::Widget {
    let v = ui::vbox(5);
    let name = if l.id == "seven_day" { "Weekly".to_string() } else { l.label.clone() };
    v.add(&ui::spread(&ui::label(&name, &["caption-heading"]), Some(ui::label(&pct(l.utilization), &["caption-heading", "numeric"]).upcast_ref())));
    v.add(&charts::bar(l.utilization, theme::for_limit(l.utilization), 6, l.projected_at_reset, l.projected_at_reset.map(theme::for_limit)));
    v.add(&ui::dim(&l.subtitle));
    let c = ui::card(None, None, &[v.upcast_ref()]);
    c.set_tooltip_text(Some(&help_text(l)));
    c.upcast()
}

pub fn help_text(l: &LimitView) -> String {
    let mut s = format!("{}: {} used. {}.", l.label, pct(l.utilization), l.reset_text);
    if let Some(p) = l.projected_at_reset {
        s.push_str(&format!(" On pace for {} by reset.", pct(p)));
    }
    s
}

fn limits_header(m: &AppModel) -> gtk::Widget {
    let snap = m.snap();
    let st = &snap.status;
    let f = &snap.forecast;
    let v = ui::vbox(12);
    v.add(&ui::label(&format!("PLAN USAGE · {}", snap.plan_name.to_uppercase()), &["dim-label", "small-caps"]));
    if st.limits.is_empty() {
        drop(snap);
        v.add(&limits_row(m));
        return v.upcast();
    }
    let n = st.limits.len().min(3) as i32;
    v.add(&ui::columns(n, 8, st.limits.iter().take(3).map(limit_ring).collect()));
    if let Some(verdict) = &st.limits_verdict {
        let row = ui::hbox(6);
        row.add(&ui::icon(theme::status_icon(verdict.status), &[&format!("status-{}", theme::status_class(verdict.status))]))
            .add(&ui::label(&verdict.text, &[]));
        v.add(&ui::pill(&row, theme::status_class(verdict.status)));
    }
    let value = ui::hbox(6);
    value.add(&ui::label("$", &["coral", "heading"])).add(&ui::runs(&[
        ("API value ", false),
        (&usd(f.spent), true),
        (" this cycle · ", false),
        (&usd(f.today), true),
        (" today", false),
    ]));
    let multiple: Option<gtk::Widget> = (snap.settings.plan_price > 0.0).then(|| {
        let l = ui::label(&format!("{:.1}× plan", f.spent / snap.settings.plan_price), &["caption-heading", "coral"]);
        let p = ui::pill(&l, "small");
        p.add_css_class("value-badge");
        p.set_tooltip_text(Some(&format!("API-equivalent value of this cycle's usage compared with your {} subscription", usd_compact(snap.settings.plan_price))));
        p.set_valign(gtk::Align::Center);
        p.upcast()
    });
    v.add(&ui::spread(&value, multiple.as_ref()));
    v.upcast()
}

fn limit_ring(l: &LimitView) -> gtk::Widget {
    let u = l.utilization;
    let center = ui::figure(&pct(u), 13);
    let ring = charts::ring(u, l.projected_at_reset, theme::for_limit(u), theme::for_limit(l.projected_at_reset.unwrap_or(u)), 66, 7.0, &center);
    let v = ui::vbox(4);
    let title = ui::label(&l.title, &["caption-heading"]);
    title.set_halign(gtk::Align::Center);
    let sub = ui::dim(&l.subtitle);
    sub.set_halign(gtk::Align::Center);
    v.add(&ring).add(&title).add(&sub);
    let b = ui::subtle(&v, 10);
    let mut tip = help_text(l);
    if l.projected_at_reset.is_some() {
        tip.push_str(" The faint arc is the pace.");
    }
    b.set_tooltip_text(Some(&tip));
    b.upcast()
}
