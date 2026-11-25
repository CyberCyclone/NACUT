//! Preferences with the same five sections as the Mac (SettingsView.swift), as an AdwPreferencesWindow.
//! Pages are rebuilt on structural changes; status rows refresh whenever the core's data changes.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use nacut_core::engine::ApiState;
use nacut_core::pricing::PriceBook;
use nacut_core::merge::DataSourceMode;
use nacut_core::settings::{AccountMode, SecondFigure, UsageDisplay};

use crate::model::{self, usd, usd_compact, AppModel};
use crate::platform;
use crate::ui;

pub struct Preferences {
    pub window: adw::PreferencesWindow,
    model: Rc<AppModel>,
    pages: RefCell<Vec<adw::PreferencesPage>>,
    live: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Preferences {
    pub fn new(model: Rc<AppModel>) -> Rc<Self> {
        let window = adw::PreferencesWindow::builder().title("NACUT Preferences").default_width(880).default_height(680).search_enabled(false).hide_on_close(true).build();
        let p = Rc::new(Self { window, model: model.clone(), pages: RefCell::new(vec![]), live: RefCell::new(vec![]) });
        p.fill();
        let weak = Rc::downgrade(&p);
        model.connect_changed(move || {
            if let Some(p) = weak.upgrade() {
                for f in p.live.borrow().iter() {
                    f();
                }
            }
        });
        p
    }

    pub fn show(self: &Rc<Self>, section: &str) {
        self.fill();
        if !section.is_empty() {
            self.window.set_visible_page_name(section);
        }
        self.window.present();
    }

    /// Rebuilds every page (after a change that alters their structure), keeping the visible page.
    fn fill(self: &Rc<Self>) {
        let visible = self.window.visible_page_name();
        for page in self.pages.borrow_mut().drain(..) {
            self.window.remove(&page);
        }
        self.live.borrow_mut().clear();
        let pages = vec![self.general(), self.budget(), self.sources(), self.alerts(), self.pricing()];
        for page in &pages {
            self.window.add(page);
        }
        *self.pages.borrow_mut() = pages;
        if let Some(v) = visible {
            self.window.set_visible_page_name(&v);
        }
    }

    fn rebuild_later(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        gtk::glib::idle_add_local_once(move || {
            if let Some(p) = weak.upgrade() {
                p.fill();
            }
        });
    }

    /// A label whose text is recomputed whenever the core's data changes.
    fn live_label(&self, f: impl Fn(&AppModel) -> String + 'static) -> gtk::Label {
        let l = ui::para(&f(&self.model), &["numeric"]);
        l.set_xalign(1.0);
        l.set_max_width_chars(48);
        // Wrap only text that doesn't fit on one line.
        l.set_natural_wrap_mode(gtk::NaturalWrapMode::None);
        l.set_valign(gtk::Align::Center);
        let (l2, m) = (l.clone(), self.model.clone());
        self.live.borrow_mut().push(Box::new(move || l2.set_label(&f(&m))));
        l
    }

    fn page(name: &str, title: &str, icon: &str) -> adw::PreferencesPage {
        adw::PreferencesPage::builder().name(name).title(title).icon_name(icon).build()
    }

    /// Group titles and descriptions are markup in libadwaita, so escape them ("Usage & Cost").
    fn group(title: &str, description: Option<&str>) -> adw::PreferencesGroup {
        let g = adw::PreferencesGroup::builder().title(gtk::glib::markup_escape_text(title).as_str()).build();
        if let Some(d) = description {
            g.set_description(Some(gtk::glib::markup_escape_text(d).as_str()));
        }
        g
    }

    fn combo(title: &str, subtitle: Option<&str>, items: &[&str], selected: usize, changed: impl Fn(usize) + 'static) -> adw::ComboRow {
        let row = adw::ComboRow::builder().title(title).model(&gtk::StringList::new(items)).selected(selected as u32).build();
        if let Some(s) = subtitle {
            row.set_subtitle(gtk::glib::markup_escape_text(s).as_str());
        }
        row.connect_selected_notify(move |r| changed(r.selected() as usize));
        row
    }

    fn switch(title: &str, subtitle: Option<&str>, on: bool, changed: impl Fn(bool) + 'static) -> adw::SwitchRow {
        let row = adw::SwitchRow::builder().title(title).active(on).build();
        if let Some(s) = subtitle {
            row.set_subtitle(s);
        }
        row.connect_active_notify(move |r| changed(r.is_active()));
        row
    }

    // MARK: General

    fn general(self: &Rc<Self>) -> adw::PreferencesPage {
        let m = &self.model;
        let s = m.settings();
        let page = Self::page("general", "General", "emblem-system-symbolic");

        if m.is_subscription() {
            let g = Self::group("Usage", None);
            let displays = [UsageDisplay::Limits, UsageDisplay::Cost];
            let me = self.clone();
            g.add(&Self::combo(
                "Show usage as",
                Some(if s.usage_display == UsageDisplay::Limits {
                    "The tray icon, panel header and tooltip show your 5-hour and weekly plan limits. What the same usage would cost at API prices is still in the panel."
                } else {
                    "The tray icon, panel header and tooltip show what your usage would cost at API prices. On a subscription you don't pay this; it shows how much value you get from your plan."
                }),
                &displays.map(model::display_title),
                displays.iter().position(|d| *d == s.usage_display).unwrap_or(0),
                move |i| {
                    me.model.set_usage_display(displays[i]);
                    me.rebuild_later();
                },
            ));
            page.add(&g);
        }

        let g = Self::group("Tray", None);
        let figures = [SecondFigure::Cycle, SecondFigure::AllTime, SecondFigure::None];
        let m2 = m.clone();
        let second = Self::combo("Second figure", Some("Shown after today's cost in the tooltip and tray menu"), &figures.map(model::second_title),
            figures.iter().position(|f| *f == s.second_figure).unwrap_or(0), move |i| m2.update(|s| s.second_figure = figures[i]));
        second.set_sensitive(!m.snap().status.shows_limits);
        g.add(&second);
        let m2 = m.clone();
        g.add(&Self::switch(if m.snap().status.shows_limits { "Compact (5-hour limit only)" } else { "Compact (today only)" }, None, s.compact_menu_bar,
            move |on| m2.update(|s| s.compact_menu_bar = on)));
        let preview = adw::ActionRow::builder().title("Preview").build();
        preview.add_suffix(&self.live_label(|m| m.snap().status.menu_bar_text.clone()));
        g.add(&preview);
        page.add(&g);

        let g = Self::group("Startup", None);
        g.add(&Self::switch("Open at login", Some("Start NACUT in the tray when you sign in"), platform::autostart_enabled(), |on| {
            if let Err(e) = platform::set_autostart(on) {
                eprintln!("autostart: {e}");
            }
        }));
        page.add(&g);

        let g = Self::group("About", Some("Costs are computed from Claude Code transcripts (desktop app Code tab, VS Code, and CLI) at Anthropic list prices. Turn on plan limits or an API connection in Data Sources to cover desktop-app chats.\n\nNACUT is an independent project and is not affiliated with, endorsed by, or sponsored by Anthropic. MIT License © 2026 Casey Gibson."));
        let row = adw::ActionRow::builder().title("Messages tracked").build();
        row.add_suffix(&self.live_label(|m| m.snap().record_count.to_string()));
        g.add(&row);
        let row = adw::ActionRow::builder().title("History since").build();
        row.add_suffix(&self.live_label(|m| m.snap().first_record_ms.map(|ms| model::fmt(ms, "%b %-d, %Y")).unwrap_or("—".into())));
        g.add(&row);
        let row = adw::ActionRow::builder().title("Core").build();
        row.add_suffix(&ui::label(&format!("nacut-core {}", nacut_core_version()), &["dim-label"]));
        g.add(&row);
        page.add(&g);
        page
    }

    // MARK: Budget

    fn budget(self: &Rc<Self>) -> adw::PreferencesPage {
        let m = &self.model;
        let s = m.settings();
        let page = Self::page("budget", "Budget", "x-office-spreadsheet-symbolic");
        let g = Self::group("Account", None);
        let modes = [AccountMode::Subscription, AccountMode::Api];
        let me = self.clone();
        g.add(&Self::combo(
            "This computer's account",
            Some(if s.account_mode == AccountMode::Subscription {
                "Costs show what your usage would cost at API prices, so you can see the value you get from your plan."
            } else {
                "Costs are what you pay per token. The budget forecast tells you if you'll go over."
            }),
            &modes.map(model::account_title),
            modes.iter().position(|x| *x == s.account_mode).unwrap_or(0),
            move |i| {
                me.model.update(|s| s.account_mode = modes[i]);
                me.rebuild_later();
            },
        ));
        if s.account_mode == AccountMode::Subscription {
            let mut plans: Vec<(f64, String)> = vec![(20.0, "Pro · $20/mo".into()), (100.0, "Max 5x · $100/mo".into()), (200.0, "Max 20x · $200/mo".into())];
            if !plans.iter().any(|p| p.0 == s.plan_price) {
                plans.push((s.plan_price, format!("Custom · {}", usd(s.plan_price))));
            }
            let titles: Vec<&str> = plans.iter().map(|p| p.1.as_str()).collect();
            let prices: Vec<f64> = plans.iter().map(|p| p.0).collect();
            let m2 = m.clone();
            g.add(&Self::combo("Plan", None, &titles, plans.iter().position(|p| p.0 == s.plan_price).unwrap_or(0), move |i| m2.update(|s| s.plan_price = prices[i])));
        }
        page.add(&g);

        let g = Self::group("Budget", None);
        let budget = adw::SpinRow::builder()
            .title("Monthly budget (USD)")
            .subtitle(if s.account_mode == AccountMode::Subscription { "Set to $0 for no budget. On a subscription this is a soft target for API-equivalent value." } else { "Set to $0 for no budget." })
            .adjustment(&gtk::Adjustment::new(s.monthly_budget, 0.0, 1_000_000.0, 10.0, 100.0, 0.0))
            .digits(0)
            .build();
        let m2 = m.clone();
        budget.connect_value_notify(move |r| m2.update(|s| s.monthly_budget = r.value().max(0.0)));
        g.add(&budget);
        let day = adw::SpinRow::builder().title("Cycle starts on day").adjustment(&gtk::Adjustment::new(s.cycle_start_day as f64, 1.0, 28.0, 1.0, 7.0, 0.0)).digits(0).build();
        let m2 = m.clone();
        day.connect_value_notify(move |r| m2.update(|s| s.cycle_start_day = r.value().clamp(1.0, 28.0) as u8));
        let (day2, m3) = (day.clone(), m.clone());
        let cycle_text = move |m: &AppModel| {
            let f = &m.snap().forecast;
            format!("Use your billing or renewal day if it isn't the 1st. Current cycle: {} – {}.", model::fmt(f.cycle_start_ms, "%b %-d"), model::fmt(f.cycle_end_ms - 86_400_000, "%b %-d"))
        };
        day.set_subtitle(&cycle_text(m));
        self.live.borrow_mut().push(Box::new(move || day2.set_subtitle(&cycle_text(&m3))));
        g.add(&day);
        page.add(&g);

        let g = Self::group("Forecast right now", None);
        let headline = adw::ActionRow::builder().title(m.snap().forecast.headline.clone()).build();
        let (h2, m3) = (headline.clone(), m.clone());
        self.live.borrow_mut().push(Box::new(move || h2.set_title(&m3.snap().forecast.headline)));
        g.add(&headline);
        for (title, f) in [
            ("Spent this cycle", Box::new(|m: &AppModel| usd(m.snap().forecast.spent)) as Box<dyn Fn(&AppModel) -> String>),
            ("Projected", Box::new(|m: &AppModel| usd(m.snap().forecast.projected))),
            ("Safe daily spend", Box::new(|m: &AppModel| m.snap().forecast.safe_daily_spend.map(usd).unwrap_or("—".into()))),
        ] {
            let row = adw::ActionRow::builder().title(title).build();
            row.add_suffix(&self.live_label(f));
            g.add(&row);
        }
        page.add(&g);
        page
    }

    // MARK: Data sources

    fn sources(self: &Rc<Self>) -> adw::PreferencesPage {
        let m = &self.model;
        let s = m.settings();
        let page = Self::page("sources", "Data Sources", "drive-harddisk-symbolic");

        let g = Self::group("Subscription limits", Some("Shows your 5-hour and weekly limit usage, including desktop-app chats. Uses the Claude Code sign-in (~/.claude/.credentials.json) and the same unofficial endpoint as Claude Code's /usage, so it may stop working if Anthropic changes it. When the sign-in expires (about every 8 hours) NACUT has Claude Code refresh it by sending a one-word Haiku prompt (not saved to your history)."));
        let me = self.clone();
        g.add(&Self::switch("Show Max/Pro plan limits", None, s.limits_enabled, move |on| {
            me.model.update(|s| s.limits_enabled = on);
            me.rebuild_later();
        }));
        if s.limits_enabled {
            let row = adw::ActionRow::builder().title("Status").build();
            let l = self.live_label(|m| {
                let snap = m.snap();
                match (&snap.status.limits_error, snap.status.limits_updated_ms) {
                    (Some(e), _) => format!("⚠ {e}"),
                    (None, Some(at)) => format!("✓ Updated {}", model::relative(at)),
                    _ => "Loading…".into(),
                }
            });
            l.set_wrap(true);
            l.set_max_width_chars(48);
            row.add_suffix(&l);
            g.add(&row);
        }
        page.add(&g);

        let g = Self::group("Anthropic Usage & Cost API", Some("Optional. Pulls exact billed cost from Anthropic's Usage & Cost API. Console orgs: an Admin API key. Claude Enterprise: a key with read:analytics created by your org's primary owner. If your org doesn't allow it, the test tells you and NACUT keeps using local logs. The key is stored only in your keyring (Secret Service). API data lags 4–24 h, so Hybrid fills recent hours from local logs, scaled to match your billed rate."));
        let api = m.snap().api.clone();
        if api.has_key {
            let row = adw::ActionRow::builder().title("Connection").build();
            row.add_suffix(&self.live_label(|m| {
                let a = &m.snap().api;
                format!("{}{}", if a.usable { "✓ " } else { "⚠ " }, a.message)
            }));
            g.add(&row);
            let modes = [DataSourceMode::Logs, DataSourceMode::Hybrid, DataSourceMode::Api];
            let m2 = m.clone();
            g.add(&Self::combo("Use", None, &modes.map(model::source_title), modes.iter().position(|x| *x == s.data_source).unwrap_or(0), move |i| m2.update(|s| s.data_source = modes[i])));

            let mut options: Vec<(Option<String>, String)> = vec![(None, "Whole organization".into())];
            options.extend(api.actors.iter().map(|a| (Some(a.id.clone()), format!("{} · {} last 30d", a.display, usd_compact(a.cost)))));
            if let Some(id) = &s.api_actor_id {
                if !api.actors.iter().any(|a| &a.id == id) {
                    options.push((Some(id.clone()), s.api_actor_name.clone().unwrap_or(id.clone())));
                }
            }
            let titles: Vec<&str> = options.iter().map(|o| o.1.as_str()).collect();
            let selected = options.iter().position(|o| o.0 == s.api_actor_id).unwrap_or(0);
            let (m2, actors) = (m.clone(), api.actors.clone());
            let ids: Vec<Option<String>> = options.iter().map(|o| o.0.clone()).collect();
            g.add(&Self::combo("Count usage from", Some("On a shared work org, pick yourself so the budget tracks only your usage."), &titles, selected, move |i| {
                m2.engine.set_actor(ids[i].as_ref().and_then(|id| actors.iter().find(|a| &a.id == id).cloned()));
                m2.reload();
            }));

            let buttons = ui::hbox(8);
            buttons.set_valign(gtk::Align::Center);
            let test = gtk::Button::with_label("Test connection");
            let m2 = m.clone();
            test.connect_clicked(move |_| m2.background(|e| e.test_api(), |_| {}));
            let refresh = gtk::Button::with_label("Refresh now");
            let m2 = m.clone();
            refresh.connect_clicked(move |_| m2.engine.refresh_api(true));
            let remove = gtk::Button::with_label("Remove key");
            remove.add_css_class("destructive-action");
            let me = self.clone();
            remove.connect_clicked(move |_| {
                me.model.engine.remove_api_key();
                me.model.reload();
                me.rebuild_later();
            });
            buttons.append(&test);
            buttons.append(&refresh);
            buttons.append(&remove);
            let row = adw::ActionRow::builder().title("Actions").build();
            row.add_suffix(&buttons);
            g.add(&row);
            if api.actors.is_empty() {
                m.background(|e| e.load_actors(), |_| {});
            }
        } else {
            let key = adw::PasswordEntryRow::builder().title("API key (sk-ant-admin… or read:analytics)").show_apply_button(true).build();
            let result = adw::ActionRow::builder().build();
            result.set_visible(api.state != ApiState::NotConfigured);
            result.set_title(&format!("⚠ {}", api.message));
            let me = self.clone();
            let r2 = result.clone();
            key.connect_apply(move |row| {
                let k = row.text().trim().to_string();
                if k.is_empty() {
                    return;
                }
                row.set_sensitive(false);
                let (me2, r3, row2) = (me.clone(), r2.clone(), row.clone());
                me.model.background(
                    move |e| {
                        let s = e.save_api_key(&k);
                        (s.is_usable(), s.message(&nacut_core::store::Cal::system()))
                    },
                    move |(usable, message)| {
                        row2.set_sensitive(true);
                        row2.set_text("");
                        if usable {
                            me2.rebuild_later();
                        } else {
                            r3.set_title(&format!("⚠ {message}"));
                            r3.set_visible(true);
                        }
                    },
                );
            });
            g.add(&key);
            g.add(&result);
        }
        page.add(&g);

        let g = Self::group("Local transcript folders", Some("History is cached, so totals survive Claude Code's automatic transcript cleanup."));
        for path in nacut_core::scanner::LogScanner::default_roots() {
            let exists = path.exists();
            let row = adw::ActionRow::builder().title(path.to_string_lossy()).subtitle(if exists { "Default" } else { "Default · not found" }).build();
            row.add_prefix(&gtk::Image::from_icon_name(if exists { "folder-symbolic" } else { "dialog-question-symbolic" }));
            g.add(&row);
        }
        for path in s.extra_roots.clone() {
            let row = adw::ActionRow::builder().title(&path).subtitle("Added folder").build();
            row.add_prefix(&gtk::Image::from_icon_name("folder-symbolic"));
            let me = self.clone();
            let p2 = path.clone();
            let b = ui::icon_button("list-remove-symbolic", "Remove", move || {
                me.model.update(|s| s.extra_roots.retain(|r| *r != p2));
                me.rebuild_later();
            });
            b.set_valign(gtk::Align::Center);
            row.add_suffix(&b);
            g.add(&row);
        }
        let add = adw::EntryRow::builder().title("Add folder (e.g. ~/other-claude/projects)").show_apply_button(true).build();
        let me = self.clone();
        add.connect_apply(move |row| {
            let v = row.text().trim().to_string();
            if !v.is_empty() {
                me.model.update(|s| s.extra_roots.push(v.clone()));
                me.rebuild_later();
            }
        });
        g.add(&add);
        let cache = adw::ActionRow::builder().title(format!("{} messages cached", m.snap().record_count)).build();
        let (c2, m3) = (cache.clone(), m.clone());
        self.live.borrow_mut().push(Box::new(move || c2.set_title(&format!("{} messages cached", m3.snap().record_count))));
        let rebuild = gtk::Button::with_label("Rebuild cache");
        rebuild.set_valign(gtk::Align::Center);
        let m2 = m.clone();
        rebuild.connect_clicked(move |_| m2.engine.rebuild_cache());
        cache.add_suffix(&rebuild);
        g.add(&cache);
        page.add(&g);
        page
    }

    // MARK: Alerts

    fn alerts(self: &Rc<Self>) -> adw::PreferencesPage {
        let m = &self.model;
        let s = m.settings();
        let page = Self::page("alerts", "Alerts", "preferences-system-notifications-symbolic");
        let g = Self::group("Notifications", Some("Each alert fires once per billing cycle (or limit window)."));
        let dependents: Rc<RefCell<Vec<adw::SwitchRow>>> = Rc::new(RefCell::new(vec![]));
        let (m2, deps) = (m.clone(), dependents.clone());
        g.add(&Self::switch("Budget notifications", None, s.alerts_enabled, move |on| {
            m2.update(|s| s.alerts_enabled = on);
            for d in deps.borrow().iter() {
                d.set_sensitive(on);
            }
        }));
        for t in [50u32, 80, 90, 100] {
            let m2 = m.clone();
            let row = Self::switch(&format!("At {t}% of budget"), None, s.alert_thresholds.contains(&t), move |on| {
                m2.update(|s| {
                    s.alert_thresholds.retain(|x| *x != t);
                    if on {
                        s.alert_thresholds.push(t);
                    }
                })
            });
            row.set_sensitive(s.alerts_enabled);
            dependents.borrow_mut().push(row.clone());
            g.add(&row);
        }
        let m2 = m.clone();
        let fr = Self::switch("When the forecast says I'll go over", None, s.forecast_alert, move |on| m2.update(|s| s.forecast_alert = on));
        let m2 = m.clone();
        let lr = Self::switch("When a plan limit passes 90%", None, s.limit_alerts, move |on| m2.update(|s| s.limit_alerts = on));
        for r in [&fr, &lr] {
            r.set_sensitive(s.alerts_enabled);
            dependents.borrow_mut().push(r.clone());
            g.add(r);
        }
        page.add(&g);
        let g = Self::group("Test", None);
        let row = adw::ActionRow::builder().title("Send a test notification").build();
        let b = gtk::Button::with_label("Send");
        b.set_valign(gtk::Align::Center);
        let m2 = m.clone();
        b.connect_clicked(move |_| m2.engine.send_test_alert());
        row.add_suffix(&b);
        g.add(&row);
        page.add(&g);
        page
    }

    // MARK: Pricing

    fn pricing(self: &Rc<Self>) -> adw::PreferencesPage {
        let m = &self.model;
        let overrides = m.settings().price_overrides;
        let table = &PriceBook::bundled().table;
        let page = Self::page("pricing", "Pricing", "accessories-calculator-symbolic");
        let g = Self::group(
            "USD per million tokens",
            Some(&format!(
                "List prices as of {}. Fast mode is ×{:.0}; web search ${:.0} per 1,000. Haiku 5.5 uses higher rates above 100K prompt tokens. Expand a model to set a custom price (e.g. a negotiated rate).",
                table.updated, table.fast_multiplier, table.web_search_per_thousand
            )),
        );
        let fmt = |v: f64| if v < 1.0 { format!("${}", trim(v, 3)) } else { format!("${}", trim(v, 2)) };
        for mp in &table.models {
            let r = overrides.get(&mp.id).cloned().unwrap_or_else(|| mp.rates());
            let row = adw::ExpanderRow::builder()
                .title(&mp.name)
                .subtitle(format!("In {} · Out {} · Write {} / {} · Read {}", fmt(r.input), fmt(r.output), fmt(r.write5m), fmt(r.write1h), fmt(r.read)))
                .build();
            if overrides.contains_key(&mp.id) {
                row.add_prefix(&gtk::Image::from_icon_name("document-edit-symbolic"));
            }
            let id = mp.id.clone();
            let base = mp.rates();
            let fields: Vec<(&str, f64)> = vec![("Input", r.input), ("Output", r.output), ("Cache write (5m)", r.write5m), ("Cache write (1h)", r.write1h), ("Cache read", r.read)];
            let spins: Vec<adw::SpinRow> = fields
                .iter()
                .map(|(t, v)| adw::SpinRow::builder().title(*t).adjustment(&gtk::Adjustment::new(*v, 0.0, 10_000.0, 0.1, 1.0, 0.0)).digits(3).build())
                .collect();
            for s in &spins {
                row.add_row(s);
            }
            let actions = adw::ActionRow::new();
            let save = gtk::Button::with_label("Save");
            save.add_css_class("suggested-action");
            let reset = gtk::Button::with_label("Use list price");
            for b in [&reset, &save] {
                b.set_valign(gtk::Align::Center);
                actions.add_suffix(b);
            }
            let (me, id2, spins2) = (self.clone(), id.clone(), spins.clone());
            save.connect_clicked(move |_| {
                let v: Vec<f64> = spins2.iter().map(|s| s.value()).collect();
                let rates = nacut_core::pricing::Rates { input: v[0], output: v[1], write5m: v[2], write1h: v[3], read: v[4] };
                me.model.update(|s| {
                    s.price_overrides.insert(id2.clone(), rates);
                });
                me.rebuild_later();
            });
            let me = self.clone();
            reset.connect_clicked(move |_| {
                me.model.update(|s| {
                    s.price_overrides.remove(&id);
                });
                me.rebuild_later();
            });
            let _ = base;
            row.add_row(&actions);
            g.add(&row);
        }
        page.add(&g);
        if !overrides.is_empty() {
            let g = Self::group("", None);
            let row = adw::ActionRow::builder().title("Reset all custom prices").build();
            let b = gtk::Button::with_label("Reset");
            b.set_valign(gtk::Align::Center);
            let me = self.clone();
            b.connect_clicked(move |_| {
                me.model.update(|s| s.price_overrides.clear());
                me.rebuild_later();
            });
            row.add_suffix(&b);
            g.add(&row);
            page.add(&g);
        }
        page
    }
}

fn trim(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

fn nacut_core_version() -> &'static str {
    nacut_core::VERSION
}
