//! Operating-system services behind small, testable functions: app folders, the Claude Code sign-in,
//! refreshing it through the CLI, API key storage, WSL discovery and file watching.

pub mod credentials;
pub mod net;
pub mod refresher;
pub mod secrets;
pub mod watcher;
pub mod wsl;

use std::path::{Path, PathBuf};

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
    claude_config_dirs_with(std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref(), &home_dir())
}

/// [`claude_config_dirs`] for a given `CLAUDE_CONFIG_DIR` value and home folder.
pub fn claude_config_dirs_with(claude_config_dir: Option<&str>, home: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = claude_config_dir
        .map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(crate::scanner::expand_tilde).collect())
        .unwrap_or_default();
    dirs.push(home.join(".claude"));
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// `settings.json` → `settings.json.tmp`, so the temp file can't collide with an unrelated `settings.tmp`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Creates `dir` (and parents) if needed. On Unix the folder itself is made private (0700), since it
/// holds project paths, session IDs and billing data.
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Atomically writes `bytes` to `path` (temp file, then rename), readable only by this user on Unix (0600).
/// Windows: the per-user `%APPDATA%` folder's ACL already keeps other users out.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    let tmp = temp_path(path);
    let _ = std::fs::remove_file(&tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let result = opts.open(&tmp).and_then(|mut f| {
        f.write_all(bytes)?;
        f.sync_all()
    });
    match result.and_then(|_| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn writes_private_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("f.json");
        super::write_private(&path, b"one").unwrap();
        super::write_private(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert!(!super::temp_path(&path).exists());
        assert_eq!(super::temp_path(&path).file_name().unwrap(), "f.json.tmp");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }

    #[test]
    fn temp_path_appends_tmp_to_the_whole_name() {
        use std::path::Path;
        assert_eq!(super::temp_path(Path::new("/d/settings.json")), Path::new("/d/settings.json.tmp"));
        assert_eq!(super::temp_path(Path::new("/d/noext")), Path::new("/d/noext.tmp"));
        assert_eq!(super::temp_path(Path::new("rel")), Path::new("rel.tmp"));
    }

    #[test]
    fn data_and_config_folders_are_named_for_the_app() {
        let (data, config) = (super::data_dir(), super::config_dir());
        if cfg!(target_os = "macos") {
            assert!(data.ends_with("Library/Application Support/NACUT"), "{data:?}");
            assert_eq!(config, data);
        } else if cfg!(windows) {
            assert!(data.ends_with("NACUT"), "{data:?}");
            assert_eq!(config, data);
        } else {
            assert!(data.ends_with("nacut") && config.ends_with("nacut"), "{data:?} {config:?}");
        }
    }

    #[test]
    fn refresh_runs_without_tools_or_project_settings() {
        let args = super::refresher::ARGS;
        let after = |flag: &str| args.iter().position(|a| *a == flag).map(|i| args[i + 1]);
        assert_eq!(after("--tools"), Some(""));
        assert_eq!(after("--setting-sources"), Some("user"));
        assert!(args.contains(&"--strict-mcp-config"));
    }
}
