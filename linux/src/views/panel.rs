//! The panel window opened from the tray (PanelView.swift). GNOME on Wayland doesn't let apps position
//! windows, so it opens where the compositor puts it; it hides when it loses focus or on Escape.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use nacut_core::forecast::BudgetStatus;
use nacut_core::merge::DataSourceMode;

use crate::model::AppModel;
use crate::prefs;
use crate::theme;
use crate::ui::{self, BoxAdd};

pub const PANEL_WIDTH: i32 = 452;
const PANEL_HEIGHT: i32 = 780;
const TABS: [&str; 4] = ["Overview", "Calendar", "Models", "Projects"];

pub struct Panel {
    pub window: adw::ApplicationWindow,
    model: Rc<AppModel>,
    header: adw::Bin,
    tab_host: adw::Bin,
    scroll: gtk::ScrolledWindow,
    status: gtk::Label,
    dot: gtk::DrawingArea,
    busy: Rc<Cell<bool>>,
    loading: gtk::Box,
    tab: Cell<usize>,
    tab_buttons: std::cell::RefCell<Option<gtk::Box>>,
    had_focus: Cell<bool>,
    open_settings: Rc<dyn Fn(&str)>,
    pub render_mode: bool,
}

impl Panel {
    pub fn new(app: &adw::Application, model: Rc<AppModel>, open_settings: Rc<dyn Fn(&str)>, quit: Rc<dyn Fn()>, render_mode: bool) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("NACUT")
            .default_width(PANEL_WIDTH)
            .default_height(PANEL_HEIGHT)
            .resizable(false)
            .hide_on_close(true)
            .build();
        let tab = TABS.iter().position(|t| *t == prefs::get("panelTab", "Overview")).unwrap_or(0);

        let busy = Rc::new(Cell::new(false));
        let dot = gtk::DrawingArea::builder().content_width(6).content_height(6).valign(gtk::Align::Center).build();
        {
            let busy = busy.clone();
            dot.set_draw_func(move |_, cr, w, h| {
                let c = if busy.get() { theme::coral() } else { theme::status(BudgetStatus::UnderBudget) };
                c.set(cr, 1.0);
                cr.arc(w as f64 / 2.0, h as f64 / 2.0, 3.0, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            });
        }
        let status = ui::dim("Loading…");
        let footer = ui::hbox(2);
        footer.add_css_class("panel-footer");
        footer.set_margin_start(12);
        footer.set_margin_end(8);
        footer.set_margin_top(6);
        footer.set_margin_bottom(6);
        let left = ui::hbox(6);
        left.add(&dot).add(&status);
        left.set_hexpand(true);
        footer.add(&left);
        {
            let m = model.clone();
            footer.add(&ui::icon_button("view-refresh-symbolic", "Refresh now", move || m.refresh_all()));
        }
        {
            let open = open_settings.clone();
            footer.add(&ui::icon_button("emblem-system-symbolic", "Preferences", move || open("")));
        }
        footer.add(&ui::icon_button("system-shutdown-symbolic", "Quit NACUT", move || quit()));

        let header = adw::Bin::new();
        header.set_margin_top(16);
        header.set_margin_start(16);
        header.set_margin_end(16);
        header.set_margin_bottom(12);
        let tab_host = adw::Bin::new();
        tab_host.set_margin_start(16);
        tab_host.set_margin_end(16);
        tab_host.set_margin_bottom(14);
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&tab_host).build();

        let loading = ui::vbox(10);
        loading.set_halign(gtk::Align::Center);
        loading.set_valign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.start();
        spinner.set_size_request(28, 28);
        loading.add(&spinner).add(&ui::dim("Reading your Claude Code history…"));

        let panel = Rc::new(Self {
            window: window.clone(),
            model: model.clone(),
            header: header.clone(),
            tab_host,
            scroll: scroll.clone(),
            status,
            dot,
            busy,
            loading: loading.clone(),
            tab: Cell::new(tab),
            tab_buttons: Default::default(),
            had_focus: Cell::new(false),
            open_settings,
            render_mode,
        });

        let weak = Rc::downgrade(&panel);
        let tabs = ui::segmented(&TABS, tab, move |i| {
            if let Some(p) = weak.upgrade() {
                p.tab.set(i);
                prefs::set("panelTab", TABS[i]);
                p.rebuild_tab(true);
            }
        });
        tabs.set_margin_start(16);
        tabs.set_margin_end(16);
        tabs.set_margin_bottom(10);
        *panel.tab_buttons.borrow_mut() = Some(tabs.clone());

        let content = ui::vbox(0);
        content.add(&header).add(&tabs).add(&scroll).add(&ui::separator()).add(&footer);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&content));
        overlay.add_overlay(&loading);
        window.set_content(Some(&overlay));

        // Escape hides the panel.
        let keys = gtk::EventControllerKey::new();
        {
            let w = window.clone();
            keys.connect_key_pressed(move |_, key, _, _| {
                if key == gtk::gdk::Key::Escape {
                    w.set_visible(false);
                    return gtk::glib::Propagation::Stop;
                }
                gtk::glib::Propagation::Proceed
            });
        }
        window.add_controller(keys);

        // Hide on focus loss, like a menu bar panel (only once it really had focus).
        let weak = Rc::downgrade(&panel);
        window.connect_is_active_notify(move |w| {
            let Some(p) = weak.upgrade() else { return };
            if w.is_active() {
                p.had_focus.set(true);
            } else if p.had_focus.get() && !p.render_mode {
                w.set_visible(false);
            }
        });

        let weak = Rc::downgrade(&panel);
        model.connect_changed(move || {
            if let Some(p) = weak.upgrade() {
                if p.window.is_visible() {
                    p.refresh();
                }
            }
        });
        let weak = Rc::downgrade(&panel);
        gtk::glib::timeout_add_seconds_local(10, move || match weak.upgrade() {
            Some(p) => {
                p.update_footer();
                gtk::glib::ControlFlow::Continue
            }
            None => gtk::glib::ControlFlow::Break,
        });
        panel
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.window.set_visible(false);
        } else {
            self.show();
        }
    }

    pub fn show(self: &Rc<Self>) {
        self.had_focus.set(false);
        self.refresh();
        self.window.present();
    }

    /// Switches tab as if its button were clicked (keeps the segmented buttons in sync).
    pub fn select_tab(self: &Rc<Self>, i: usize) {
        let button = self.tab_buttons.borrow().as_ref().and_then(|b| {
            let mut child = b.first_child();
            for _ in 0..i {
                child = child.and_then(|c| c.next_sibling());
            }
            child.and_then(|c| c.downcast::<gtk::ToggleButton>().ok())
        });
        match button {
            Some(b) if !b.is_active() => b.set_active(true),
            _ => {
                self.tab.set(i);
                self.rebuild_tab(true);
            }
        }
    }

    pub fn refresh(self: &Rc<Self>) {
        self.header.set_child(Some(&super::header::build(&self.model, &self.open_settings)));
        self.rebuild_tab(false);
        let snap = self.model.snap();
        self.loading.set_visible(!snap.status.has_loaded && snap.record_count == 0);
        drop(snap);
        self.update_footer();
    }

    fn rebuild_tab(self: &Rc<Self>, reset_scroll: bool) {
        let adj = self.scroll.vadjustment();
        let offset = adj.value();
        let weak = Rc::downgrade(self);
        let rebuild: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(p) = weak.upgrade() {
                p.rebuild_tab(false);
            }
        });
        let child = match self.tab.get() {
            1 => super::calendar::build(&self.model, rebuild),
            2 => super::models::build(&self.model, rebuild),
            3 => super::projects::build(&self.model, rebuild),
            _ => super::overview::build(&self.model),
        };
        self.tab_host.set_child(Some(&child));
        if reset_scroll {
            adj.set_value(0.0);
        } else {
            gtk::glib::idle_add_local_once(move || adj.set_value(offset));
        }
    }

    pub fn tab_content(&self) -> Option<gtk::Widget> {
        self.tab_host.child()
    }

    fn update_footer(&self) {
        let snap = self.model.snap();
        let st = &snap.status;
        self.busy.set(st.is_scanning || snap.api.is_fetching);
        self.dot.queue_draw();
        let updated = crate::model::updated_text(jiff::Timestamp::now().as_millisecond(), st.last_updated_ms);
        let source = match snap.settings.data_source {
            DataSourceMode::Logs => "local logs",
            DataSourceMode::Api => "Anthropic API",
            DataSourceMode::Hybrid => "API + logs",
        };
        self.status.set_label(&format!("{updated} · {source}"));
    }
}
