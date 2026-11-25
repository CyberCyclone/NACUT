//! `nacut-gtk --render-to DIR [--limits-json FILE]` saves PNGs of every panel tab, every preferences page
//! and the tray icon, then exits. It reads the logs once with no network or keyring access (like the
//! Mac app's `--render-to`). Needs a display (it renders through GTK).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use crate::model::AppModel;
use crate::views::panel::Panel;
use crate::views::settings::Preferences;

pub fn run(app: adw::Application, model: Rc<AppModel>, panel: Rc<Panel>, prefs: Rc<Preferences>, dir: PathBuf) {
    if let Some(json) = arg("--limits-json") {
        if let Ok(data) = std::fs::read(json) {
            model.engine.debug_set_limits(nacut_core::limits::PlanLimit::parse(&data));
            model.reload();
        }
    }
    let _ = std::fs::create_dir_all(&dir);
    let theme = if crate::theme::is_dark() { "dark" } else { "light" };
    gtk::glib::spawn_future_local(async move {
        let tabs = ["overview", "calendar", "models", "projects"];
        panel.show();
        for (i, name) in tabs.iter().enumerate() {
            panel.select_tab(i);
            panel.refresh();
            settle().await;
            save(panel.window.upcast_ref(), &dir.join(format!("panel-{name}-{theme}.png")));
            if let Some(content) = panel.tab_content() {
                save(&content, &dir.join(format!("tab-{name}-{theme}.png")));
            }
        }
        panel.window.set_visible(false);
        for page in ["general", "budget", "sources", "alerts", "pricing"] {
            prefs.show(page);
            settle().await;
            save(prefs.window.upcast_ref(), &dir.join(format!("settings-{page}-{theme}.png")));
        }
        prefs.window.set_visible(false);
        let status = model.snap().status.clone();
        save_icon(&status, &dir.join("tray.png"));
        eprintln!("Rendered screenshots to {}", dir.display());
        app.quit();
    });
}

async fn settle() {
    gtk::glib::timeout_future(Duration::from_millis(700)).await;
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

/// Renders a widget's current content to a PNG through the GSK renderer.
fn save(widget: &gtk::Widget, path: &Path) {
    let (w, h) = (widget.width(), widget.height());
    if w == 0 || h == 0 {
        eprintln!("skip {}: not laid out", path.display());
        return;
    }
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w as f64, h as f64);
    let Some(node) = snapshot.to_node() else { return };
    let Some(native) = widget.native() else { return };
    let Some(renderer) = native.renderer() else { return };
    let texture = renderer.render_texture(&node, None);
    if let Err(e) = texture.save_to_png(path) {
        eprintln!("save {}: {e}", path.display());
    }
}

fn save_icon(status: &nacut_core::views::StatusView, path: &Path) {
    let icon = crate::tray::render(status, 48);
    // ARGB (big-endian) → RGBA for a GdkMemoryTexture.
    let mut rgba = Vec::with_capacity(icon.data.len());
    for px in icon.data.chunks(4) {
        rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
    }
    let bytes = gtk::glib::Bytes::from_owned(rgba);
    let texture = gtk::gdk::MemoryTexture::new(icon.width, icon.height, gtk::gdk::MemoryFormat::R8g8b8a8Premultiplied, &bytes, icon.width as usize * 4);
    let _ = texture.save_to_png(path);
}
