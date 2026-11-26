//! Open at login: an XDG autostart entry, or the XDG Background portal inside Flatpak.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gtk::glib::prelude::*; // ToVariant
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.cybercyclone.nacut";

/// ui.json key holding the open-at-login choice sent to the Background portal (Flatpak only).
const PORTAL_PREF: &str = "openAtLogin";

/// Inside Flatpak, XDG_CONFIG_HOME is the app's private `~/.var/app/<id>/config` and the binary is
/// `/app/bin/nacut-gtk`, so an autostart file written there is never seen by the host session.
fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn autostart_file() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| nacut_core::scanner::home_dir().join(".config"));
    config.join("autostart").join(format!("{APP_ID}.desktop"))
}

pub fn autostart_enabled() -> bool {
    if in_flatpak() {
        // The portal keeps its autostart entry on the host, out of our reach; show what was last requested.
        return crate::prefs::get(PORTAL_PREF, "off") == "on";
    }
    autostart_file().exists()
}

pub fn set_autostart(on: bool) -> std::io::Result<()> {
    if in_flatpak() {
        request_background(on)?;
        crate::prefs::set(PORTAL_PREF, if on { "on" } else { "off" });
        return Ok(());
    }
    let path = autostart_file();
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    let exe = std::env::current_exe()?;
    std::fs::write(
        &path,
        format!(
            "[Desktop Entry]\nType=Application\nName=NACUT\nComment=Claude usage in the tray\nExec=\"{}\" --background\nIcon={APP_ID}\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
            exe.display()
        ),
    )
}

/// Asks `org.freedesktop.portal.Background` to add or remove our autostart entry. The portal answers later
/// through a Request object's Response signal (the user may be asked); we only log a failed call.
fn request_background(autostart: bool) -> std::io::Result<()> {
    let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut options: HashMap<String, glib::Variant> = HashMap::new();
    options.insert("reason".into(), "Start NACUT in the tray when you sign in".to_variant());
    options.insert("autostart".into(), autostart.to_variant());
    options.insert("commandline".into(), vec!["nacut-gtk".to_string(), "--background".to_string()].to_variant());
    options.insert("dap-activatable".into(), false.to_variant());
    // (s parent_window, a{sv} options)
    let params = (String::new(), options).to_variant();
    bus.call(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Background",
        "RequestBackground",
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |result| {
            if let Err(e) = result {
                eprintln!("autostart (Background portal): {e}");
            }
        },
    );
    Ok(())
}
