//! Open at login through an XDG autostart entry.

use std::path::PathBuf;

pub const APP_ID: &str = "io.github.cybercyclone.nacut";

fn autostart_file() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| nacut_core::scanner::home_dir().join(".config"));
    config.join("autostart").join(format!("{APP_ID}.desktop"))
}

pub fn autostart_enabled() -> bool {
    autostart_file().exists()
}

pub fn set_autostart(on: bool) -> std::io::Result<()> {
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
