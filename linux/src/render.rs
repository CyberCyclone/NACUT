//! `nacut-gtk --render-to DIR [--limits-json FILE]` saves PNGs of every panel tab, every preferences page
//! and the tray icon, then exits. It reads the logs once with no network or keyring access (like the
//! Mac app's `--render-to`). Needs a display (it renders through GTK).
//!
//! It never writes the real data folder, settings.json or ui.json: the engine runs on a copy of the history
//! cache in a private temporary folder (deleted afterwards), settings are read without saving, and ui.json
//! changes stay in memory.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use crate::model::AppModel;
use crate::views::panel::Panel;
use crate::views::settings::Preferences;

/// This run's private scratch folder (see [`engine_config`]).
static SCRATCH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Creates a new, private (0700) folder with an unpredictable name. `create` fails if the path already
/// exists, so another user can't pre-create or symlink it in the shared temp folder.
fn new_scratch_dir() -> PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::temp_dir();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    for attempt in 0..100u32 {
        let name = format!("nacut-render-{}-{:x}", std::process::id(), seed.wrapping_mul(6364136223846793005).wrapping_add(attempt as u128));
        let path = base.join(name);
        if std::fs::DirBuilder::new().mode(0o700).create(&path).is_ok() {
            return path;
        }
    }
    panic!("couldn't create a private temporary folder in {}", base.display());
}

/// Engine setup for screenshots: the real settings (read, never saved) and a scratch copy of the history
/// cache, so nothing under ~/.local/share/nacut or ~/.config/nacut is created, renamed or rewritten.
pub fn engine_config() -> nacut_core::EngineConfig {
    let scratch = SCRATCH.get_or_init(new_scratch_dir).clone();
    let name = nacut_core::cache::CACHE_FILE_NAME;
    let _ = std::fs::copy(nacut_core::platform::data_dir().join(name), scratch.join(name));
    let real = nacut_core::platform::config_dir().join("settings.json");
    let mut config = nacut_core::EngineConfig::standard();
    config.data_dir = scratch;
    config.settings_path = None;
    config.settings = Some(nacut_core::Settings::read(&real).unwrap_or_else(nacut_core::Settings::detected));
    config
}

pub fn run(app: adw::Application, model: Rc<AppModel>, panel: Rc<Panel>, prefs: Rc<Preferences>, dir: PathBuf, limits_json: Option<PathBuf>) {
    if let Some(json) = limits_json {
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
        if let Some(scratch) = SCRATCH.get() {
            let _ = std::fs::remove_dir_all(scratch);
        }
        app.quit();
    });
}

async fn settle() {
    gtk::glib::timeout_future(Duration::from_millis(700)).await;
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
    let bytes = gtk::glib::Bytes::from_owned(argb_to_rgba(&icon.data));
    let texture = gtk::gdk::MemoryTexture::new(icon.width, icon.height, gtk::gdk::MemoryFormat::R8g8b8a8Premultiplied, &bytes, icon.width as usize * 4);
    let _ = texture.save_to_png(path);
}

/// Tray icon pixels (big-endian ARGB) → RGBA bytes for a GdkMemoryTexture. A trailing partial pixel is dropped.
fn argb_to_rgba(argb: &[u8]) -> Vec<u8> {
    argb.chunks_exact(4).flat_map(|px| [px[1], px[2], px[3], px[0]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_moves_alpha_last() {
        assert_eq!(argb_to_rgba(&[0xAA, 0x11, 0x22, 0x33, 0xFF, 0x00, 0x80, 0xFE]), vec![0x11, 0x22, 0x33, 0xAA, 0x00, 0x80, 0xFE, 0xFF]);
    }

    #[test]
    fn argb_empty_and_partial_pixels() {
        assert!(argb_to_rgba(&[]).is_empty());
        assert!(argb_to_rgba(&[1, 2, 3]).is_empty());
        assert_eq!(argb_to_rgba(&[1, 2, 3, 4, 5]), vec![2, 3, 4, 1]);
    }

    #[test]
    fn rendered_tray_icon_converts_to_a_full_texture() {
        let status = crate::tray::tests::status(Some(0.42), nacut_core::views::Level::Normal);
        let icon = crate::tray::render(&status, 48);
        let rgba = argb_to_rgba(&icon.data);
        assert_eq!(rgba.len(), 48 * 48 * 4);
        // Every pixel keeps its alpha, just moved to the end.
        for (a, b) in icon.data.chunks_exact(4).zip(rgba.chunks_exact(4)) {
            assert_eq!(a[0], b[3]);
        }
    }
}
