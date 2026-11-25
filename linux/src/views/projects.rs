//! Projects, top sessions, where Claude is used, agent split and the weekday × hour heatmap (ProjectsTab.swift).

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use nacut_core::views::SessionView;

use crate::charts;
use crate::model::{self, fmt, usd, AppModel};
use crate::prefs;
use crate::theme;
use crate::ui::{self, BoxAdd};

thread_local! {
    static SHOW_ALL: Cell<bool> = const { Cell::new(false) };
}

pub fn build(m: &AppModel, rebuild: Rc<dyn Fn()>) -> gtk::Widget {
    let range = prefs::range("projectsRange");
    let projects = m.engine.project_stats(range);
    let sessions = m.engine.sessions(range, 8);
    let entry = m.engine.entrypoint_stats(range);
    let (main, subagents) = m.engine.agent_split(range);
    let show_all = SHOW_ALL.get();

    let toggle: Option<gtk::Widget> = (projects.len() > 6).then(|| {
        let rb = rebuild.clone();
        ui::link_button(&if show_all { "Show less".into() } else { format!("Show all {}", projects.len()) }, move || {
            SHOW_ALL.set(!SHOW_ALL.get());
            rb();
        })
        .upcast()
    });
    let rows = ui::vbox(8);
    if projects.is_empty() {
        rows.add(&ui::empty("No project usage in this range"));
    }
    for p in projects.iter().take(if show_all { usize::MAX } else { 6 }) {
        let left = ui::hbox(6);
        left.add(&ui::icon("folder-symbolic", &["coral"])).add(&ui::label(&p.name, &["heading"]));
        let r = ui::vbox(4);
        r.add(&ui::spread(&left, Some(ui::label(&usd(p.cost), &["heading", "numeric"]).upcast_ref())))
            .add(&charts::bar(p.share, theme::coral(), 5, None, None))
            .add(&ui::dim(&format!(
                "{} session{} · {} msgs · last active {}",
                p.sessions, if p.sessions == 1 { "" } else { "s" }, p.messages, model::relative(p.last_active_ms)
            )));
        r.set_tooltip_text(Some(&p.path));
        rows.add(&r);
    }

    let session_rows = ui::vbox(4);
    if sessions.is_empty() {
        session_rows.add(&ui::empty("No sessions in this range"));
    }
    for s in &sessions {
        session_rows.add(&session_row(s));
    }

    let shares = ui::columns(
        2,
        12,
        vec![
            ui::card(Some("Where you use Claude"), None, &[charts::segmented_share(&entry.iter().map(|e| (e.name.clone(), e.value)).collect::<Vec<_>>()).upcast_ref()]).upcast(),
            ui::card(Some("Agents"), None, &[charts::segmented_share(&[("Main agent".into(), main), ("Subagents".into(), subagents)]).upcast_ref()]).upcast(),
        ],
    );

    let grid = m.engine.weekday_hour_heatmap(range);
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let g2 = grid.clone();
    let heat = charts::heatmap(grid, move |r, c| format!("{} {}: {}", days[r], model::hour_label(c as u32), usd(g2.get(r).and_then(|x| x.get(c)).copied().unwrap_or(0.0))));

    let v = ui::vbox(12);
    let rb = rebuild.clone();
    v.add(&super::models::range_picker(range, move |r| {
        prefs::set("projectsRange", prefs::range_key(r));
        rb();
    }))
    .add(&ui::card(Some("Projects"), toggle.as_ref(), &[rows.upcast_ref()]))
    .add(&ui::card(Some("Top sessions"), None, &[session_rows.upcast_ref()]))
    .add(&shares)
    .add(&ui::card(Some("When you use Claude"), Some(charts::ramp_legend().upcast_ref()), &[heat.upcast_ref()]));
    v.upcast()
}

pub fn session_row(s: &SessionView) -> gtk::Widget {
    let left = ui::vbox(2);
    let chips = ui::flow(s.models.iter().map(|mdl| ui::label(mdl, &["chip"]).upcast()).collect(), 4);
    left.add(&ui::label(&s.title, &["heading"]))
        .add(&ui::dim(&format!("{} · {} · {} · {}", s.project, fmt(s.start_ms, "%b %-d, %-I:%M %p"), duration(s.duration_secs), s.entrypoint)))
        .add(&chips);
    let right = ui::vbox(2);
    let c = ui::label(&usd(s.cost), &["heading", "numeric"]);
    c.set_halign(gtk::Align::End);
    let n = ui::dim(&format!("{} msgs", s.messages));
    n.set_halign(gtk::Align::End);
    right.add(&c).add(&n);
    right.set_valign(gtk::Align::Start);
    let row = ui::spread(&left, Some(right.upcast_ref()));
    row.set_margin_top(3);
    row.set_margin_bottom(3);
    row.upcast()
}

fn duration(secs: f64) -> String {
    let s = secs as i64;
    if s < 60 {
        return "<1m".into();
    }
    let (h, m) = (s / 3600, s % 3600 / 60);
    if h >= 48 {
        format!("{}d {}h", h / 24, h % 24)
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}
