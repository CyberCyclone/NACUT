//! `nacut`: Claude usage from the terminal, and for status bars such as Waybar or Polybar.
//!
//! ```text
//! nacut status  [--json | --waybar]   menu bar text, today and this cycle (plan limits if enabled)
//! nacut limits  [--json]              plan limits now (Max/Pro, needs a Claude Code sign-in)
//! nacut summary [--json]              totals and per-model cost for this month
//! options: --root <dir>  scan only this transcript folder (repeatable)
//! ```

use std::path::PathBuf;
use std::time::Instant;

use nacut_core::forecast::CurrencyFormat;
use nacut_core::views::{percent, Level};
use nacut_core::{Engine, EngineConfig, Settings, StatRange};

#[derive(Debug, PartialEq)]
struct Args {
    command: String,
    json: bool,
    waybar: bool,
    roots: Vec<PathBuf>,
}

/// What the command line asks for.
#[derive(Debug, PartialEq)]
enum Parsed {
    Run(Args),
    Help,
    Version,
    /// Not a valid command line: print usage and exit with status 2.
    Invalid,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Parsed {
    let mut a = Args { command: "status".into(), json: false, waybar: false, roots: Vec::new() };
    let mut it = args.into_iter();
    let mut command = None;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--json" => a.json = true,
            "--waybar" => a.waybar = true,
            "--root" => match it.next() {
                Some(r) => a.roots.push(PathBuf::from(r)),
                None => return Parsed::Invalid,
            },
            "-h" | "--help" | "help" => return Parsed::Help,
            "-V" | "--version" => return Parsed::Version,
            s if s.starts_with('-') => return Parsed::Invalid,
            s if command.is_none() => command = Some(s.to_string()),
            _ => return Parsed::Invalid,
        }
    }
    if let Some(c) = command {
        a.command = c;
    }
    Parsed::Run(a)
}

fn usage(code: i32) -> ! {
    eprintln!(
        "nacut {}: Not Another Claude Usage Tracker\n\n\
         usage:\n  nacut status  [--json | --waybar]\n  nacut limits  [--json]\n  nacut summary [--json]\n\n\
         options:\n  --root <dir>   scan only this transcript folder (repeatable)",
        env!("CARGO_PKG_VERSION")
    );
    std::process::exit(code)
}

/// The user's settings, read-only: the CLI never changes them.
fn user_settings() -> Settings {
    Settings::read(&nacut_core::platform::config_dir().join("settings.json")).unwrap_or_else(Settings::detected)
}

/// The engine with the user's settings and the shared cache.
fn engine(args: &Args) -> Engine {
    let settings = user_settings();
    Engine::new(EngineConfig {
        data_dir: nacut_core::platform::data_dir(),
        settings_path: None,
        settings: Some(settings),
        roots_override: (!args.roots.is_empty()).then(|| args.roots.clone()),
    })
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Parsed::Run(a) => a,
        Parsed::Help => usage(0),
        Parsed::Version => {
            println!("nacut {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Parsed::Invalid => usage(2),
    };
    match args.command.as_str() {
        "status" => status(&args),
        "limits" => limits(&args),
        "summary" => summary(&args),
        _ => usage(2),
    }
}

fn status(args: &Args) {
    let e = engine(args);
    e.scan_now();
    if e.settings().limits_enabled {
        e.fetch_limits_now();
    }
    let st = e.status();
    if args.waybar {
        let class = match st.level {
            Level::Normal => "normal",
            Level::Warning => "warning",
            Level::Critical => "critical",
        };
        let v = serde_json::json!({
            "text": st.menu_bar_text,
            "tooltip": st.tooltip,
            "class": class,
            "percentage": st.gauge_fraction.map(|f| (f * 100.0).round() as i64).unwrap_or(0),
        });
        println!("{v}");
    } else if args.json {
        println!("{}", serde_json::to_string_pretty(&st).unwrap());
    } else {
        println!("{}", st.menu_bar_text);
        println!("Today {} · this cycle {}", CurrencyFormat::string(st.today, false), CurrencyFormat::string(st.cycle_spent, false));
        for l in &st.limits {
            println!("{:<22} {:>4}  {}", l.label, percent(l.utilization), l.subtitle);
        }
        if let Some(v) = &st.limits_verdict {
            println!("{}", v.text);
        }
        if let Some(err) = &st.limits_error {
            println!("Plan limits: {err}");
        }
    }
}

fn limits(args: &Args) {
    let mut settings = engine(args).settings();
    settings.limits_enabled = true;
    let e = Engine::new(EngineConfig {
        data_dir: nacut_core::platform::data_dir(),
        settings_path: None,
        settings: Some(settings),
        roots_override: Some(Vec::new()),
    });
    e.fetch_limits_now();
    let st = e.status();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "limits": st.limits, "error": st.limits_error, "verdict": st.limits_verdict })).unwrap());
        return;
    }
    if let Some(err) = &st.limits_error {
        eprintln!("{err}");
        std::process::exit(1);
    }
    for l in &st.limits {
        println!("{:<22} {:>4}  {}", l.label, percent(l.utilization), l.subtitle);
    }
    if let Some(v) = &st.limits_verdict {
        println!("{}", v.text);
    }
}

/// Scans the transcript folders from scratch (ignoring the cache) and prints totals.
/// Uses the same folders as the app (defaults, added folders, WSL) unless `--root` is given.
fn summary(args: &Args) {
    let started = Instant::now();
    // A fresh private folder (random name, 0700); removed when `tmp` drops.
    let tmp = tempfile::Builder::new().prefix("nacut-summary-").tempdir().expect("temporary folder");
    let dir = tmp.path().to_path_buf();
    let mut settings = user_settings();
    settings.limits_enabled = false;
    settings.data_source = nacut_core::merge::DataSourceMode::Logs;
    let e = Engine::new(EngineConfig {
        data_dir: dir.clone(),
        settings_path: None,
        settings: Some(settings),
        roots_override: (!args.roots.is_empty()).then(|| args.roots.clone()),
    });
    e.scan_now();
    let scan_secs = started.elapsed().as_secs_f64();
    let snap = e.snapshot();
    let today = snap.series.iter().find(|d| d.date == snap.today).map(|d| d.cost).unwrap_or(0.0);
    let month_start = nacut_core::views::DayKey { day: 1, ..snap.today };
    let month: f64 = snap.series.iter().filter(|d| d.date >= month_start && d.date <= snap.today).map(|d| d.cost).sum();
    // Calendar month (cycle start day 1 in the default settings).
    let models = e.model_stats(StatRange::Cycle);
    if args.json {
        let v = serde_json::json!({
            "records": snap.record_count,
            "scanSeconds": scan_secs,
            "today": today,
            "month": month,
            "allTime": snap.insights.all_time,
            "monthByModel": models.iter().map(|m| serde_json::json!({"id": m.id, "name": m.name, "cost": m.cost})).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        let usd = |v: f64| CurrencyFormat::string(v, false);
        println!("{} messages from {} folder(s), read in {:.2}s", snap.record_count, e.log_roots().len(), scan_secs);
        println!("{:<14}{:>12}", "Today", usd(today));
        println!("{:<14}{:>12}", "This month", usd(month));
        println!("{:<14}{:>12}", "All time", usd(snap.insights.all_time));
        if !models.is_empty() {
            println!("\n{}", snap.forecast.cycle_label);
            for m in &models {
                println!("  {:<12}{:>12}", m.name, usd(m.cost));
            }
        }
        if snap.settings.account_mode == nacut_core::settings::AccountMode::Subscription {
            println!("\nCosts are API-equivalent value at list prices; on a subscription you don't pay them.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Parsed {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    fn run(command: &str, json: bool, waybar: bool, roots: &[&str]) -> Parsed {
        Parsed::Run(Args { command: command.into(), json, waybar, roots: roots.iter().map(PathBuf::from).collect() })
    }

    #[test]
    fn status_is_the_default_command() {
        assert_eq!(parse(&[]), run("status", false, false, &[]));
    }

    #[test]
    fn flags_and_roots_in_any_order() {
        assert_eq!(parse(&["summary", "--json"]), run("summary", true, false, &[]));
        assert_eq!(parse(&["--waybar", "status"]), run("status", false, true, &[]));
        assert_eq!(parse(&["--root", "/a", "limits", "--root", "/b"]), run("limits", false, false, &["/a", "/b"]));
    }

    #[test]
    fn help_and_version() {
        for a in ["-h", "--help", "help"] {
            assert_eq!(parse(&[a]), Parsed::Help);
        }
        assert_eq!(parse(&["status", "--version"]), Parsed::Version);
        assert_eq!(parse(&["-V"]), Parsed::Version);
    }

    #[test]
    fn invalid_command_lines() {
        assert_eq!(parse(&["--root"]), Parsed::Invalid);
        assert_eq!(parse(&["--nope"]), Parsed::Invalid);
        assert_eq!(parse(&["status", "summary"]), Parsed::Invalid);
    }
}
