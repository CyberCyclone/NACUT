//! Gets Claude Code to refresh its own sign-in.
//!
//! The Claude desktop app's Code tab signs in separately, so the CLI's stored token goes stale
//! (it lasts about 8 hours) unless the CLI runs. Rather than rotating the token ourselves, which
//! could race with Claude Code and sign it out, we run the CLI with a one-word Haiku prompt:
//! Claude Code refreshes and saves the token itself. Runs at most once every 15 minutes, and only
//! while the "Refresh sign-in automatically" setting is on.
//!
//! The run is locked down:
//! - `--tools ""`: no tools at all; `--strict-mcp-config` with an empty config: no MCP servers.
//! - `--setting-sources user`: project and local settings (and their hooks) are never loaded.
//! - It runs in a private folder (0700 on Unix) that only this app writes to, never a shared
//!   temp folder where another user could plant `.claude/settings.json`.
//! - `--no-session-persistence` keeps it out of the transcripts this app counts.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::scanner::home_dir;

const MIN_INTERVAL: Duration = Duration::from_secs(15 * 60);
const TIMEOUT: Duration = Duration::from_secs(60);

static LAST_ATTEMPT: Mutex<Option<Instant>> = Mutex::new(None);

/// Runs the refresh unless one ran in the last 15 minutes. Returns whether the CLI exited successfully.
pub fn refresh() -> bool {
    {
        let mut last = LAST_ATTEMPT.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < MIN_INTERVAL) {
            return false;
        }
        *last = Some(Instant::now());
    }
    if in_flatpak() {
        return run_on_host();
    }
    let Some(exe) = executable() else { return false };
    let Some(dir) = work_dir() else { return false };
    run(exe, dir)
}

/// Inside a Flatpak sandbox the host's `claude` isn't visible.
fn in_flatpak() -> bool {
    cfg!(target_os = "linux") && std::path::Path::new("/.flatpak-info").exists()
}

/// A folder only this user can enter, for the CLI's working directory.
fn work_dir() -> Option<PathBuf> {
    let dir = super::data_dir().join("claude-refresh");
    super::create_private_dir(&dir).ok()?;
    Some(dir)
}

/// Runs the host's `claude` through `flatpak-spawn --host` (needs `--talk-name=org.freedesktop.Flatpak`),
/// in a login shell so the user's own PATH (e.g. `~/.local/bin`) applies, from a private folder in the
/// host's `~/.cache`.
fn run_on_host() -> bool {
    const SCRIPT: &str = r#"d="${XDG_CACHE_HOME:-$HOME/.cache}/nacut/claude-refresh" && mkdir -p "$d" && chmod 700 "$d" && cd "$d" && exec claude "$@""#;
    let mut cmd = Command::new("flatpak-spawn");
    cmd.args(["--host", "sh", "-lc", SCRIPT, "sh"]).args(ARGS);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    wait(cmd)
}

/// The Claude Code CLI, from its usual install locations or `PATH` (absolute entries only).
pub fn executable() -> Option<PathBuf> {
    let home = home_dir();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        candidates.push(home.join(".local").join("bin").join("claude.exe"));
        if let Some(appdata) = std::env::var_os("APPDATA") {
            candidates.push(PathBuf::from(appdata).join("npm").join("claude.cmd"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Programs").join("claude").join("claude.exe"));
        }
    } else {
        candidates.push(home.join(".local").join("bin").join("claude"));
        candidates.push(home.join(".claude").join("local").join("claude"));
        candidates.push(PathBuf::from("/opt/homebrew/bin/claude"));
        candidates.push(PathBuf::from("/usr/local/bin/claude"));
        candidates.push(home.join(".npm-global").join("bin").join("claude"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        // Relative entries (".", "bin") would resolve against whatever folder we happen to be in.
        for dir in std::env::split_paths(&path).filter(|d| d.is_absolute()) {
            for name in if cfg!(windows) { &["claude.exe", "claude.cmd"][..] } else { &["claude"][..] } {
                candidates.push(dir.join(name));
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub(crate) const ARGS: [&str; 12] = [
    "-p",
    "--model",
    "haiku",
    "--no-session-persistence",
    "--max-turns",
    "1",
    "--tools",
    "",
    "--setting-sources",
    "user",
    "--strict-mcp-config",
    "--mcp-config={\"mcpServers\":{}}",
];

fn run(exe: PathBuf, dir: PathBuf) -> bool {
    // A `.cmd` (npm install) is passed straight to `Command`: the standard library quotes batch-file
    // arguments safely itself, which wrapping it in `cmd /C` would bypass.
    let mut cmd = Command::new(&exe);
    cmd.args(ARGS);
    cmd.current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(parent) = exe.parent() {
        let mut paths = vec![parent.to_path_buf()];
        if let Some(p) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
    }
    super::no_window(&mut cmd);
    wait(cmd)
}

/// Spawns the CLI, sends the prompt on stdin and waits up to the timeout.
fn wait(mut cmd: Command) -> bool {
    let Ok(mut child) = cmd.spawn() else { return false };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(b"Reply with OK");
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn flatpak_is_only_detected_on_linux() {
        assert!(!in_flatpak());
    }

    #[test]
    fn refresh_is_a_single_non_interactive_haiku_turn() {
        assert_eq!(&ARGS[..3], ["-p", "--model", "haiku"]);
        assert!(ARGS.contains(&"--no-session-persistence"));
        let turns = ARGS.iter().position(|a| *a == "--max-turns").map(|i| ARGS[i + 1]);
        assert_eq!(turns, Some("1"));
        assert_eq!(ARGS.last(), Some(&"--mcp-config={\"mcpServers\":{}}"));
    }

    #[test]
    fn rate_limit_and_timeout() {
        assert_eq!(MIN_INTERVAL, Duration::from_secs(900));
        assert_eq!(TIMEOUT, Duration::from_secs(60));
    }
}
