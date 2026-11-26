//! NACUT for Linux: a GTK4/libadwaita tray app on the shared Rust core.
//!
//! `nacut-gtk`                 opens the panel (or, if already running, asks that instance to)
//! `nacut-gtk --background`    starts in the tray only (used by Open at login)
//! `nacut-gtk --render-to DIR [--limits-json FILE]`   saves screenshots and exits (a missing value is an error)

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
use std::path::PathBuf;
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

/// Command-line options (see the module docs).
#[derive(Debug, Default, PartialEq)]
struct Args {
    background: bool,
    render_to: Option<PathBuf>,
    limits_json: Option<PathBuf>,
}

/// Parses the arguments after the program name. Unknown arguments are ignored; `--render-to` and
/// `--limits-json` need a value.
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().filter(|v| !v.is_empty() && !v.starts_with("--")).map(PathBuf::from).ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--background" => out.background = true,
            "--render-to" => out.render_to = Some(value()?),
            "--limits-json" => out.limits_json = Some(value()?),
            _ => {}
        }
    }
    Ok(out)
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Args { background, render_to, limits_json } = match parse_args(args.get(1..).unwrap_or_default()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("nacut-gtk: {e}");
            return glib::ExitCode::FAILURE;
        }
    };
    let skip_first_activation = background || render_to.is_some();

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
                render::run(app.clone(), model.clone(), panel.clone(), prefs.clone(), dir, limits_json.clone());
            }
            *state.borrow_mut() = Some(State { panel, _prefs: prefs, _tray: tray, _hold: app.hold() });
        });
    }

    {
        let state = state.clone();
        app.connect_activate(move |_| {
            // The first activation is this launch; skip it for --background and screenshots.
            let first = first_activation.replace(false);
            if first && skip_first_activation {
                return;
            }
            if let Some(s) = state.borrow().as_ref() {
                s.panel.show();
            }
        });
    }

    // Our flags are parsed above; GApplication only sees the program name.
    app.run_with_args(&args[..args.len().min(1)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn no_arguments_opens_the_panel() {
        assert_eq!(parse(&[]), Ok(Args::default()));
    }

    #[test]
    fn background_flag() {
        assert_eq!(parse(&["--background"]), Ok(Args { background: true, ..Default::default() }));
    }

    #[test]
    fn render_to_with_limits() {
        let a = parse(&["--render-to", "/tmp/shots", "--limits-json", "fixtures/limits.json"]).unwrap();
        assert_eq!(a.render_to, Some(PathBuf::from("/tmp/shots")));
        assert_eq!(a.limits_json, Some(PathBuf::from("fixtures/limits.json")));
        assert!(!a.background);
    }

    #[test]
    fn options_in_any_order() {
        let a = parse(&["--limits-json", "l.json", "--background", "--render-to", "out"]).unwrap();
        assert_eq!(a, Args { background: true, render_to: Some("out".into()), limits_json: Some("l.json".into()) });
    }

    #[test]
    fn render_to_without_a_value_is_an_error() {
        // Used to leave render_to unset while still skipping the first activation, like --background.
        assert_eq!(parse(&["--render-to"]), Err("--render-to needs a value".into()));
        assert_eq!(parse(&["--render-to", ""]), Err("--render-to needs a value".into()));
        assert_eq!(parse(&["--render-to", "--background"]), Err("--render-to needs a value".into()));
        assert_eq!(parse(&["--render-to", "--limits-json", "l.json"]), Err("--render-to needs a value".into()));
    }

    #[test]
    fn limits_json_without_a_value_is_an_error() {
        assert_eq!(parse(&["--render-to", "out", "--limits-json"]), Err("--limits-json needs a value".into()));
    }

    #[test]
    fn unknown_arguments_are_ignored() {
        assert_eq!(parse(&["--verbose", "file.txt", "-x"]), Ok(Args::default()));
    }

    #[test]
    fn last_value_wins() {
        assert_eq!(parse(&["--render-to", "a", "--render-to", "b"]).unwrap().render_to, Some(PathBuf::from("b")));
    }

    #[test]
    fn values_with_spaces_and_dashes_inside() {
        assert_eq!(parse(&["--render-to", "/tmp/my shots-1"]).unwrap().render_to, Some(PathBuf::from("/tmp/my shots-1")));
        assert_eq!(parse(&["--render-to", "-dir"]).unwrap().render_to, Some(PathBuf::from("-dir")));
    }
}
