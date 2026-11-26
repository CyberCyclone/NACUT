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
        let key = status_key(status);
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

/// What the tray's icon, title and tooltip depend on; an unchanged key skips the D-Bus update.
fn status_key(status: &StatusView) -> String {
    format!("{}|{:?}|{:?}|{}|{}", status.has_loaded, status.gauge_fraction.map(|f| (f * 100.0).round()), status.level, status.menu_bar_text, status.tooltip)
}

/// The icon's text and font size for a `size`-pixel icon: the gauge percentage ("!" from 100%), or "$" without a gauge.
fn label_for(status: &StatusView, size: f64) -> (String, f64) {
    match status.gauge_fraction {
        Some(f) => {
            // NaN and negatives read 0, like the Windows tray.
            let pct = if f.is_nan() { 0 } else { (f.max(0.0) * 100.0).round() as i64 };
            let label = if pct >= 100 { "!".to_string() } else { pct.to_string() };
            let font = if label.len() >= 2 { size * 0.62 } else { size * 0.72 };
            (label, font)
        }
        None => ("$".into(), size * 0.8),
    }
}

/// Gauge (and warning text) color for a level.
fn accent(level: Level) -> (f64, f64, f64) {
    match level {
        Level::Critical => (0.94, 0.32, 0.32),
        Level::Warning => (0.94, 0.64, 0.23),
        Level::Normal => (0.90, 0.51, 0.38),
    }
}

/// Height of the gauge bar along the icon's bottom edge.
fn bar_height(size: f64) -> f64 {
    (size / 8.0).max(2.0)
}

/// Draws the icon (white ink: GNOME's top bar is dark in both themes) as ARGB32 in network byte order.
pub fn render(status: &StatusView, size: i32) -> ksni::Icon {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).expect("surface");
    {
        let cr = cairo::Context::new(&surface).expect("context");
        let s = size as f64;
        let accent = accent(status.level);
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        // Grayscale antialiasing: subpixel (colored) fringes look wrong on a transparent icon.
        if let Ok(mut options) = cairo::FontOptions::new() {
            options.set_antialias(cairo::Antialias::Gray);
            options.set_hint_style(cairo::HintStyle::Slight);
            cr.set_font_options(&options);
        }
        let bar_h = bar_height(s);
        let (label, font_size) = label_for(status, s);
        cr.set_font_size(font_size);
        if let Some(f) = status.gauge_fraction {
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
            cr.set_source_rgb(1.0, 1.0, 1.0);
            centered(&cr, &label, s, s);
        }
    }
    let stride = surface.stride() as usize;
    let data = surface.data().expect("pixels");
    ksni::Icon { width: size, height: size, data: to_network_argb(&data, stride, size as usize, size as usize) }
}

/// cairo ARGB32 rows (native-endian pixels, `stride` bytes apart) → packed big-endian ARGB, as SNI wants.
fn to_network_argb(data: &[u8], stride: usize, width: usize, height: usize) -> Vec<u8> {
    let mut argb = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let i = y * stride + x * 4;
            let px = u32::from_ne_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            argb.extend_from_slice(&px.to_be_bytes());
        }
    }
    argb
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn status(gauge_fraction: Option<f64>, level: Level) -> StatusView {
        StatusView {
            has_loaded: true,
            is_scanning: false,
            shows_limits: gauge_fraction.is_some(),
            menu_bar_text: "5h 42% · Week 15%".into(),
            tooltip: "5-hour session 42%".into(),
            gauge_fraction,
            level,
            today: 1.25,
            cycle_spent: 30.0,
            all_time: 300.0,
            last_updated_ms: Some(1_795_694_400_000),
            source_label: "local logs".into(),
            limits: vec![],
            limits_error: None,
            limits_updated_ms: None,
            limits_verdict: None,
        }
    }

    fn label(f: Option<f64>) -> String {
        label_for(&status(f, Level::Normal), 48.0).0
    }

    /// Big-endian ARGB of pixel (x, y).
    fn pixel(icon: &ksni::Icon, x: i32, y: i32) -> [u8; 4] {
        let i = ((y * icon.width + x) * 4) as usize;
        icon.data[i..i + 4].try_into().unwrap()
    }

    fn assert_near(actual: [u8; 4], expected: [u8; 4]) {
        assert!(actual.iter().zip(expected).all(|(a, e)| (*a as i32 - e as i32).abs() <= 2), "{actual:?} vs {expected:?}");
    }

    #[test]
    fn label_is_the_rounded_percentage() {
        assert_eq!(label(Some(0.0)), "0");
        assert_eq!(label(Some(0.05)), "5");
        assert_eq!(label(Some(0.42)), "42");
        assert_eq!(label(Some(0.994)), "99");
        assert_eq!(label(Some(-0.004)), "0");
    }

    #[test]
    fn label_is_a_bang_from_100_percent() {
        assert_eq!(label(Some(0.995)), "!");
        assert_eq!(label(Some(1.0)), "!");
        assert_eq!(label(Some(1.7)), "!");
        assert_eq!(label(Some(f64::INFINITY)), "!");
    }

    #[test]
    fn label_odd_fractions_do_not_panic() {
        assert_eq!(label(Some(f64::NAN)), "0");
        assert_eq!(label(Some(-0.05)), "0");
        assert_eq!(label(Some(f64::NEG_INFINITY)), "0");
    }

    #[test]
    fn dollar_without_a_gauge() {
        assert_eq!(label_for(&status(None, Level::Normal), 48.0), ("$".to_string(), 48.0 * 0.8));
    }

    #[test]
    fn two_digits_use_a_smaller_font() {
        let size = |f| label_for(&status(Some(f), Level::Normal), 100.0).1;
        assert_eq!(size(0.42), 62.0);
        assert_eq!(size(0.07), 72.0);
        assert_eq!(size(1.0), 72.0);
    }

    #[test]
    fn accents_differ_by_level() {
        let (n, w, c) = (accent(Level::Normal), accent(Level::Warning), accent(Level::Critical));
        assert!(n != w && w != c && n != c);
    }

    #[test]
    fn bar_height_has_a_minimum() {
        assert_eq!(bar_height(48.0), 6.0);
        assert_eq!(bar_height(22.0), 2.75);
        assert_eq!(bar_height(8.0), 2.0);
        assert_eq!(bar_height(1.0), 2.0);
    }

    #[test]
    fn status_key_tracks_what_is_shown() {
        let base = status(Some(0.42), Level::Normal);
        assert_eq!(status_key(&base), status_key(&base.clone()));
        // Changes the icon can't show (a fraction of a percent, the cost totals) keep the key.
        let mut same = base.clone();
        same.gauge_fraction = Some(0.4204);
        same.today = 99.0;
        same.is_scanning = true;
        assert_eq!(status_key(&base), status_key(&same));
        let changes: [fn(&mut StatusView); 6] = [
            |s| s.gauge_fraction = Some(0.43),
            |s| s.gauge_fraction = None,
            |s| s.level = Level::Warning,
            |s| s.menu_bar_text = "5h 43%".into(),
            |s| s.tooltip = "other".into(),
            |s| s.has_loaded = false,
        ];
        for change in changes {
            let mut s = base.clone();
            change(&mut s);
            assert_ne!(status_key(&base), status_key(&s), "{s:?}");
        }
    }

    #[test]
    fn network_argb_from_padded_native_rows() {
        let mut data = vec![];
        for row in [[0xAA11_2233u32, 0xFF00_0000], [0x0000_0000, 0x8040_2010]] {
            for px in row {
                data.extend_from_slice(&px.to_ne_bytes());
            }
            data.extend_from_slice(&[9, 9, 9, 9]); // stride padding
        }
        let out = to_network_argb(&data, 12, 2, 2);
        assert_eq!(out, vec![0xAA, 0x11, 0x22, 0x33, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0x80, 0x40, 0x20, 0x10]);
        assert!(to_network_argb(&[], 0, 0, 0).is_empty());
    }

    #[test]
    fn rendered_icon_sizes() {
        for size in [1, 8, 22, 32, 48, 64] {
            for st in [status(Some(0.42), Level::Normal), status(Some(1.2), Level::Critical), status(None, Level::Normal)] {
                let icon = render(&st, size);
                assert_eq!((icon.width, icon.height), (size, size));
                assert_eq!(icon.data.len(), (size * size * 4) as usize);
            }
        }
        assert_eq!(render_icons(&status(Some(0.5), Level::Warning)).iter().map(|i| i.width).collect::<Vec<_>>(), [22, 32, 48]);
    }

    #[test]
    fn gauge_bar_pixels() {
        let to_byte = |c: f64| (c * 255.0).round() as u8;
        // A full gauge: the bar's middle is the opaque accent.
        let (r, g, b) = accent(Level::Normal);
        let full = render(&status(Some(1.0), Level::Normal), 48);
        assert_near(pixel(&full, 24, 45), [255, to_byte(r), to_byte(g), to_byte(b)]);
        // An empty gauge: only the 32% white track there (premultiplied).
        let empty = render(&status(Some(0.0), Level::Normal), 48);
        assert_near(pixel(&empty, 24, 45), [82, 82, 82, 82]);
        // Half full: accent on the left, track on the right.
        let (r, g, b) = accent(Level::Critical);
        let half = render(&status(Some(0.5), Level::Critical), 48);
        assert_near(pixel(&half, 10, 45), [255, to_byte(r), to_byte(g), to_byte(b)]);
        assert_near(pixel(&half, 40, 45), [82, 82, 82, 82]);
        // Nothing is drawn in the corners.
        for icon in [&full, &empty, &half, &render(&status(None, Level::Normal), 48)] {
            assert_eq!(pixel(icon, 0, 0), [0, 0, 0, 0]);
            assert_eq!(pixel(icon, 47, 0), [0, 0, 0, 0]);
        }
    }
}
