//! Bugs found while writing the test suite. Each test failed before its fix.

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use nacut_core::analytics::{status_for, ApiError, ConnectionStatus};
use nacut_core::platform::{claude_config_dirs_with, write_private};
use nacut_core::pricing::PriceBook;
use nacut_core::record::project_name;
use nacut_core::views::limits_verdict;
use nacut_core::{Cal, CurrencyFormat, PlanLimit};

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
