//! Charts drawn with cairo in `GtkDrawingArea`s, following the same mark specs as the Mac and Windows
//! apps: thin marks, rounded bar ends, 2px lines, dashed projections, recessive axes, hover tooltips.

use std::cell::Cell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

use gtk::cairo::Context;
use gtk::prelude::*;
use nacut_core::CurrencyFormat;

use crate::model::{fmt, hour_label};
use crate::theme::{self, Rgb};
use crate::ui;

const AXIS_LEFT: f64 = 40.0;
const AXIS_BOTTOM: f64 = 18.0;
const DAY_MS: i64 = 86_400_000;
/// Donut hole radius as a fraction of the outer radius.
const DONUT_INNER: f64 = 0.64;
const HEAT_LABEL_W: f64 = 30.0;
const HEAT_ROW_H: f64 = 15.0;

pub fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, FRAC_PI_2 * 2.0);
    cr.arc(x + r, y + r, r, FRAC_PI_2 * 2.0, FRAC_PI_2 * 3.0);
    cr.close_path();
}

/// Rounded top corners only (bars anchored to the baseline).
fn top_rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h).max(0.0);
    cr.new_sub_path();
    cr.move_to(x, y + h);
    cr.line_to(x, y + r);
    cr.arc(x + r, y + r, r, FRAC_PI_2 * 2.0, FRAC_PI_2 * 3.0);
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.line_to(x + w, y + h);
    cr.close_path();
}

fn text(cr: &Context, widget: &gtk::DrawingArea, s: &str, x: f64, y: f64, align_right: bool, width: f64) {
    let layout = widget.create_pango_layout(Some(s));
    let mut desc = widget.pango_context().font_description().unwrap_or_default();
    desc.set_size(8 * gtk::pango::SCALE);
    layout.set_font_description(Some(&desc));
    let (tw, _) = layout.pixel_size();
    let x = if align_right { x + width - tw as f64 } else { x };
    theme::ink().set(cr, 0.62);
    cr.move_to(x, y);
    pangocairo::functions::show_layout(cr, &layout);
}

fn track(cr: &Context, alpha: f64) {
    theme::ink().set(cr, alpha);
}

/// Width of the plot area of a `width`-wide chart (left axis labels, 4px right margin).
fn plot_width(width: f64) -> f64 {
    width - AXIS_LEFT - 4.0
}

/// x of a timestamp on a time axis that starts at `start_ms` and spans `span_ms` (the left edge if the span is empty).
fn time_x(ms: i64, start_ms: i64, span_ms: f64, plot_w: f64) -> f64 {
    if span_ms.is_nan() || span_ms <= 0.0 {
        return AXIS_LEFT;
    }
    AXIS_LEFT + (ms - start_ms) as f64 / span_ms * plot_w
}

/// y of a value on an axis from 0 at the bottom of the plot to `y_max` at `top`.
fn value_y(v: f64, y_max: f64, top: f64, plot_h: f64) -> f64 {
    top + plot_h - v / y_max * plot_h
}

/// The day (0-based, clamped to the span) nearest to pointer `x` on a time axis spanning `span_ms`.
fn hover_day(x: f64, plot_w: f64, span_ms: f64) -> i64 {
    let last = (span_ms / DAY_MS as f64 - 1.0).max(0.0);
    ((x - AXIS_LEFT) / plot_w * span_ms / DAY_MS as f64).round().clamp(0.0, last) as i64
}

/// Index of the slot under pointer `x` when `n` equal slots share the plot width.
fn slot_at(x: f64, plot_w: f64, n: usize) -> Option<usize> {
    let i = ((x - AXIS_LEFT) / (plot_w / n.max(1) as f64)).floor();
    (i >= 0.0 && i < n as f64).then_some(i as usize)
}

/// Left edge and width of bar `i` of `n`, centered in its slot; `bar_width` 0 fills the slot less a 2px gap.
fn bar_rect(i: usize, n: usize, plot_w: f64, bar_width: f64) -> (f64, f64) {
    let slot = plot_w / n.max(1) as f64;
    let bw = if bar_width > 0.0 { bar_width.min(slot - 2.0) } else { (slot - 2.0).max(1.0) };
    (AXIS_LEFT + i as f64 * slot + (slot - bw) / 2.0, bw)
}

/// Start and end angle of each value's sector, in degrees clockwise from 12 o'clock.
fn donut_angles(values: &[f64]) -> Vec<(f64, f64)> {
    let total = values.iter().sum::<f64>().max(0.0001);
    let mut a = 0.0;
    values
        .iter()
        .map(|v| {
            let sweep = v / total * 360.0;
            let r = (a, a + sweep);
            a += sweep;
            r
        })
        .collect()
}

/// The sector under a pointer at (`dx`, `dy`) from the donut's center, if it's on the ring.
fn donut_hit(dx: f64, dy: f64, outer: f64, angles: &[(f64, f64)]) -> Option<usize> {
    let r = (dx * dx + dy * dy).sqrt();
    if r <= outer && r >= outer * DONUT_INNER {
        let mut a = dy.atan2(dx).to_degrees() + 90.0;
        if a < 0.0 {
            a += 360.0;
        }
        angles.iter().position(|(a0, a1)| a >= *a0 && a < *a1)
    } else {
        None
    }
}

/// The (weekday row, hour column) heatmap cell under (`x`, `y`) in a `width`-wide chart.
fn heat_cell(x: f64, y: f64, width: f64) -> Option<(usize, usize)> {
    let cell_w = (width - HEAT_LABEL_W) / 24.0;
    let (c, r) = (((x - HEAT_LABEL_W) / cell_w).floor(), (y / HEAT_ROW_H).floor());
    ((0.0..24.0).contains(&c) && (0.0..7.0).contains(&r)).then_some((r as usize, c as usize))
}

/// The rounded square drawn for heatmap cell (`r`, `c`): x, y, width, height.
fn heat_cell_rect(r: usize, c: usize, width: f64) -> (f64, f64, f64, f64) {
    let cell_w = (width - HEAT_LABEL_W) / 24.0;
    (HEAT_LABEL_W + c as f64 * cell_w + 1.5, r as f64 * HEAT_ROW_H + 1.5, cell_w - 3.0, 12.0)
}

/// Grid lines and $ labels; returns the y scale max.
fn y_axis(cr: &Context, w: &gtk::DrawingArea, max: f64, width: f64, top: f64, plot_h: f64) -> f64 {
    let t = nacut_core::views::chart_ticks(max.max(0.1), 3); // at least 10¢: smaller steps would round to the same cent label
    let y_max = *t.last().unwrap();
    for v in &t {
        let y = value_y(*v, y_max, top, plot_h);
        track(cr, 0.08);
        cr.set_line_width(1.0);
        cr.move_to(AXIS_LEFT, y.round() + 0.5);
        cr.line_to(width - 4.0, y.round() + 0.5);
        let _ = cr.stroke();
        text(cr, w, &CurrencyFormat::axis(*v), 0.0, y - 7.0, true, AXIS_LEFT - 6.0);
    }
    y_max
}

// MARK: Rings and bars

/// Progress ring: solid arc for the value, faint arc to the projection.
pub fn ring(fraction: f64, projected: Option<f64>, color: Rgb, projected_color: Rgb, size: i32, thickness: f64, center: &impl IsA<gtk::Widget>) -> gtk::Overlay {
    let area = gtk::DrawingArea::builder().content_width(size).content_height(size).halign(gtk::Align::Center).build();
    area.set_draw_func(move |_, cr, w, h| {
        let c = (w as f64 / 2.0, h as f64 / 2.0);
        let r = (w.min(h) as f64 - thickness) / 2.0;
        cr.set_line_width(thickness);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        track(cr, 0.1);
        cr.arc(c.0, c.1, r, 0.0, TAU);
        let _ = cr.stroke();
        let arc = |cr: &Context, f: f64| {
            let f = f.clamp(0.004, 0.9999);
            cr.arc(c.0, c.1, r, -FRAC_PI_2, -FRAC_PI_2 + f * TAU);
            let _ = cr.stroke();
        };
        if let Some(p) = projected.filter(|p| *p > fraction) {
            projected_color.set(cr, 0.25);
            arc(cr, p.min(1.0));
        }
        color.set(cr, 1.0);
        arc(cr, fraction);
    });
    let o = gtk::Overlay::new();
    o.set_child(Some(&area));
    center.set_halign(gtk::Align::Center);
    center.set_valign(gtk::Align::Center);
    o.add_overlay(center);
    o
}

/// Capsule bar: track, optional faint projection, solid value.
pub fn bar(fraction: f64, color: Rgb, height: i32, projected: Option<f64>, projected_color: Option<Rgb>) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::builder().content_height(height).hexpand(true).build();
    d.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        track(cr, 0.1);
        rounded_rect(cr, 0.0, 0.0, w, h, h / 2.0);
        let _ = cr.fill();
        if let Some(p) = projected.filter(|p| *p > fraction) {
            projected_color.unwrap_or(color).set(cr, 0.28);
            rounded_rect(cr, 0.0, 0.0, (w * p.min(1.0)).max(h), h, h / 2.0);
            let _ = cr.fill();
        }
        color.set(cr, 1.0);
        rounded_rect(cr, 0.0, 0.0, (w * fraction.clamp(0.0, 1.0)).max(h.min(4.0)), h, h / 2.0);
        let _ = cr.fill();
    });
    d
}

/// Part-to-whole as one segmented bar with a legend (≤ 4 parts).
pub fn segmented_share(items: &[(String, f64)]) -> gtk::Box {
    let total = items.iter().map(|i| i.1).sum::<f64>().max(0.0001);
    let shown: Vec<(String, f64)> = items.iter().take(4).cloned().collect();
    let values: Vec<f64> = shown.iter().map(|i| i.1).collect();
    let d = gtk::DrawingArea::builder().content_height(10).hexpand(true).build();
    d.set_draw_func(move |_, cr, w, h| {
        let gaps = (values.iter().filter(|v| **v > 0.0).count().saturating_sub(1)) as f64 * 2.0;
        let mut x = 0.0;
        for (i, v) in values.iter().enumerate() {
            if *v <= 0.0 {
                continue;
            }
            let seg = ((w as f64 - gaps) * v / total).max(3.0);
            theme::categorical(i).set(cr, 1.0);
            rounded_rect(cr, x, 0.0, seg, h as f64, 3.0);
            let _ = cr.fill();
            x += seg + 2.0;
        }
    });
    let b = ui::vbox(8);
    b.append(&d);
    for (i, (name, value)) in shown.iter().enumerate() {
        let row = ui::hbox(6);
        row.append(&ui::swatch(theme::categorical(i), 8));
        row.append(&ui::label(name, &["caption"]));
        let r = ui::spread(&row, Some(ui::dim(&crate::model::pct(value / total)).upcast_ref()));
        r.set_tooltip_text(Some(&crate::model::usd(*value)));
        b.append(&r);
    }
    b
}

// MARK: Cumulative spend

#[derive(Clone)]
pub struct LinePoint {
    pub day_ms: i64,
    pub value: f64,
    /// 0 = actual, 1 = projected, 2 = budget pace.
    pub series: u8,
}

/// Cumulative line with area fill, dashed projection and pace, budget rule, and a hover crosshair.
pub fn cumulative(points: Vec<LinePoint>, start_ms: i64, end_ms: i64, budget: f64, height: i32, tooltip: impl Fn(i64) -> String + 'static) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::builder().content_height(height).hexpand(true).has_tooltip(true).build();
    let hover: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let span = (end_ms - start_ms) as f64;
    {
        let hover = hover.clone();
        d.set_draw_func(move |area, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            let top = 14.0;
            let plot_w = plot_width(w);
            let plot_h = h - AXIS_BOTTOM - top;
            let max = points.iter().map(|p| p.value).fold(budget, f64::max) * 1.05;
            let y_max = y_axis(cr, area, max, w, top, plot_h);
            let x = |ms: i64| time_x(ms, start_ms, span, plot_w);
            let y = |v: f64| value_y(v, y_max, top, plot_h);

            let mut d0 = start_ms;
            while d0 < end_ms {
                if x(d0) + 36.0 > w {
                    break;
                }
                text(cr, area, &fmt(d0, "%b %-d"), x(d0) - 2.0, top + plot_h + 3.0, false, 0.0);
                d0 += 7 * DAY_MS;
            }

            let coral = theme::coral();
            let actual: Vec<&LinePoint> = points.iter().filter(|p| p.series == 0).collect();
            if let (Some(first), Some(last)) = (actual.first(), actual.last()) {
                let grad = gtk::cairo::LinearGradient::new(0.0, top, 0.0, top + plot_h);
                grad.add_color_stop_rgba(0.0, coral.r(), coral.g(), coral.b(), 0.22);
                grad.add_color_stop_rgba(1.0, coral.r(), coral.g(), coral.b(), 0.02);
                cr.move_to(x(first.day_ms), y(0.0));
                for p in &actual {
                    cr.line_to(x(p.day_ms), y(p.value));
                }
                cr.line_to(x(last.day_ms), y(0.0));
                cr.close_path();
                let _ = cr.set_source(&grad);
                let _ = cr.fill();
            }
            cr.set_line_join(gtk::cairo::LineJoin::Round);
            cr.set_line_cap(gtk::cairo::LineCap::Round);
            for series in [2u8, 1, 0] {
                let pts: Vec<&LinePoint> = points.iter().filter(|p| p.series == series).collect();
                if pts.len() < 2 {
                    continue;
                }
                match series {
                    0 => coral.set(cr, 1.0),
                    1 => coral.set(cr, 0.6),
                    _ => theme::ink().set(cr, 0.35),
                }
                cr.set_line_width(if series == 0 { 2.5 } else { 2.0 });
                cr.set_dash(if series == 0 { &[] } else { &[4.0, 4.0] }, 0.0);
                cr.move_to(x(pts[0].day_ms), y(pts[0].value));
                for p in &pts[1..] {
                    cr.line_to(x(p.day_ms), y(p.value));
                }
                let _ = cr.stroke();
            }
            cr.set_dash(&[], 0.0);
            if budget > 0.0 {
                theme::ink().set(cr, 0.4);
                cr.set_line_width(1.0);
                cr.move_to(AXIS_LEFT, y(budget).round() + 0.5);
                cr.line_to(w - 4.0, y(budget).round() + 0.5);
                let _ = cr.stroke();
                text(cr, area, &format!("Budget {}", CurrencyFormat::axis(budget)), AXIS_LEFT + 2.0, y(budget) - 15.0, false, 0.0);
            }
            if let Some(hx) = hover.get() {
                theme::ink().set(cr, 0.3);
                cr.set_line_width(1.0);
                cr.move_to(hx.round() + 0.5, top);
                cr.line_to(hx.round() + 0.5, top + plot_h);
                let _ = cr.stroke();
            }
        });
    }
    let motion = gtk::EventControllerMotion::new();
    {
        let hover = hover.clone();
        let d2 = d.clone();
        motion.connect_motion(move |_, x, _| {
            let plot_w = plot_width(d2.width() as f64);
            let day = hover_day(x, plot_w, span);
            hover.set(Some(time_x(start_ms + day * DAY_MS, start_ms, span, plot_w)));
            d2.queue_draw();
        });
    }
    {
        let hover = hover.clone();
        let d2 = d.clone();
        motion.connect_leave(move |_| {
            hover.set(None);
            d2.queue_draw();
        });
    }
    d.add_controller(motion);
    d.connect_query_tooltip(move |area, x, _, _, tip| {
        if (x as f64) < AXIS_LEFT {
            return false;
        }
        let day = hover_day(x as f64, plot_width(area.width() as f64), span);
        tip.set_text(Some(&tooltip(start_ms + day * DAY_MS)));
        true
    });
    d
}

// MARK: Stacked bars

#[derive(Clone)]
pub struct Stack {
    pub day_ms: i64,
    pub parts: Vec<(String, f64, Rgb)>,
}

/// One stacked bar per slot with a dashed average rule; hovering a bar shows its breakdown.
pub fn stacked_bars(stacks: Vec<Stack>, height: i32, average: Option<f64>, x_label: impl Fn(usize) -> Option<String> + 'static, tooltip: impl Fn(&Stack) -> String + 'static, bar_width: f64) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::builder().content_height(height).hexpand(true).has_tooltip(true).build();
    let stacks = Rc::new(stacks);
    let top = 6.0;
    {
        let stacks = stacks.clone();
        d.set_draw_func(move |area, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            let plot_w = plot_width(w);
            let plot_h = h - AXIS_BOTTOM - top;
            let max = stacks.iter().map(|s| s.parts.iter().map(|p| p.1).sum::<f64>()).fold(0.01, f64::max);
            let y_max = y_axis(cr, area, max, w, top, plot_h);
            let y = |v: f64| value_y(v, y_max, top, plot_h);
            for (i, s) in stacks.iter().enumerate() {
                let (x, bw) = bar_rect(i, stacks.len(), plot_w, bar_width);
                let parts: Vec<&(String, f64, Rgb)> = s.parts.iter().filter(|p| p.1 > 0.0).collect();
                let mut base = 0.0;
                for (j, p) in parts.iter().enumerate() {
                    let y0 = y(base);
                    let y1 = y(base + p.1);
                    base += p.1;
                    if y0 - y1 < 0.5 {
                        continue;
                    }
                    p.2.set(cr, 1.0);
                    let gap = if j + 1 < parts.len() { 1.0 } else { 0.0 };
                    if j + 1 == parts.len() {
                        top_rounded_rect(cr, x, y1, bw, y0 - y1, 2.0);
                    } else {
                        cr.rectangle(x, y1 + gap, bw, (y0 - y1 - gap).max(0.5));
                    }
                    let _ = cr.fill();
                }
                if let Some(l) = x_label(i) {
                    text(cr, area, &l, x - 2.0, top + plot_h + 3.0, false, 0.0);
                }
            }
            if let Some(avg) = average.filter(|a| *a > 0.0) {
                theme::ink().set(cr, 0.35);
                cr.set_line_width(1.0);
                cr.set_dash(&[3.0, 3.0], 0.0);
                cr.move_to(AXIS_LEFT, y(avg).round() + 0.5);
                cr.line_to(w - 4.0, y(avg).round() + 0.5);
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
            }
        });
    }
    d.connect_query_tooltip(move |area, x, _, _, tip| {
        match slot_at(x as f64, plot_width(area.width() as f64), stacks.len()) {
            Some(i) if !stacks[i].parts.iter().all(|p| p.1 <= 0.0) => {
                tip.set_text(Some(&tooltip(&stacks[i])));
                true
            }
            _ => false,
        }
    });
    d
}

/// Two bars per day (billed vs local), for the reconciliation card.
pub fn grouped_bars(rows: Vec<(i64, f64, f64)>, ca: Rgb, cb: Rgb, height: i32) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::builder().content_height(height).hexpand(true).has_tooltip(true).build();
    let rows = Rc::new(rows);
    let top = 6.0;
    {
        let rows = rows.clone();
        d.set_draw_func(move |area, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            let plot_w = plot_width(w);
            let plot_h = h - AXIS_BOTTOM - top;
            let max = rows.iter().map(|r| r.1.max(r.2)).fold(0.01, f64::max);
            let y_max = y_axis(cr, area, max, w, top, plot_h);
            let y = |v: f64| value_y(v, y_max, top, plot_h);
            let slot = plot_w / rows.len().max(1) as f64;
            let bw = ((slot - 3.0) / 2.0).max(1.0);
            for (i, (day, a, b)) in rows.iter().enumerate() {
                let x = AXIS_LEFT + i as f64 * slot + 1.0;
                for (v, c, dx) in [(*a, ca, 0.0), (*b, cb, bw + 1.0)] {
                    if y(0.0) - y(v) < 0.5 {
                        continue;
                    }
                    c.set(cr, 1.0);
                    top_rounded_rect(cr, x + dx, y(v), bw, y(0.0) - y(v), 2.0);
                    let _ = cr.fill();
                }
                if i % 7 == 0 {
                    text(cr, area, &fmt(*day, "%b %-d"), x - 2.0, top + plot_h + 3.0, false, 0.0);
                }
            }
        });
    }
    d.connect_query_tooltip(move |area, x, _, _, tip| {
        let Some(i) = slot_at(x as f64, plot_width(area.width() as f64), rows.len()) else { return false };
        let (day, a, b) = rows[i];
        tip.set_text(Some(&format!("{}\nBilled {} · Local {}", fmt(day, "%a %b %-d"), crate::model::usd(a), crate::model::usd(b))));
        true
    });
    d
}

// MARK: Donut

/// Donut with a 1.5° gap between sectors; hovering a sector dims the rest and updates the center.
pub fn donut(items: Vec<(String, f64, Rgb)>, size: i32, total_label: String) -> gtk::Overlay {
    let d = gtk::DrawingArea::builder().content_width(size).content_height(size).build();
    let selected: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let angles: Rc<Vec<(f64, f64)>> = Rc::new(donut_angles(&items.iter().map(|i| i.1).collect::<Vec<_>>()));
    {
        let selected = selected.clone();
        let angles = angles.clone();
        let colors: Vec<Rgb> = items.iter().map(|i| i.2).collect();
        let n = items.len();
        d.set_draw_func(move |_, cr, w, h| {
            let c = (w as f64 / 2.0, h as f64 / 2.0);
            let outer = w.min(h) as f64 / 2.0;
            let inner = outer * DONUT_INNER;
            for (i, (a0, a1)) in angles.iter().enumerate() {
                if a1 - a0 <= 0.0 {
                    continue;
                }
                let gap = if n > 1 { (1.5f64).min((a1 - a0) / 3.0) } else { 0.0 };
                let (s, e) = ((a0 + gap / 2.0 - 90.0).to_radians(), (a1 - gap / 2.0 - 90.0).to_radians());
                cr.new_path();
                cr.arc(c.0, c.1, outer, s, e);
                cr.arc_negative(c.0, c.1, inner, e, s);
                cr.close_path();
                let alpha = match selected.get() {
                    Some(sel) if sel != i => 0.35,
                    _ => 1.0,
                };
                colors[i].set(cr, alpha);
                let _ = cr.fill();
            }
        });
    }
    let title = ui::dim("Total");
    let value = ui::figure(&total_label, 12);
    let center = ui::vbox(0);
    title.set_halign(gtk::Align::Center);
    value.set_halign(gtk::Align::Center);
    center.append(&title);
    center.append(&value);
    center.set_can_target(false);

    let motion = gtk::EventControllerMotion::new();
    {
        let (selected, angles, d2, title, value) = (selected.clone(), angles.clone(), d.clone(), title.clone(), value.clone());
        let names: Vec<(String, f64)> = items.iter().map(|i| (i.0.clone(), i.1)).collect();
        let total_label = total_label.clone();
        let size = size as f64;
        motion.connect_motion(move |_, x, y| {
            let hit = donut_hit(x - size / 2.0, y - size / 2.0, size / 2.0, &angles);
            if hit != selected.get() {
                selected.set(hit);
                match hit {
                    Some(i) => {
                        title.set_label(&names[i].0);
                        value.set_label(&CurrencyFormat::axis(names[i].1));
                    }
                    None => {
                        title.set_label("Total");
                        value.set_label(&total_label);
                    }
                }
                d2.queue_draw();
            }
        });
    }
    {
        let (selected, d2, title, value) = (selected, d.clone(), title, value);
        motion.connect_leave(move |_| {
            selected.set(None);
            title.set_label("Total");
            value.set_label(&total_label);
            d2.queue_draw();
        });
    }
    d.add_controller(motion);
    let o = gtk::Overlay::new();
    o.set_child(Some(&d));
    center.set_halign(gtk::Align::Center);
    center.set_valign(gtk::Align::Center);
    o.add_overlay(&center);
    o.set_valign(gtk::Align::Center);
    o
}

// MARK: Horizontal bars

pub fn hbars(rows: Vec<(String, f64, String)>, color: Rgb) -> gtk::Grid {
    let g = gtk::Grid::builder().column_spacing(8).row_spacing(6).build();
    let max = rows.iter().map(|r| r.1).fold(0.0001, f64::max) * 1.6;
    for (i, (name, value, annotation)) in rows.into_iter().enumerate() {
        g.attach(&ui::dim(&name), 0, i as i32, 1, 1);
        let f = (value / max).clamp(0.0, 1.0);
        let d = gtk::DrawingArea::builder().content_height(16).hexpand(true).build();
        d.set_draw_func(move |area, cr, w, h| {
            let bw = (w as f64 * f).max(2.0);
            color.set(cr, 1.0);
            rounded_rect(cr, 0.0, 1.0, bw, h as f64 - 2.0, 3.0);
            let _ = cr.fill();
            text(cr, area, &annotation, bw + 6.0, 0.0, false, 0.0);
        });
        g.attach(&d, 1, i as i32, 1, 1);
    }
    g
}

// MARK: Heatmap

pub fn heatmap(grid: Vec<Vec<f64>>, tip: impl Fn(usize, usize) -> String + 'static) -> gtk::Box {
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let max = grid.iter().flatten().fold(0.0001f64, |a, b| a.max(*b));
    let d = gtk::DrawingArea::builder().content_height(7 * 15).hexpand(true).has_tooltip(true).build();
    let grid = Rc::new(grid);
    {
        let grid = grid.clone();
        d.set_draw_func(move |area, cr, w, _| {
            for r in 0..7 {
                text(cr, area, days[r], 0.0, r as f64 * HEAT_ROW_H, false, 0.0);
                for c in 0..24 {
                    let v = grid.get(r).and_then(|row| row.get(c)).copied().unwrap_or(0.0);
                    match theme::ramp(v / max) {
                        Some(col) if v > 0.0 => col.set(cr, 1.0),
                        _ => track(cr, 0.06),
                    }
                    let (x, y, cw, ch) = heat_cell_rect(r, c, w as f64);
                    rounded_rect(cr, x, y, cw, ch, 2.5);
                    let _ = cr.fill();
                }
            }
        });
    }
    d.connect_query_tooltip(move |area, x, y, _, t| {
        let Some((r, c)) = heat_cell(x as f64, y as f64, area.width() as f64) else { return false };
        t.set_text(Some(&tip(r, c)));
        true
    });
    let hours = ui::hbox(0);
    hours.set_margin_start(HEAT_LABEL_W as i32);
    for h in [0, 6, 12, 18] {
        let l = ui::dim(&hour_label(h));
        l.set_hexpand(true);
        hours.append(&l);
    }
    let b = ui::vbox(3);
    b.append(&d);
    b.append(&hours);
    b
}

pub fn ramp_legend() -> gtk::Box {
    let b = ui::hbox(2);
    b.append(&ui::dim("Less"));
    for i in 0..theme::RAMP_LEN {
        b.append(&ui::swatch(theme::ramp_step(i), 9));
    }
    b.append(&ui::dim("More"));
    b
}

pub fn legend(items: Vec<(String, Rgb)>) -> gtk::FlowBox {
    ui::flow(
        items
            .into_iter()
            .map(|(name, c)| {
                let b = ui::hbox(4);
                b.append(&ui::swatch(c, 10));
                b.append(&ui::dim(&name));
                b.upcast()
            })
            .collect(),
        10,
    )
}

pub fn legend_line(name: &str, color: Rgb, dashed: bool, alpha: f64) -> gtk::Box {
    let d = gtk::DrawingArea::builder().content_width(12).content_height(6).valign(gtk::Align::Center).build();
    d.set_draw_func(move |_, cr, _, _| {
        color.set(cr, alpha);
        cr.set_line_width(2.0);
        if dashed {
            cr.set_dash(&[3.0, 2.0], 0.0);
        }
        cr.move_to(0.0, 3.0);
        cr.line_to(12.0, 3.0);
        let _ = cr.stroke();
    });
    let b = ui::hbox(4);
    b.append(&d);
    b.append(&ui::dim(name));
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: i64 = 1_793_491_200_000; // 2026-11-01T00:00Z
    const SPAN_30: f64 = 30.0 * DAY_MS as f64;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn plot_width_leaves_room_for_the_axis() {
        assert_eq!(plot_width(400.0), 356.0);
        assert_eq!(plot_width(AXIS_LEFT + 4.0), 0.0);
    }

    #[test]
    fn time_axis() {
        assert_eq!(time_x(START, START, SPAN_30, 300.0), AXIS_LEFT);
        assert_eq!(time_x(START + 30 * DAY_MS, START, SPAN_30, 300.0), AXIS_LEFT + 300.0);
        assert!(close(time_x(START + 15 * DAY_MS, START, SPAN_30, 300.0), AXIS_LEFT + 150.0));
        assert!(time_x(START - DAY_MS, START, SPAN_30, 300.0) < AXIS_LEFT);
    }

    #[test]
    fn time_axis_with_an_empty_span() {
        assert_eq!(time_x(START + DAY_MS, START, 0.0, 300.0), AXIS_LEFT);
        assert_eq!(time_x(START, START, -1.0, 300.0), AXIS_LEFT);
        assert_eq!(time_x(START, START, f64::NAN, 300.0), AXIS_LEFT);
    }

    #[test]
    fn value_axis() {
        assert_eq!(value_y(0.0, 10.0, 6.0, 100.0), 106.0);
        assert_eq!(value_y(10.0, 10.0, 6.0, 100.0), 6.0);
        assert_eq!(value_y(5.0, 10.0, 6.0, 100.0), 56.0);
        assert!(value_y(20.0, 10.0, 6.0, 100.0) < 6.0);
    }

    #[test]
    fn y_scale_from_core_ticks_fits_the_data() {
        for max in [0.1, 0.37, 1.0, 4.2, 17.0, 99.0, 1234.5, 98_765.0] {
            let ticks = nacut_core::views::chart_ticks(max, 3);
            let y_max = *ticks.last().unwrap();
            assert!(y_max >= max, "{max}: {ticks:?}");
            let y = value_y(max, y_max, 14.0, 200.0);
            assert!((14.0..=214.0).contains(&y), "{max}");
        }
    }

    #[test]
    fn hover_snaps_to_the_nearest_day() {
        // 30 days over 300px: 10px a day.
        assert_eq!(hover_day(AXIS_LEFT, 300.0, SPAN_30), 0);
        assert_eq!(hover_day(AXIS_LEFT + 4.9, 300.0, SPAN_30), 0);
        assert_eq!(hover_day(AXIS_LEFT + 5.1, 300.0, SPAN_30), 1);
        assert_eq!(hover_day(AXIS_LEFT + 123.0, 300.0, SPAN_30), 12);
        assert_eq!(hover_day(AXIS_LEFT + 291.0, 300.0, SPAN_30), 29);
    }

    #[test]
    fn hover_clamps_to_the_span() {
        assert_eq!(hover_day(0.0, 300.0, SPAN_30), 0);
        assert_eq!(hover_day(AXIS_LEFT + 300.0, 300.0, SPAN_30), 29);
        assert_eq!(hover_day(10_000.0, 300.0, SPAN_30), 29);
        assert_eq!(hover_day(AXIS_LEFT + 150.0, 300.0, DAY_MS as f64), 0);
    }

    #[test]
    fn hover_degenerate_inputs_do_not_panic() {
        assert_eq!(hover_day(f64::NAN, 300.0, SPAN_30), 0);
        assert_eq!(hover_day(AXIS_LEFT, 0.0, SPAN_30), 0);
        assert_eq!(hover_day(AXIS_LEFT + 10.0, 0.0, SPAN_30), 29);
        assert_eq!(hover_day(AXIS_LEFT + 10.0, 300.0, 0.0), 0);
        assert_eq!(hover_day(AXIS_LEFT + 10.0, 300.0, -SPAN_30), 0);
        assert_eq!(hover_day(AXIS_LEFT + 10.0, 300.0, f64::NAN), 0);
    }

    #[test]
    fn hover_round_trips_every_day() {
        for plot_w in [120.0, 300.0, 412.0] {
            for d in 0..30 {
                let x = time_x(START + d * DAY_MS, START, SPAN_30, plot_w);
                assert_eq!(hover_day(x, plot_w, SPAN_30), d, "{plot_w} {d}");
            }
        }
    }

    #[test]
    fn slot_under_the_pointer() {
        // 30 slots over 300px: 10px each.
        assert_eq!(slot_at(AXIS_LEFT, 300.0, 30), Some(0));
        assert_eq!(slot_at(AXIS_LEFT + 9.99, 300.0, 30), Some(0));
        assert_eq!(slot_at(AXIS_LEFT + 10.0, 300.0, 30), Some(1));
        assert_eq!(slot_at(AXIS_LEFT + 299.9, 300.0, 30), Some(29));
        assert_eq!(slot_at(AXIS_LEFT + 300.0, 300.0, 30), None);
        assert_eq!(slot_at(AXIS_LEFT - 0.1, 300.0, 30), None);
        assert_eq!(slot_at(AXIS_LEFT + 150.0, 300.0, 1), Some(0));
    }

    #[test]
    fn slot_degenerate_inputs() {
        assert_eq!(slot_at(AXIS_LEFT + 5.0, 300.0, 0), None);
        assert_eq!(slot_at(f64::NAN, 300.0, 30), None);
        assert_eq!(slot_at(AXIS_LEFT, 0.0, 30), None);
        assert_eq!(slot_at(AXIS_LEFT + 5.0, 0.0, 30), None);
        assert_eq!(slot_at(AXIS_LEFT + 5.0, -10.0, 30), None);
    }

    #[test]
    fn bars_are_centered_in_their_slots() {
        for (n, plot_w, bar_width) in [(24, 240.0, 9.0), (24, 240.0, 0.0), (30, 356.0, 0.0), (7, 100.0, 5.0)] {
            let slot = plot_w / n as f64;
            for i in 0..n {
                let (x, w) = bar_rect(i, n, plot_w, bar_width);
                assert!(close(x + w / 2.0, AXIS_LEFT + (i as f64 + 0.5) * slot));
                assert!(w > 0.0 && w <= slot - 2.0);
                assert_eq!(slot_at(x + w / 2.0, plot_w, n), Some(i));
            }
        }
    }

    #[test]
    fn bar_widths() {
        assert_eq!(bar_rect(0, 24, 240.0, 9.0), (41.0, 8.0)); // capped at the slot less 2px
        assert_eq!(bar_rect(0, 24, 240.0, 5.0), (42.5, 5.0));
        assert_eq!(bar_rect(1, 24, 240.0, 0.0), (51.0, 8.0));
        assert_eq!(bar_rect(0, 100, 100.0, 0.0).1, 1.0); // at least 1px
        assert_eq!(bar_rect(0, 0, 240.0, 0.0), (41.0, 238.0));
    }

    #[test]
    fn donut_sectors() {
        assert!(donut_angles(&[]).is_empty());
        assert_eq!(donut_angles(&[1.0, 1.0, 2.0]), [(0.0, 90.0), (90.0, 180.0), (180.0, 360.0)]);
        assert_eq!(donut_angles(&[5.0]), [(0.0, 360.0)]);
        assert_eq!(donut_angles(&[1.0, 0.0, 3.0]), [(0.0, 90.0), (90.0, 90.0), (90.0, 360.0)]);
        assert_eq!(donut_angles(&[0.0, 0.0]), [(0.0, 0.0), (0.0, 0.0)]);
    }

    #[test]
    fn donut_sectors_are_contiguous() {
        let a = donut_angles(&[3.2, 0.7, 11.0, 0.01, 5.5]);
        assert_eq!(a[0].0, 0.0);
        assert!(a.windows(2).all(|w| w[0].1 == w[1].0));
        assert!(close(a.last().unwrap().1, 360.0));
    }

    #[test]
    fn donut_hit_by_angle() {
        let quarters = donut_angles(&[1.0, 1.0, 1.0, 1.0]);
        assert_eq!(donut_hit(0.0, -40.0, 50.0, &quarters), Some(0)); // 12 o'clock
        assert_eq!(donut_hit(40.0, 0.0, 50.0, &quarters), Some(1)); // 3 o'clock
        assert_eq!(donut_hit(0.0, 40.0, 50.0, &quarters), Some(2));
        assert_eq!(donut_hit(-40.0, 0.0, 50.0, &quarters), Some(3));
        assert_eq!(donut_hit(-40.0, -0.0, 50.0, &quarters), Some(3));
        assert_eq!(donut_hit(-20.0, -30.0, 50.0, &quarters), Some(3));
        assert_eq!(donut_hit(-0.001, -40.0, 50.0, &quarters), Some(3)); // just left of 12
        assert_eq!(donut_hit(25.0, -25.0, 50.0, &quarters), Some(0));
    }

    #[test]
    fn donut_hit_by_radius() {
        let one = donut_angles(&[1.0]);
        assert_eq!(donut_hit(50.0, 0.0, 50.0, &one), Some(0));
        assert_eq!(donut_hit(50.01, 0.0, 50.0, &one), None);
        assert_eq!(donut_hit(32.0, 0.0, 50.0, &one), Some(0));
        assert_eq!(donut_hit(31.9, 0.0, 50.0, &one), None);
        assert_eq!(donut_hit(0.0, 0.0, 50.0, &one), None);
        assert_eq!(donut_hit(f64::NAN, 0.0, 50.0, &one), None);
        assert_eq!(donut_hit(40.0, 0.0, 50.0, &[]), None);
    }

    #[test]
    fn donut_hit_skips_empty_sectors() {
        let a = donut_angles(&[0.0, 1.0]);
        assert_eq!(donut_hit(0.0, -40.0, 50.0, &a), Some(1));
    }

    #[test]
    fn heatmap_cells() {
        // 270px wide: 10px per hour after the 30px labels; rows are 15px.
        assert_eq!(heat_cell(30.0, 0.0, 270.0), Some((0, 0)));
        assert_eq!(heat_cell(29.9, 0.0, 270.0), None);
        assert_eq!(heat_cell(45.0, 16.0, 270.0), Some((1, 1)));
        assert_eq!(heat_cell(269.9, 104.9, 270.0), Some((6, 23)));
        assert_eq!(heat_cell(270.0, 0.0, 270.0), None);
        assert_eq!(heat_cell(100.0, 105.0, 270.0), None);
        assert_eq!(heat_cell(100.0, -0.1, 270.0), None);
        assert_eq!(heat_cell(f64::NAN, 0.0, 270.0), None);
        assert_eq!(heat_cell(100.0, f64::NAN, 270.0), None);
        assert_eq!(heat_cell(100.0, 0.0, HEAT_LABEL_W), None);
    }

    #[test]
    fn heatmap_cells_round_trip() {
        for width in [270.0, 412.0] {
            for r in 0..7 {
                for c in 0..24 {
                    let (x, y, w, h) = heat_cell_rect(r, c, width);
                    assert!(w > 0.0 && h > 0.0);
                    assert_eq!(heat_cell(x + w / 2.0, y + h / 2.0, width), Some((r, c)));
                    assert_eq!(heat_cell(x, y, width), Some((r, c)));
                    assert_eq!(heat_cell(x + w, y + h, width), Some((r, c)));
                }
            }
        }
    }
}
