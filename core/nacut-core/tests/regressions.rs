//! Bugs found while writing the test suite. Each test failed before its fix.

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use nacut_core::analytics::{status_for, ApiCostData, ApiCostRow, ApiError, ConnectionStatus};
use nacut_core::merge::DataSourceMode;
use nacut_core::platform::{claude_config_dirs_with, write_private};
use nacut_core::pricing::PriceBook;
use nacut_core::record::project_name;
use nacut_core::views::limits_verdict;
use nacut_core::{Cal, CurrencyFormat, Engine, EngineConfig, PlanLimit, Settings};

#[test]
fn bedrock_ids_with_a_date_and_version_normalize_fully() {
    // The version tag hid the date, so the model fell back to its family and was marked estimated.
    assert_eq!(PriceBook::normalize("anthropic.claude-sonnet-5-5-20261001-v1:0"), "claude-sonnet-5-5");
    assert_eq!(PriceBook::normalize("us.anthropic.claude-haiku-5-5-20261001-v2"), "claude-haiku-5-5");
}

#[test]
fn token_counts_roll_over_to_the_next_unit() {
    assert_eq!(CurrencyFormat::tokens(999_950), "1.0M");
    assert_eq!(CurrencyFormat::tokens(999_950_000), "1.00B");
    assert_eq!(CurrencyFormat::tokens(999_949), "999.9K");
}

#[test]
fn compact_negative_dollars_put_the_sign_first() {
    assert_eq!(CurrencyFormat::string(-12_345.0, true), "-$12.3k");
}

#[test]
fn verdict_handles_a_limit_name_starting_with_a_non_ascii_letter() {
    let now: Timestamp = "2026-11-20T00:00:00Z".parse().unwrap();
    let full = PlanLimit {
        id: "seven_day_eclair".into(),
        label: "Weekly · Éclair".into(),
        utilization: 1.0,
        resets_at: Some("2026-11-21T00:00:00Z".parse().unwrap()),
    };
    let v = limits_verdict(&[full], now, &Cal::utc());
    assert!(v.text.starts_with("Éclair weekly limit reached"), "{}", v.text);
}

#[test]
fn decoding_errors_are_cut_on_a_character_boundary() {
    let message = format!("{}é and more", "x".repeat(159));
    let ConnectionStatus::Failed(text) = status_for(&ApiError::Decoding(message)) else { panic!() };
    assert!(text.ends_with(&"x".repeat(159)), "{text}");
}

#[test]
fn root_folder_has_a_project_name() {
    assert_eq!(project_name("/"), "Unknown");
    assert_eq!(project_name("\\"), "Unknown");
}

#[test]
fn config_dirs_drop_every_duplicate() {
    let home = Path::new("/home/me");
    let dirs = claude_config_dirs_with(Some("/home/me/.claude, /work/claude"), home);
    assert_eq!(dirs, [PathBuf::from("/home/me/.claude"), PathBuf::from("/work/claude")]);
}

#[test]
fn private_writes_leave_unrelated_temp_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mine = dir.path().join("settings.tmp");
    std::fs::write(&mine, b"keep me").unwrap();
    write_private(&dir.path().join("settings.json"), b"{}").unwrap();
    assert_eq!(std::fs::read(&mine).unwrap(), b"keep me");
}

#[test]
fn api_caches_saved_before_the_coverage_clamp_keep_older_log_days() {
    // Caches from the previous release start at the first local record. Refreshes are incremental and kept that
    // start, so in Hybrid mode every older day without API rows counted as billed $0 until a full refresh.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    let line = r#"{"type":"assistant","timestamp":"2025-06-15T12:00:00.000Z","requestId":"r1","uuid":"a1","sessionId":"s","cwd":"/x/app","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":0,"output_tokens":1000000}}}"#;
    std::fs::write(root.join("s.jsonl"), format!("{line}\n")).unwrap();
    let row = ApiCostRow { start: ts("2026-10-10T12:00:00Z"), end: ts("2026-10-10T13:00:00Z"), model: Some("claude-opus-5-5".into()), amount: 7.0, list_amount: 7.0 };
    let data = ApiCostData {
        rows: vec![row],
        refreshed_at: Some(ts("2026-10-30T00:00:00Z")),
        coverage_start: ts("2025-05-01T00:00:00Z"),
        coverage_end: ts("2026-10-31T00:00:00Z"),
        fetched_at: ts("2026-10-31T00:00:00Z"),
    };
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("api-cache-v2.json"), serde_json::to_vec(&serde_json::json!({ "actor_id": null, "data": data })).unwrap()).unwrap();

    let e = Engine::new(EngineConfig {
        data_dir,
        settings_path: Some(dir.path().join("settings.json")),
        settings: Some(Settings { data_source: DataSourceMode::Hybrid, ..Settings::default() }),
        roots_override: Some(vec![root]),
    });
    e.scan_now();
    let snap = e.snapshot();
    let june = snap.series.iter().find(|d| (d.date.year, d.date.month, d.date.day) == (2025, 6, 15));
    assert!(june.is_some_and(|d| d.cost > 0.0), "{june:?}");
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}
