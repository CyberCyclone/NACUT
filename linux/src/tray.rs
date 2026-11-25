//! The tray icon, a StatusNotifierItem over D-Bus (shown by Ubuntu's AppIndicator extension, KDE and
//! Waybar). Like Windows, the icon draws the figure the core picks (worst plan limit, or budget used) as a
//! number over a small gauge bar, or "$" in cost mode; the tooltip and menu carry the full text.

use gtk::cairo;
use ksni::blocking::TrayMethods;
use nacut_core::views::{Level, StatusView};

/// Requests from the tray thread to the GTK main loop.
pub enum TrayAction {
    Toggle,
    Open,
    Refresh,
    Preferences,
    Quit,
}

pub struct NacutTray {
    tx: async_channel::Sender<TrayAction>,
    title: String,
    tooltip: String,
    icons: Vec<ksni::Icon>,
}

impl ksni::Tray for NacutTray {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "io.github.cybercyclone.nacut".into()
    }

    fn title(&self) -> String {
        "NACUT".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "NACUT".into(), description: self.tooltip.clone(), icon_name: String::new(), icon_pixmap: vec![] }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send_blocking(TrayAction::Toggle);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let item = |label: &str, action: fn() -> TrayAction| -> ksni::MenuItem<Self> {
            StandardItem { label: label.into(), activate: Box::new(move |t: &mut Self| { let _ = t.tx.send_blocking(action()); }), ..Default::default() }.into()
        };
        vec![
            StandardItem { label: self.title.clone(), enabled: false, ..Default::default() }.into(),
            ksni::MenuItem::Separator,
            item("Open NACUT", || TrayAction::Open),
            item("Refresh now", || TrayAction::Refresh),
            item("Preferences…", || TrayAction::Preferences),
            ksni::MenuItem::Separator,
            item("Quit NACUT", || TrayAction::Quit),
        ]
    }
}

pub struct Tray {
    handle: Option<ksni::blocking::Handle<NacutTray>>,
    last_key: std::cell::RefCell<String>,
}

impl Tray {
    pub fn spawn(tx: async_channel::Sender<TrayAction>, status: &StatusView) -> Self {
        let tray = NacutTray { tx, title: status.menu_bar_text.clone(), tooltip: status.tooltip.clone(), icons: render_icons(status) };
        let handle = match tray.spawn() {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("NACUT: no tray available ({e}); open the panel from the app launcher instead");
                None
            }
        };
        Self { handle, last_key: Default::default() }
    }

    pub fn update(&self, status: &StatusView) {
        let Some(h) = &self.handle else { return };
        let key = format!("{:?}|{:?}|{}|{}", status.gauge_fraction.map(|f| (f * 100.0).round()), status.level, status.menu_bar_text, status.tooltip);
        if *self.last_key.borrow() == key {
            return;
        }
        *self.last_key.borrow_mut() = key;
        let (title, tooltip, icons) = (if status.has_loaded { status.menu_bar_text.clone() } else { "NACUT · loading…".into() }, status.tooltip.clone(), render_icons(status));
        h.update(move |t| {
            t.title = title;
            t.tooltip = tooltip;
            t.icons = icons;
        });
    }

    pub fn shutdown(&self) {
        if let Some(h) = &self.handle {
            h.shutdown().wait();
        }
    }
}

fn render_icons(status: &StatusView) -> Vec<ksni::Icon> {
    [22, 32, 48].iter().map(|s| render(status, *s)).collect()
}

/// Draws the icon (white ink: GNOME's top bar is dark in both themes) as ARGB32 in network byte order.
pub fn render(status: &StatusView, size: i32) -> ksni::Icon {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).expect("surface");
    {
        let cr = cairo::Context::new(&surface).expect("context");
        let s = size as f64;
        let accent = match status.level {
            Level::Critical => (0.94, 0.32, 0.32),
            Level::Warning => (0.94, 0.64, 0.23),
            Level::Normal => (0.90, 0.51, 0.38),
        };
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        // Grayscale antialiasing: subpixel (colored) fringes look wrong on a transparent icon.
        if let Ok(mut options) = cairo::FontOptions::new() {
            options.set_antialias(cairo::Antialias::Gray);
            options.set_hint_style(cairo::HintStyle::Slight);
            cr.set_font_options(&options);
        }
        let bar_h = (s / 8.0).max(2.0);
        if let Some(f) = status.gauge_fraction {
            let pct = (f * 100.0).round() as i64;
            let label = if pct >= 100 { "!".to_string() } else { pct.to_string() };
            cr.set_font_size(if label.len() >= 2 { s * 0.62 } else { s * 0.72 });
            if status.level == Level::Normal {
                cr.set_source_rgb(1.0, 1.0, 1.0);
            } else {
                cr.set_source_rgb(accent.0, accent.1, accent.2);
            }
            centered(&cr, &label, s, s - bar_h - 1.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.32);
            pill(&cr, 0.5, s - bar_h, s - 1.0, bar_h);
            cr.set_source_rgb(accent.0, accent.1, accent.2);
            pill(&cr, 0.5, s - bar_h, ((s - 1.0) * f.clamp(0.0, 1.0)).max(bar_h), bar_h);
        } else {
            cr.set_font_size(s * 0.8);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            centered(&cr, "$", s, s);
        }
    }
    let stride = surface.stride() as usize;
    let data = surface.data().expect("pixels");
    let mut argb = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size as usize {
        for x in 0..size as usize {
            let i = y * stride + x * 4;
            // cairo ARGB32 is native-endian (BGRA bytes on little-endian); SNI wants ARGB big-endian.
            let px = u32::from_ne_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            argb.extend_from_slice(&px.to_be_bytes());
        }
    }
    ksni::Icon { width: size, height: size, data: argb }
}

fn centered(cr: &cairo::Context, text: &str, width: f64, height: f64) {
    if let Ok(ext) = cr.text_extents(text) {
        cr.move_to((width - ext.width()) / 2.0 - ext.x_bearing(), (height - ext.height()) / 2.0 - ext.y_bearing());
        let _ = cr.show_text(text);
    }
}

fn pill(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64) {
    crate::charts::rounded_rect(cr, x, y, w, h, h / 2.0);
    let _ = cr.fill();
}
