//! NACUT for Linux: a GTK4/libadwaita tray app on the shared Rust core.
//!
//! `nacut-gtk`                 opens the panel (or, if already running, asks that instance to)
//! `nacut-gtk --background`    starts in the tray only (used by Open at login)
//! `nacut-gtk --render-to DIR [--limits-json FILE]`   saves screenshots and exits

mod charts;
mod model;
mod platform;
mod prefs;
mod render;
mod theme;
mod tray;
mod ui;
mod views;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use model::{AppModel, CoreEvent};
use platform::APP_ID;
use tray::{Tray, TrayAction};
use views::panel::Panel;
use views::settings::Preferences;

struct State {
    panel: Rc<Panel>,
    _prefs: Rc<Preferences>,
    _tray: Option<Rc<Tray>>,
    _hold: gio::ApplicationHoldGuard,
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let render_to = args.iter().position(|a| a == "--render-to").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    let background = args.iter().any(|a| a == "--background");

    let flags = if render_to.is_some() { gio::ApplicationFlags::NON_UNIQUE } else { gio::ApplicationFlags::empty() };
    let app = adw::Application::builder().application_id(APP_ID).flags(flags).build();
    let state: Rc<RefCell<Option<State>>> = Rc::new(RefCell::new(None));
    let first_activation = Rc::new(Cell::new(true));

    {
        let state = state.clone();
        app.connect_startup(move |app| {
            gtk::Window::set_default_icon_name(APP_ID);
            let css = gtk::CssProvider::new();
            css.load_from_string(&theme::css());
            gtk::style_context_add_provider_for_display(&gtk::gdk::Display::default().expect("display"), &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

            let render_mode = render_to.is_some();
            if render_mode {
                prefs::set_read_only();
            }
            let (model, events) = AppModel::new(render_mode);

            let prefs = Preferences::new(model.clone());
            let open_settings: Rc<dyn Fn(&str)> = {
                let prefs = prefs.clone();
                Rc::new(move |section: &str| prefs.show(section))
            };
            let quit: Rc<dyn Fn()> = {
                let (app, model) = (app.clone(), model.clone());
                Rc::new(move || {
                    model.engine.stop();
                    app.quit();
                })
            };
            let panel = Panel::new(app, model.clone(), open_settings.clone(), quit.clone(), render_mode);

            // The panel's CSS and charts follow the system light/dark setting.
            {
                let panel = panel.clone();
                adw::StyleManager::default().connect_dark_notify(move |_| {
                    css.load_from_string(&theme::css());
                    if panel.window.is_visible() {
                        panel.refresh();
                    }
                });
            }

            // Notifications: clicking one opens the panel.
            let open = gio::SimpleAction::new("open", None);
            {
                let panel = panel.clone();
                open.connect_activate(move |_, _| panel.show());
            }
            app.add_action(&open);

            // Engine callbacks arrive on background threads; handle them on the main loop.
            {
                let (model, app) = (model.clone(), app.clone());
                glib::spawn_future_local(async move {
                    while let Ok(event) = events.recv().await {
                        match event {
                            CoreEvent::Changed => model.schedule_reload(),
                            CoreEvent::Alert(alert) => {
                                let n = gio::Notification::new(&alert.title);
                                n.set_body(Some(&alert.body));
                                n.set_default_action("app.open");
                                app.send_notification(Some(&alert.id), &n);
                            }
                        }
                    }
                });
            }

            let tray = if render_mode {
                None
            } else {
                let (tx, rx) = async_channel::unbounded();
                let tray = Rc::new(Tray::spawn(tx, &model.snap().status));
                {
                    let (tray, model2) = (tray.clone(), model.clone());
                    model.connect_changed(move || tray.update(&model2.snap().status));
                }
                {
                    let (panel, model, open_settings, quit, tray) = (panel.clone(), model.clone(), open_settings.clone(), quit.clone(), tray.clone());
                    glib::spawn_future_local(async move {
                        while let Ok(action) = rx.recv().await {
                            match action {
                                TrayAction::Toggle => panel.toggle(),
                                TrayAction::Open => panel.show(),
                                TrayAction::Refresh => model.refresh_all(),
                                TrayAction::Preferences => open_settings(""),
                                TrayAction::Quit => {
                                    tray.shutdown();
                                    quit();
                                }
                            }
                        }
                    });
                }
                Some(tray)
            };

            if let Some(dir) = render_to.clone() {
                render::run(app.clone(), model.clone(), panel.clone(), prefs.clone(), dir);
            }
            *state.borrow_mut() = Some(State { panel, _prefs: prefs, _tray: tray, _hold: app.hold() });
        });
    }

    {
        let state = state.clone();
        app.connect_activate(move |_| {
            // The first activation is this launch; skip it for --background and screenshots.
            let first = first_activation.replace(false);
            if first && (background || std::env::args().any(|a| a == "--render-to")) {
                return;
            }
            if let Some(s) = state.borrow().as_ref() {
                s.panel.show();
            }
        });
    }

    // Our flags are parsed above; GApplication only sees the program name.
    app.run_with_args(&args[..1])
}
