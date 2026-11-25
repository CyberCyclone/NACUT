//! Cost by model (donut + rows), token classes and billed-vs-local reconciliation (ModelsTab.swift).

use std::rc::Rc;

use gtk::prelude::*;
use nacut_core::engine::StatRange;
use nacut_core::views::ModelView;

use crate::charts;
use crate::model::{self, pct, tokens, usd, usd_compact, usd_exact, AppModel};
use crate::prefs;
use crate::theme;
use crate::ui::{self, BoxAdd};

pub fn build(m: &AppModel, rebuild: Rc<dyn Fn()>) -> gtk::Widget {
    let range = prefs::range("modelsRange");
    let stats = m.engine.model_stats(range);
    let total: f64 = stats.iter().map(|s| s.cost).sum();
    let v = ui::vbox(12);
    let rb = rebuild.clone();
    v.add(&range_picker(range, move |r| {
        prefs::set("modelsRange", prefs::range_key(r));
        rb();
    }));

    if stats.is_empty() {
        v.add(&ui::card(None, None, &[ui::empty("No model usage in this range").upcast_ref()]));
    } else {
        let donut = charts::donut(stats.iter().map(|s| (s.name.clone(), s.cost, theme::for_model(&s.id))).collect(), 130, usd_compact(total));
        let legend = ui::vbox(6);
        legend.set_valign(gtk::Align::Center);
        legend.set_hexpand(true);
        for s in stats.iter().take(6) {
            let left = ui::hbox(6);
            left.add(&ui::swatch(theme::for_model(&s.id), 10)).add(&ui::label(&s.name, &[]));
            legend.add(&ui::spread(&left, Some(ui::label(&pct(s.share), &["dim-label", "numeric"]).upcast_ref())));
        }
        let row = ui::hbox(16);
        row.add(&donut).add(&legend);
        v.add(&ui::card(Some("Cost by model"), None, &[row.upcast_ref()]));
        for s in &stats {
            v.add(&model_row(s));
        }
        v.add(&token_card(m, range));
        if stats.iter().any(|s| s.estimated) {
            v.add(&ui::para(
                "Some models aren't in the bundled price list, so they're priced like the newest model in their family. Add exact prices in Preferences → Pricing.",
                &["dim-label", "caption"],
            ));
        }
    }
    if !m.snap().reconciliation.is_empty() {
        v.add(&reconciliation_card(m));
    }
    v.upcast()
}

pub fn range_picker(current: StatRange, changed: impl Fn(StatRange) + 'static) -> gtk::Box {
    let ranges = [StatRange::Cycle, StatRange::Last30, StatRange::AllTime];
    let labels: Vec<&str> = ranges.iter().map(|r| model::range_title(*r)).collect();
    let selected = ranges.iter().position(|r| *r == current).unwrap_or(0);
    ui::segmented(&labels, selected, move |i| {
        if ranges[i] != current {
            changed(ranges[i]);
        }
    })
}

fn model_row(s: &ModelView) -> gtk::Widget {
    let title = ui::hbox(6);
    title.add(&ui::dot(theme::for_model(&s.id), 9)).add(&ui::label(&s.name, &["heading"]));
    if s.estimated {
        let i = ui::icon("dialog-question-symbolic", &["dim-label"]);
        i.set_tooltip_text(Some("Estimated price"));
        title.add(&i);
    }
    let cost = ui::label(&usd(s.cost), &["heading", "numeric"]);
    cost.set_tooltip_text(Some(&usd_exact(s.cost)));
    let metric = |t: &str, val: String| -> gtk::Widget {
        let v = ui::vbox(1);
        v.add(&ui::dim(t)).add(&ui::label(&val, &["caption-heading", "numeric"]));
        v.upcast()
    };
    let v = ui::vbox(6);
    v.add(&ui::spread(&title, Some(cost.upcast_ref())))
        .add(&charts::bar(s.share, theme::for_model(&s.id), 5, None, None))
        .add(&ui::columns(4, 10, vec![metric("Messages", s.messages.to_string()), metric("Tokens", tokens(s.tokens)), metric("Per msg", usd_exact(s.cost_per_message)), metric("Fast", pct(s.fast_share))]))
        .add(&ui::columns(4, 10, vec![metric("Input", tokens(s.input)), metric("Output", tokens(s.output)), metric("Cache write", tokens(s.cache_write)), metric("Cache read", tokens(s.cache_read))]));
    ui::card(None, None, &[v.upcast_ref()]).upcast()
}

fn token_card(m: &AppModel, range: StatRange) -> gtk::Widget {
    let classes = m.engine.token_classes(range);
    let total = classes.iter().map(|c| c.value).sum::<f64>().max(0.0001);
    let top = classes.iter().max_by(|a, b| a.value.total_cmp(&b.value)).filter(|c| c.value > 0.0).map(|c| format!("{} is {} of cost in this range.", c.name, pct(c.value / total)));
    let bars = charts::hbars(classes.iter().map(|c| (c.name.clone(), c.value, format!("{} · {} tok", usd_compact(c.value), tokens(c.tokens)))).collect(), theme::coral());
    let note = ui::dim(&top.unwrap_or_default());
    ui::card(Some("Where the money goes"), None, &[bars.upcast_ref(), note.upcast_ref()]).upcast()
}

fn reconciliation_card(m: &AppModel) -> gtk::Widget {
    let snap = m.snap();
    let days = &snap.reconciliation;
    let api: f64 = days.iter().map(|d| d.api).sum();
    let logs: f64 = days.iter().map(|d| d.logs).sum();
    let rows = days
        .iter()
        .map(|d| (model::date_of(&d.date).to_zoned(jiff::tz::TimeZone::system()).map(|z| z.timestamp().as_millisecond()).unwrap_or(0), d.api, d.logs))
        .collect();
    let chart = charts::grouped_bars(rows, theme::categorical(0), theme::coral(), 130);
    let text = if logs > 0.0 {
        format!(
            "Over {} billed days the API shows {} vs {} from local logs ({}). Live estimates are scaled by ×{:.2}.",
            days.len(), usd(api), usd(logs), pct(api / logs), snap.api.calibration.ratio
        )
    } else {
        "No overlapping days yet.".into()
    };
    let legend = charts::legend(vec![("Billed (API)".into(), theme::categorical(0)), ("Local logs".into(), theme::coral())]);
    ui::card(Some("Billed vs local estimate"), None, &[chart.upcast_ref(), legend.upcast_ref(), ui::para(&text, &["dim-label", "caption"]).upcast_ref()]).upcast()
}
