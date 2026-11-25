//! Charts drawn with cairo in `GtkDrawingArea`s, following the same mark specs as the Mac and Windows
//! apps: thin marks, rounded bar ends, 2px lines, dashed projections, recessive axes, hover tooltips.

use std::cell::Cell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

use gtk::cairo::Context;
use gtk::prelude::*;

use crate::model::{fmt, hour_label, usd_compact};
use crate::theme::{self, Rgb};
use crate::ui;

const AXIS_LEFT: f64 = 40.0;
const AXIS_BOTTOM: f64 = 18.0;

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

/// "Nice" tick values from 0 to at least `max` (about `count` steps).
pub fn ticks(max: f64, count: usize) -> Vec<f64> {
    if max <= 0.0 {
        return vec![0.0, 1.0];
    }
    let raw = max / count as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(raw);
    let mut out = vec![];
    let mut v = 0.0;
    while v < max + step * 0.999 {
        out.push(v);
        v += step;
    }
    out
}

/// Grid lines and $ labels; returns (y scale max, plot top, plot height).
fn y_axis(cr: &Context, w: &gtk::DrawingArea, max: f64, width: f64, top: f64, plot_h: f64) -> f64 {
    let t = ticks(max.max(0.1), 3); // at least 10¢: smaller steps would round to the same cent label
    let y_max = *t.last().unwrap();
    for v in &t {
        let y = top + plot_h - v / y_max * plot_h;
        track(cr, 0.08);
        cr.set_line_width(1.0);
        cr.move_to(AXIS_LEFT, y.round() + 0.5);
        cr.line_to(width - 4.0, y.round() + 0.5);
        let _ = cr.stroke();
        text(cr, w, &usd_compact(*v), 0.0, y - 7.0, true, AXIS_LEFT - 6.0);
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
    let day_ms = 86_400_000.0;
    let span = (end_ms - start_ms) as f64;
    {
        let hover = hover.clone();
        d.set_draw_func(move |area, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            let top = 14.0;
            let plot_w = w - AXIS_LEFT - 4.0;
            let plot_h = h - AXIS_BOTTOM - top;
            let max = points.iter().map(|p| p.value).fold(budget, f64::max) * 1.05;
            let y_max = y_axis(cr, area, max, w, top, plot_h);
            let x = |ms: i64| AXIS_LEFT + (ms - start_ms) as f64 / span * plot_w;
            let y = |v: f64| top + plot_h - v / y_max * plot_h;

            let mut d0 = start_ms;
            while d0 < end_ms {
                if x(d0) + 36.0 > w {
                    break;
                }
                text(cr, area, &fmt(d0, "%b %-d"), x(d0) - 2.0, top + plot_h + 3.0, false, 0.0);
                d0 += 7 * day_ms as i64;
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
                text(cr, area, &format!("Budget {}", usd_compact(budget)), AXIS_LEFT + 2.0, y(budget) - 15.0, false, 0.0);
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
            let w = d2.width() as f64;
            let plot_w = w - AXIS_LEFT - 4.0;
            let days = ((x - AXIS_LEFT) / plot_w * span / day_ms).round().clamp(0.0, (span / day_ms - 1.0).max(0.0));
            hover.set(Some(AXIS_LEFT + days * day_ms / span * plot_w));
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
        let plot_w = area.width() as f64 - AXIS_LEFT - 4.0;
        if (x as f64) < AXIS_LEFT {
            return false;
        }
        let days = ((x as f64 - AXIS_LEFT) / plot_w * span / day_ms).round().clamp(0.0, (span / day_ms - 1.0).max(0.0));
        tip.set_text(Some(&tooltip(start_ms + (days * day_ms) as i64)));
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
            let plot_w = w - AXIS_LEFT - 4.0;
            let plot_h = h - AXIS_BOTTOM - top;
            let max = stacks.iter().map(|s| s.parts.iter().map(|p| p.1).sum::<f64>()).fold(0.01, f64::max);
            let y_max = y_axis(cr, area, max, w, top, plot_h);
            let y = |v: f64| top + plot_h - v / y_max * plot_h;
            let slot = plot_w / stacks.len().max(1) as f64;
            let bw = if bar_width > 0.0 { bar_width.min(slot - 2.0) } else { (slot - 2.0).max(1.0) };
            for (i, s) in stacks.iter().enumerate() {
                let x = AXIS_LEFT + i as f64 * slot + (slot - bw) / 2.0;
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
        let plot_w = area.width() as f64 - AXIS_LEFT - 4.0;
        let slot = plot_w / stacks.len().max(1) as f64;
        let i = ((x as f64 - AXIS_LEFT) / slot).floor();
        if i < 0.0 || i as usize >= stacks.len() || stacks[i as usize].parts.iter().all(|p| p.1 <= 0.0) {
            return false;
        }
        tip.set_text(Some(&tooltip(&stacks[i as usize])));
        true
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
            let plot_w = w - AXIS_LEFT - 4.0;
            let plot_h = h - AXIS_BOTTOM - top;
            let max = rows.iter().map(|r| r.1.max(r.2)).fold(0.01, f64::max);
            let y_max = y_axis(cr, area, max, w, top, plot_h);
            let y = |v: f64| top + plot_h - v / y_max * plot_h;
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
        let slot = (area.width() as f64 - AXIS_LEFT - 4.0) / rows.len().max(1) as f64;
        let i = ((x as f64 - AXIS_LEFT) / slot).floor();
        if i < 0.0 || i as usize >= rows.len() {
            return false;
        }
        let (day, a, b) = rows[i as usize];
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
    let total = items.iter().map(|i| i.1).sum::<f64>().max(0.0001);
    let angles: Rc<Vec<(f64, f64)>> = Rc::new({
        let mut a = 0.0;
        items
            .iter()
            .map(|i| {
                let sweep = i.1 / total * 360.0;
                let r = (a, a + sweep);
                a += sweep;
                r
            })
            .collect()
    });
    {
        let selected = selected.clone();
        let angles = angles.clone();
        let colors: Vec<Rgb> = items.iter().map(|i| i.2).collect();
        let n = items.len();
        d.set_draw_func(move |_, cr, w, h| {
            let c = (w as f64 / 2.0, h as f64 / 2.0);
            let outer = w.min(h) as f64 / 2.0;
            let inner = outer * 0.64;
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
            let (dx, dy) = (x - size / 2.0, y - size / 2.0);
            let r = (dx * dx + dy * dy).sqrt();
            let hit = if r <= size / 2.0 && r >= size / 2.0 * 0.64 {
                let mut a = dy.atan2(dx).to_degrees() + 90.0;
                if a < 0.0 {
                    a += 360.0;
                }
                angles.iter().position(|(a0, a1)| a >= *a0 && a < *a1)
            } else {
                None
            };
            if hit != selected.get() {
                selected.set(hit);
                match hit {
                    Some(i) => {
                        title.set_label(&names[i].0);
                        value.set_label(&usd_compact(names[i].1));
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
    let label_w = 30.0;
    let grid = Rc::new(grid);
    {
        let grid = grid.clone();
        d.set_draw_func(move |area, cr, w, _| {
            let cell_w = (w as f64 - label_w) / 24.0;
            for r in 0..7 {
                text(cr, area, days[r], 0.0, r as f64 * 15.0, false, 0.0);
                for c in 0..24 {
                    let v = grid.get(r).and_then(|row| row.get(c)).copied().unwrap_or(0.0);
                    match theme::ramp(v / max) {
                        Some(col) if v > 0.0 => col.set(cr, 1.0),
                        _ => track(cr, 0.06),
                    }
                    rounded_rect(cr, label_w + c as f64 * cell_w + 1.5, r as f64 * 15.0 + 1.5, cell_w - 3.0, 12.0, 2.5);
                    let _ = cr.fill();
                }
            }
        });
    }
    d.connect_query_tooltip(move |area, x, y, _, t| {
        let cell_w = (area.width() as f64 - label_w) / 24.0;
        let (c, r) = (((x as f64 - label_w) / cell_w).floor(), (y as f64 / 15.0).floor());
        if c < 0.0 || c >= 24.0 || r < 0.0 || r >= 7.0 {
            return false;
        }
        t.set_text(Some(&tip(r as usize, c as usize)));
        true
    });
    let hours = ui::hbox(0);
    hours.set_margin_start(label_w as i32);
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
