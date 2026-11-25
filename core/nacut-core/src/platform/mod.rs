//! Operating-system services behind small, testable functions: app folders, the Claude Code sign-in,
//! refreshing it through the CLI, API key storage, WSL discovery and file watching.

pub mod credentials;
pub mod refresher;
pub mod secrets;
pub mod watcher;
pub mod wsl;

use std::path::PathBuf;

use crate::scanner::home_dir;

pub const APP_NAME: &str = "NACUT";

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Where the history cache and API cache live.
/// macOS: `~/Library/Application Support/NACUT`; Windows: `%APPDATA%\NACUT`;
/// Linux: `$XDG_DATA_HOME/nacut` (default `~/.local/share/nacut`).
pub fn data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir().join("Library").join("Application Support").join(APP_NAME)
    } else if cfg!(windows) {
        env_dir("APPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Roaming")).join(APP_NAME)
    } else {
        env_dir("XDG_DATA_HOME").unwrap_or_else(|| home_dir().join(".local").join("share")).join("nacut")
    }
}

/// Where `settings.json` lives (Windows and Linux; the Mac uses UserDefaults).
/// Windows: `%APPDATA%\NACUT`; Linux: `$XDG_CONFIG_HOME/nacut` (default `~/.config/nacut`).
pub fn config_dir() -> PathBuf {
    if cfg!(target_os = "macos") || cfg!(windows) {
        data_dir()
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home_dir().join(".config")).join("nacut")
    }
}

/// Claude Code's config folders: `$CLAUDE_CONFIG_DIR` entries first, then `~/.claude`.
pub fn claude_config_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(crate::scanner::expand_tilde).collect())
        .unwrap_or_default();
    dirs.push(home_dir().join(".claude"));
    dirs.dedup();
    dirs
}

/// Hides the console window that Windows would otherwise flash for child processes.
#[cfg(windows)]
pub(crate) fn no_window(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn no_window(_cmd: &mut std::process::Command) {}
