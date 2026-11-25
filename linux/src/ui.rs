//! Small builders for the panel's views, mirroring the Mac app's SwiftUI components (Card, StatTile,
//! pills). Views are rebuilt from the latest snapshot rather than bound to data.

use gtk::prelude::*;
use gtk::{Align, Orientation};

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::builder().label(text).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
    for c in classes {
        l.add_css_class(c);
    }
    l
}

/// A label that wraps instead of ellipsizing.
pub fn para(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::builder().label(text).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).build();
    for c in classes {
        l.add_css_class(c);
    }
    l
}

pub fn dim(text: &str) -> gtk::Label {
    label(text, &["dim-label", "caption"])
}

/// A large rounded figure, sized in points.
pub fn figure(text: &str, size: u32) -> gtk::Label {
    let l = label(text, &["figure"]);
    l.set_attributes(Some(&size_attrs(size)));
    l
}

pub fn size_attrs(points: u32) -> gtk::pango::AttrList {
    let attrs = gtk::pango::AttrList::new();
    attrs.insert(gtk::pango::AttrSize::new(points as i32 * gtk::pango::SCALE));
    attrs
}

/// One line of runs: dim text and semibold values, e.g. `[("Today ", false), ("$4.21", true)]`.
pub fn runs(parts: &[(&str, bool)]) -> gtk::Label {
    let mut markup = String::new();
    for (text, strong) in parts {
        let t = gtk::glib::markup_escape_text(text);
        if *strong {
            markup.push_str(&format!("<b>{t}</b>"));
        } else {
            markup.push_str(&format!("<span alpha=\"62%\">{t}</span>"));
        }
    }
    let l = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
    l.set_markup(&markup);
    l.add_css_class("numeric");
    l
}

pub fn icon(name: &str, classes: &[&str]) -> gtk::Image {
    let i = gtk::Image::from_icon_name(name);
    for c in classes {
        i.add_css_class(c);
    }
    i
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(Orientation::Vertical, spacing)
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(Orientation::Horizontal, spacing)
}

pub trait BoxAdd {
    fn add(&self, w: &impl IsA<gtk::Widget>) -> &Self;
}

impl BoxAdd for gtk::Box {
    fn add(&self, w: &impl IsA<gtk::Widget>) -> &Self {
        self.append(w);
        self
    }
}

/// `left` expands; `right` hugs the trailing edge.
pub fn spread(left: &impl IsA<gtk::Widget>, right: Option<&gtk::Widget>) -> gtk::Box {
    let b = hbox(8);
    left.set_hexpand(true);
    b.append(left);
    if let Some(r) = right {
        r.set_halign(Align::End);
        b.append(r);
    }
    b
}

/// Equal-width columns, wrapping into rows.
pub fn columns(count: i32, spacing: i32, items: Vec<gtk::Widget>) -> gtk::Grid {
    let g = gtk::Grid::builder().column_spacing(spacing).row_spacing(spacing).column_homogeneous(true).build();
    for (i, item) in items.iter().enumerate() {
        item.set_hexpand(true);
        g.attach(item, i as i32 % count, i as i32 / count, 1, 1);
    }
    g
}

/// Rounded card with an optional title row (libadwaita's `card` style).
pub fn card(title: Option<&str>, trailing: Option<&gtk::Widget>, content: &[&gtk::Widget]) -> gtk::Box {
    let b = vbox(10);
    b.add_css_class("card");
    b.set_margin_bottom(0);
    let inner = vbox(10);
    inner.set_margin_top(12);
    inner.set_margin_bottom(12);
    inner.set_margin_start(12);
    inner.set_margin_end(12);
    if title.is_some() || trailing.is_some() {
        inner.append(&spread(&label(title.unwrap_or(""), &["heading"]), trailing));
    }
    for c in content {
        inner.append(*c);
    }
    b.append(&inner);
    b
}

/// A subtly filled box (no shadow), for limit rings and month summaries.
pub fn subtle(child: &impl IsA<gtk::Widget>, padding: i32) -> gtk::Box {
    let b = vbox(0);
    b.add_css_class("subtle-box");
    child.set_margin_top(padding);
    child.set_margin_bottom(padding);
    child.set_margin_start(padding);
    child.set_margin_end(padding);
    b.append(child);
    b
}

/// Capsule tinted with a status color class (`good`, `tight`, `over`, `exceeded`).
pub fn pill(child: &impl IsA<gtk::Widget>, status_class: &str) -> gtk::Box {
    let b = hbox(0);
    b.add_css_class("pill");
    b.add_css_class(status_class);
    child.set_hexpand(true);
    b.append(child);
    b
}

pub fn stat_tile(title: &str, value: &str, detail: Option<&str>, icon_name: &str, tint_class: &str, tip: Option<&str>) -> gtk::Widget {
    let head = hbox(5);
    head.append(&icon(icon_name, &[tint_class]));
    head.append(&dim(title));
    let v = vbox(4);
    v.append(&head);
    v.append(&figure(value, 15));
    if let Some(d) = detail {
        v.append(&dim(d));
    }
    let c = card(None, None, &[v.upcast_ref()]);
    if let Some(t) = tip {
        c.set_tooltip_text(Some(t));
    }
    c.upcast()
}

pub fn empty(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("dim-label");
    l.set_margin_top(20);
    l.set_margin_bottom(20);
    l
}

pub fn icon_button(icon_name: &str, tip: &str, f: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon_name);
    b.add_css_class("flat");
    b.set_tooltip_text(Some(tip));
    b.connect_clicked(move |_| f());
    b
}

pub fn link_button(text: &str, f: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("flat");
    b.add_css_class("caption");
    b.connect_clicked(move |_| f());
    b
}

/// Linked toggle buttons acting as a segmented control.
pub fn segmented(labels: &[&str], selected: usize, on_change: impl Fn(usize) + 'static) -> gtk::Box {
    let b = hbox(0);
    b.add_css_class("linked");
    b.set_homogeneous(true);
    let on_change = std::rc::Rc::new(on_change);
    let mut first: Option<gtk::ToggleButton> = None;
    for (i, text) in labels.iter().enumerate() {
        let t = gtk::ToggleButton::with_label(text);
        if let Some(f) = &first {
            t.set_group(Some(f));
        } else {
            first = Some(t.clone());
        }
        t.set_active(i == selected);
        let cb = on_change.clone();
        t.connect_toggled(move |t| {
            if t.is_active() {
                cb(i);
            }
        });
        b.append(&t);
    }
    b
}

/// A color swatch (legends).
pub fn swatch(color: crate::theme::Rgb, size: i32) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::builder().content_width(size).content_height(size).valign(Align::Center).build();
    d.set_draw_func(move |_, cr, w, h| {
        color.set(cr, 1.0);
        crate::charts::rounded_rect(cr, 0.0, 0.0, w as f64, h as f64, 2.0);
        let _ = cr.fill();
    });
    d
}

pub fn dot(color: crate::theme::Rgb, size: i32) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::builder().content_width(size).content_height(size).valign(Align::Center).build();
    d.set_draw_func(move |_, cr, w, h| {
        color.set(cr, 1.0);
        cr.arc(w as f64 / 2.0, h as f64 / 2.0, w.min(h) as f64 / 2.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    });
    d
}

pub fn separator() -> gtk::Separator {
    gtk::Separator::new(Orientation::Horizontal)
}

/// Wrapping flow of small widgets (legends, model chips).
pub fn flow(items: Vec<gtk::Widget>, spacing: u32) -> gtk::FlowBox {
    let f = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(spacing)
        .row_spacing(2)
        .max_children_per_line(12)
        .homogeneous(false)
        .build();
    for i in items {
        f.insert(&i, -1);
    }
    f
}
