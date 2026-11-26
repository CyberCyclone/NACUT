//! OS-facing helpers that can run hermetically: sign-in parsing, WSL output, private writes, config folders,
//! and the transcript watcher.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use nacut_core::platform::credentials::{parse, CredentialError};
use nacut_core::platform::watcher::TranscriptWatcher;
use nacut_core::platform::{claude_config_dirs_with, write_private, wsl, APP_NAME};
use nacut_core::scanner::home_dir;

mod common;

// MARK: Credentials

#[test]
fn invalid_credentials_json_is_unreadable() {
    assert!(matches!(parse(b"{ nope"), Err(CredentialError::Unreadable(_))));
    assert!(matches!(parse(b""), Err(CredentialError::Unreadable(_))));
}

#[test]
fn oauth_without_an_access_token_is_unreadable() {
    assert!(matches!(parse(br#"{"claudeAiOauth":{"expiresAt":1}}"#), Err(CredentialError::Unreadable(_))));
}

#[test]
fn null_oauth_is_not_found() {
    assert_eq!(parse(br#"{"claudeAiOauth":null}"#).unwrap_err(), CredentialError::NotFound);
}

#[test]
fn token_without_expiry_never_expires() {
    let t = parse(br#"{"claudeAiOauth":{"accessToken":"a"}}"#).unwrap();
    assert_eq!(t.expires_at, None);
    assert_eq!(t.subscription_type, None);
    assert!(!t.is_expired(Timestamp::MAX));
}

#[test]
fn token_counts_as_expired_one_minute_early() {
    let t = parse(br#"{"claudeAiOauth":{"accessToken":"a","expiresAt":1795429672000}}"#).unwrap();
    let exp = Timestamp::from_millisecond(1_795_429_672_000).unwrap();
    assert!(t.is_expired(exp - SignedDuration::from_secs(60)));
    assert!(!t.is_expired(exp - SignedDuration::from_secs(61)));
    assert!(t.is_expired(exp + SignedDuration::from_secs(1)));
}

#[test]
fn fractional_expiry_is_truncated_to_the_millisecond() {
    let t = parse(br#"{"claudeAiOauth":{"accessToken":"a","expiresAt":1795429672763.9}}"#).unwrap();
    assert_eq!(t.expires_at.unwrap().as_millisecond(), 1_795_429_672_763);
}

#[test]
fn null_expiry_never_expires() {
    let t = parse(br#"{"claudeAiOauth":{"accessToken":"a","expiresAt":null}}"#).unwrap();
    assert_eq!(t.expires_at, None);
}

#[test]
fn subscription_type_is_read() {
    let t = parse(br#"{"claudeAiOauth":{"accessToken":"a","subscriptionType":"pro"}}"#).unwrap();
    assert_eq!(t.subscription_type.as_deref(), Some("pro"));
}

#[test]
fn debug_output_redacts_the_token() {
    let t = parse(br#"{"claudeAiOauth":{"accessToken":"sk-ant-oat-secret","expiresAt":1795429672000}}"#).unwrap();
    let d = format!("{t:?}");
    assert!(!d.contains("sk-ant-oat-secret"), "{d}");
    assert!(d.contains("<redacted>"), "{d}");
}

// MARK: WSL

fn utf16(s: &str, bom: bool) -> Vec<u8> {
    let mut out = if bom { vec![0xFF, 0xFE] } else { vec![] };
    out.extend(s.encode_utf16().flat_map(|u| u.to_le_bytes()));
    out
}

#[test]
fn distro_list_with_a_utf16_byte_order_mark() {
    assert_eq!(wsl::decode_distro_list(&utf16("Ubuntu\r\nDebian\r\n", true)), ["Ubuntu", "Debian"]);
}

#[test]
fn distro_list_with_non_ascii_utf16_names() {
    assert_eq!(wsl::decode_distro_list(&utf16("Убунту\r\nDebian\r\n", false)), ["Убунту", "Debian"]);
    assert_eq!(wsl::decode_distro_list(&utf16("Ünïcode\r\n", false)), ["Ünïcode"]);
}

#[test]
fn empty_distro_list() {
    assert!(wsl::decode_distro_list(b"").is_empty());
    assert!(wsl::decode_distro_list(&utf16("\r\n\r\n", false)).is_empty());
}

#[test]
fn odd_length_output_is_read_as_utf8() {
    assert_eq!(wsl::decode_distro_list(b"Ubuntu\nArch\n\n"), ["Ubuntu", "Arch"]);
    assert_eq!(wsl::decode_distro_list("Débian\n".as_bytes()), ["Débian"]);
}

#[test]
fn even_length_utf8_output_is_read_as_utf8() {
    assert_eq!(wsl::decode_distro_list(b"Ubuntu\n"), ["Ubuntu"]);
    assert_eq!(wsl::decode_distro_list(b"Arch\r\nDebian\r\n"), ["Arch", "Debian"]);
}

#[test]
fn blank_lines_and_nuls_are_dropped() {
    assert_eq!(wsl::decode_distro_list(&utf16("\r\n  Ubuntu  \r\n\0\r\n\r\n", false)), ["Ubuntu"]);
    assert_eq!(wsl::decode_distro_list(b"Ubuntu\0\n\0\n"), ["Ubuntu"]);
}

#[cfg(not(windows))]
#[test]
fn wsl_roots_are_empty_off_windows() {
    assert!(wsl::transcript_roots().is_empty());
}

// MARK: Files and folders

#[test]
fn private_write_fails_when_the_parent_is_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, b"x").unwrap();
    assert!(write_private(&file.join("settings.json"), b"{}").is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"x");
}

#[test]
fn private_write_replaces_a_stale_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(dir.path().join("settings.json.tmp"), b"left over from a crash").unwrap();
    write_private(&path, b"{}").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"{}");
    assert!(!dir.path().join("settings.json.tmp").exists());
}

#[test]
fn private_write_fails_when_the_target_is_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::create_dir(&path).unwrap();
    assert!(write_private(&path, b"{}").is_err());
    assert!(!dir.path().join("settings.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn private_dirs_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().join("a").join("b");
    nacut_core::platform::create_private_dir(&d).unwrap();
    assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
    // Calling it again on an existing folder is fine.
    nacut_core::platform::create_private_dir(&d).unwrap();
}

#[test]
fn config_dirs_without_the_variable() {
    let home = Path::new("/home/me");
    assert_eq!(claude_config_dirs_with(None, home), [home.join(".claude")]);
}

#[test]
fn config_dirs_trim_and_skip_empty_entries() {
    let home = Path::new("/home/me");
    assert_eq!(
        claude_config_dirs_with(Some("  /a , , /b ,"), home),
        [PathBuf::from("/a"), PathBuf::from("/b"), home.join(".claude")]
    );
    assert_eq!(claude_config_dirs_with(Some(""), home), [home.join(".claude")]);
}

#[test]
fn config_dirs_expand_a_tilde() {
    let home = Path::new("/home/me");
    assert_eq!(claude_config_dirs_with(Some("~/work"), home), [home_dir().join("work"), home.join(".claude")]);
}

#[test]
fn app_name() {
    assert_eq!(APP_NAME, "NACUT");
}

// MARK: Watcher

#[test]
fn watcher_needs_an_existing_root() {
    let dir = tempfile::tempdir().unwrap();
    assert!(TranscriptWatcher::start(&[], || {}).is_none());
    assert!(TranscriptWatcher::start(&[dir.path().join("missing")], || {}).is_none());
}

#[test]
fn watcher_reports_a_transcript_write() {
    let dir = tempfile::tempdir().unwrap();
    // Canonical path: FSEvents reports /private/var/… for /var/… temp folders.
    let root = dir.path().canonicalize().unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let _w = TranscriptWatcher::start(&[root.clone()], move || {
        let _ = tx.lock().unwrap().send(());
    })
    .expect("watching a temp folder");
    // Some backends only start delivering events shortly after the watch is registered: keep writing.
    let file = root.join("s.jsonl");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut n = 0;
    loop {
        std::fs::write(&file, format!("{n}\n")).unwrap();
        n += 1;
        if rx.recv_timeout(Duration::from_millis(500)).is_ok() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "no change event within 20 s");
    }
}
