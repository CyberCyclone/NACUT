//! The `nacut` binary end to end, in a sandbox: every folder it could read or write (home, config, data,
//! temp, Claude Code's config) points into a temporary directory, and `USER` is removed so macOS never
//! reaches for the Keychain. `limits` is never run (it would call the network).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let s = Self { root: tempfile::tempdir().unwrap() };
        std::fs::create_dir_all(s.home()).unwrap();
        std::fs::create_dir_all(s.tmp()).unwrap();
        s
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    fn tmp(&self) -> PathBuf {
        self.root.path().join("tmp")
    }

    /// Where the CLI reads `settings.json` on this OS (mirrors `platform::config_dir`).
    fn config_dir(&self) -> PathBuf {
        let home = self.home();
        if cfg!(target_os = "macos") {
            home.join("Library").join("Application Support").join("NACUT")
        } else if cfg!(windows) {
            home.join("AppData").join("Roaming").join("NACUT")
        } else {
            home.join(".config").join("nacut")
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let home = self.home();
        Command::new(env!("CARGO_BIN_EXE_nacut"))
            .args(args)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local").join("share"))
            .env("APPDATA", home.join("AppData").join("Roaming"))
            .env("LOCALAPPDATA", home.join("AppData").join("Local"))
            .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
            .env("TMPDIR", self.tmp())
            .env("TMP", self.tmp())
            .env("TEMP", self.tmp())
            .env_remove("USER")
            .env_remove("HTTPS_PROXY")
            .env_remove("https_proxy")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            .output()
            .expect("run nacut")
    }

    /// Every path under the sandbox, relative and sorted.
    fn contents(&self) -> Vec<String> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                out.push(e.path().strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
                if e.file_type().unwrap().is_dir() {
                    walk(&e.path(), base, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(self.root.path(), self.root.path(), &mut out);
        out.sort();
        out
    }
}

fn fixtures_root() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shared/fixtures/proj").to_string_lossy().into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

fn json(o: &Output) -> Value {
    assert!(o.status.success(), "exit {:?}: {}", o.status, stderr(o));
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", stdout(o)))
}

fn close(v: &Value, expected: f64) {
    let x = v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"));
    assert!((x - expected).abs() < 1e-9, "{x} != {expected}");
}

fn keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort();
    k
}

const UNTOUCHED: [&str; 2] = ["home", "tmp"];

// MARK: summary

#[test]
fn summary_json_totals_the_fixtures() {
    let sb = Sandbox::new();
    let v = json(&sb.run(&["summary", "--json", "--root", &fixtures_root()]));
    assert_eq!(keys(&v), ["allTime", "month", "monthByModel", "records", "scanSeconds", "today"]);
    assert_eq!(v["records"], 5);
    close(&v["allTime"], 26.72344);
    assert!(v["today"].is_number() && v["month"].is_number() && v["scanSeconds"].is_number());
    for m in v["monthByModel"].as_array().unwrap() {
        assert_eq!(keys(m), ["cost", "id", "name"]);
    }
    assert_eq!(sb.contents(), UNTOUCHED);
}

#[test]
fn summary_text_lists_totals() {
    let sb = Sandbox::new();
    let o = sb.run(&["summary", "--root", &fixtures_root()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[0].starts_with("5 messages from 1 folder(s), read in "), "{out}");
    assert!(lines.contains(&format!("{:<14}{:>12}", "All time", "$26.72").as_str()), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("Today ")), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("This month ")), "{out}");
    assert!(!out.contains("API-equivalent"), "{out}");
    assert_eq!(sb.contents(), UNTOUCHED);
}

#[test]
fn summary_notes_api_equivalent_value_on_a_subscription() {
    let sb = Sandbox::new();
    std::fs::write(sb.home().join(".claude.json"), r#"{"oauthAccount":{"organizationType":"claude_max"}}"#).unwrap();
    let o = sb.run(&["summary", "--root", &fixtures_root()]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("Costs are API-equivalent value at list prices"), "{}", stdout(&o));
    assert_eq!(sb.contents(), ["home", "home/.claude.json", "tmp"]);
}

#[test]
fn summary_with_an_empty_root_reports_zero() {
    let sb = Sandbox::new();
    let empty = sb.root.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let empty = empty.to_string_lossy().into_owned();
    let v = json(&sb.run(&["summary", "--json", "--root", &empty]));
    assert_eq!(v["records"], 0);
    close(&v["allTime"], 0.0);
    assert_eq!(v["monthByModel"], serde_json::json!([]));
}

// MARK: status

#[test]
fn status_json_is_the_status_view() {
    let sb = Sandbox::new();
    let v = json(&sb.run(&["status", "--json", "--root", &fixtures_root()]));
    assert_eq!(v["has_loaded"], true);
    assert_eq!(v["is_scanning"], false);
    assert_eq!(v["shows_limits"], false);
    assert_eq!(v["source_label"], "local logs");
    assert_eq!(v["limits"], serde_json::json!([]));
    assert_eq!(v["limits_error"], Value::Null);
    assert_eq!(v["level"], "normal");
    close(&v["all_time"], 26.72344);
    assert!(v["menu_bar_text"].as_str().unwrap().starts_with('$'));
    assert_eq!(sb.contents(), UNTOUCHED);
}

#[test]
fn status_waybar_json_shape() {
    let sb = Sandbox::new();
    let v = json(&sb.run(&["status", "--waybar", "--root", &fixtures_root()]));
    assert_eq!(keys(&v), ["class", "percentage", "text", "tooltip"]);
    assert!(["normal", "warning", "critical"].contains(&v["class"].as_str().unwrap()));
    assert_eq!(v["class"], "normal");
    // No budget and no limits: nothing to fill a gauge with.
    assert_eq!(v["percentage"], 0);
    assert!(v["text"].as_str().unwrap().starts_with('$'));
    assert!(v["tooltip"].as_str().unwrap().starts_with("NACUT\nToday "));
    assert_eq!(sb.contents(), UNTOUCHED);
}

#[test]
fn status_text_has_the_menu_bar_line_and_totals() {
    let sb = Sandbox::new();
    let o = sb.run(&["status", "--root", &fixtures_root()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].starts_with('$') && lines[0].contains(" · "), "{out}");
    assert!(lines[1].starts_with("Today $") && lines[1].contains(" · this cycle $"), "{out}");
}

#[test]
fn status_reads_settings_without_changing_them() {
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.config_dir()).unwrap();
    let path = sb.config_dir().join("settings.json");
    let text = br#"{"secondFigure":"allTime","cycleStartDay":0}"#;
    std::fs::write(&path, text).unwrap();
    let before = sb.contents();
    let o = sb.run(&["status", "--root", &fixtures_root()]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).lines().next().unwrap().ends_with(" · $26.72"), "{}", stdout(&o));
    assert_eq!(std::fs::read(&path).unwrap(), text);
    assert_eq!(sb.contents(), before);
}

#[test]
fn status_is_the_default_command() {
    let sb = Sandbox::new();
    // No --root: the default folders are inside the empty sandbox home. WSL is turned off so a Windows machine
    // with a running distribution can't add transcripts from outside the sandbox.
    std::fs::create_dir_all(sb.config_dir()).unwrap();
    std::fs::write(sb.config_dir().join("settings.json"), r#"{"includeWsl":false}"#).unwrap();
    let before = sb.contents();
    let o = sb.run(&[]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "$0.00 · $0.00\nToday $0.00 · this cycle $0.00\n");
    assert_eq!(sb.contents(), before);
}

// MARK: Arguments

fn assert_usage_error(args: &[&str]) {
    let o = Sandbox::new().run(args);
    assert_eq!(o.status.code(), Some(2), "{args:?}");
    assert!(stdout(&o).is_empty(), "{args:?}");
    assert!(stderr(&o).contains("usage:"), "{args:?}: {}", stderr(&o));
}

#[test]
fn unknown_flag_is_a_usage_error() {
    assert_usage_error(&["--nope"]);
}

#[test]
fn root_without_a_value_is_a_usage_error() {
    assert_usage_error(&["summary", "--root"]);
}

#[test]
fn two_commands_are_a_usage_error() {
    assert_usage_error(&["status", "summary"]);
}

#[test]
fn unknown_command_is_a_usage_error() {
    assert_usage_error(&["frobnicate"]);
}

#[test]
fn help_prints_usage_and_succeeds() {
    for flag in ["--help", "-h", "help"] {
        let o = Sandbox::new().run(&[flag]);
        assert_eq!(o.status.code(), Some(0), "{flag}");
        let err = stderr(&o);
        assert!(err.starts_with(&format!("nacut {}: Not Another Claude Usage Tracker", env!("CARGO_PKG_VERSION"))), "{err}");
        assert!(err.contains("nacut summary [--json]") && err.contains("--root <dir>"), "{err}");
    }
}

#[test]
fn version_prints_the_version() {
    for flag in ["-V", "--version"] {
        let o = Sandbox::new().run(&[flag]);
        assert_eq!(o.status.code(), Some(0));
        assert_eq!(stdout(&o), format!("nacut {}\n", env!("CARGO_PKG_VERSION")));
    }
}
