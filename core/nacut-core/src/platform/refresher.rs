//! Gets Claude Code to refresh its own sign-in.
//!
//! The Claude desktop app's Code tab signs in separately, so the CLI's stored token goes stale
//! (it lasts about 8 hours) unless the CLI runs. Rather than rotating the token ourselves, which
//! could race with Claude Code and sign it out, we run the CLI with a one-word Haiku prompt:
//! Claude Code refreshes and saves the token itself. `--no-session-persistence` keeps it out of
//! the transcripts this app counts. Runs at most once every 15 minutes.

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
    run(exe)
}

/// Inside a Flatpak sandbox the host's `claude` isn't visible.
fn in_flatpak() -> bool {
    cfg!(target_os = "linux") && std::path::Path::new("/.flatpak-info").exists()
}

/// Runs the host's `claude` through `flatpak-spawn --host` (needs `--talk-name=org.freedesktop.Flatpak`),
/// in a login shell so the user's own PATH (e.g. `~/.local/bin`) applies.
fn run_on_host() -> bool {
    let mut cmd = Command::new("flatpak-spawn");
    cmd.args(["--host", "sh", "-lc", r#"cd /tmp && exec claude "$@""#, "sh"]).args(ARGS);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    wait(cmd)
}

/// The Claude Code CLI, from its usual install locations or `PATH`.
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
        for dir in std::env::split_paths(&path) {
            for name in if cfg!(windows) { &["claude.exe", "claude.cmd"][..] } else { &["claude"][..] } {
                candidates.push(dir.join(name));
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

const ARGS: [&str; 11] = [
    "-p",
    "--model",
    "haiku",
    "--no-session-persistence",
    "--max-turns",
    "1",
    "--strict-mcp-config",
    "--mcp-config",
    r#"{"mcpServers":{}}"#,
    "--disallowed-tools",
    "Bash,Edit,Write,Read,WebFetch,WebSearch",
];

fn run(exe: PathBuf) -> bool {
    let args = ARGS;
    let is_cmd = exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd"));
    let mut cmd = if is_cmd {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(&exe).args(args);
        c
    } else {
        let mut c = Command::new(&exe);
        c.args(args);
        c
    };
    cmd.current_dir(std::env::temp_dir()).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = exe.parent() {
        let mut paths = vec![dir.to_path_buf()];
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
