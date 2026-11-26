//! Transcript files on disk: checkpoints, partial lines, folder walking and default roots.

use std::io::Write;
use std::path::{Path, PathBuf};

use nacut_core::scanner::{expand_tilde, home_dir, path_key, FileCheckpoint};
use nacut_core::LogScanner;

mod common;

fn line(id: &str) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"2026-11-20T01:00:00Z","message":{{"id":"{id}","model":"claude-opus-5-5","usage":{{"input_tokens":1,"output_tokens":1}}}}}}"#
    )
}

fn ids(path: &Path, cp: Option<FileCheckpoint>) -> Option<Vec<String>> {
    LogScanner::scan_file(path, cp).map(|r| r.records.into_iter().map(|r| r.id).collect())
}

fn append(path: &Path, text: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

#[test]
fn missing_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(LogScanner::scan_file(&dir.path().join("nope.jsonl"), None).is_none());
}

#[test]
fn empty_file_gives_an_empty_result_at_offset_zero() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    std::fs::write(&file, b"").unwrap();
    let r = LogScanner::scan_file(&file, None).unwrap();
    assert!(r.records.is_empty());
    assert_eq!(r.checkpoint.offset, 0);
    assert_eq!(r.path, path_key(&file));
}

#[test]
fn unchanged_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    std::fs::write(&file, format!("{}\n", line("m1"))).unwrap();
    let r = LogScanner::scan_file(&file, None).unwrap();
    assert_eq!(r.checkpoint.offset, std::fs::metadata(&file).unwrap().len());
    assert!(LogScanner::scan_file(&file, Some(r.checkpoint)).is_none());
}

#[test]
fn data_without_a_newline_keeps_the_old_offset() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    let first = format!("{}\n", line("m1"));
    std::fs::write(&file, &first).unwrap();
    let r1 = LogScanner::scan_file(&file, None).unwrap();
    append(&file, &line("m2"));
    let r2 = LogScanner::scan_file(&file, Some(r1.checkpoint)).unwrap();
    assert!(r2.records.is_empty());
    assert_eq!(r2.checkpoint, r1.checkpoint);
    assert_eq!(r2.checkpoint.offset, first.len() as u64);
}

#[test]
fn a_file_with_only_a_partial_line_stays_at_zero() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    std::fs::write(&file, line("m1")).unwrap();
    let r = LogScanner::scan_file(&file, None).unwrap();
    assert!(r.records.is_empty());
    assert_eq!(r.checkpoint.offset, 0);
}

#[test]
fn a_new_inode_rereads_from_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    std::fs::write(&file, format!("{}\n{}\n", line("m1"), line("m2"))).unwrap();
    let r = LogScanner::scan_file(&file, None).unwrap();
    let stale = FileCheckpoint { inode: r.checkpoint.inode.wrapping_add(1), offset: r.checkpoint.offset };
    assert_eq!(ids(&file, Some(stale)).unwrap(), ["m1", "m2"]);
}

#[test]
fn a_truncated_file_rereads_from_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    std::fs::write(&file, format!("{}\n", line("m1"))).unwrap();
    let r = LogScanner::scan_file(&file, None).unwrap();
    let beyond = FileCheckpoint { inode: r.checkpoint.inode, offset: r.checkpoint.offset + 1000 };
    let again = LogScanner::scan_file(&file, Some(beyond)).unwrap();
    assert_eq!(again.records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["m1"]);
    assert_eq!(again.checkpoint, r.checkpoint);
}

#[test]
fn appended_lines_are_read_from_the_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    std::fs::write(&file, format!("{}\n", line("m1"))).unwrap();
    let r = LogScanner::scan_file(&file, None).unwrap();
    append(&file, &format!("{}\n", line("m2")));
    assert_eq!(ids(&file, Some(r.checkpoint)).unwrap(), ["m2"]);
}

#[test]
fn files_under_a_subagents_folder_are_subagent_records() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sess").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("a.jsonl"), format!("{}\n", line("m1"))).unwrap();
    std::fs::write(dir.path().join("main.jsonl"), format!("{}\n", line("m2"))).unwrap();
    let r = LogScanner::scan_file(&sub.join("a.jsonl"), None).unwrap();
    assert!(r.records[0].is_subagent);
    let r = LogScanner::scan_file(&dir.path().join("main.jsonl"), None).unwrap();
    assert!(!r.records[0].is_subagent);
}

#[test]
fn byte_order_mark_file_keeps_its_first_record() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.jsonl");
    std::fs::write(&file, format!("\u{feff}{}\r\n{}\r\n", line("m1"), line("m2"))).unwrap();
    assert_eq!(ids(&file, None).unwrap(), ["m1", "m2"]);
}

fn names(files: Vec<PathBuf>, root: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        files.iter().map(|p| p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")).collect();
    v.sort();
    v
}

#[test]
fn walk_finds_jsonl_recursively_and_skips_hidden_and_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for p in ["a.jsonl", "sub/b.jsonl", "sub/deeper/c.jsonl", "notes.txt", "d.jsonl.bak", ".hidden.jsonl", ".dot/e.jsonl", "sub/.x/f.jsonl"] {
        let path = root.join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    std::fs::create_dir_all(root.join("folder.jsonl")).unwrap();
    let files = LogScanner::new(vec![root.to_path_buf()]).transcript_files();
    assert_eq!(names(files, root), ["a.jsonl", "sub/b.jsonl", "sub/deeper/c.jsonl"]);
}

#[cfg(unix)]
#[test]
fn walk_does_not_follow_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("x.jsonl"), b"").unwrap();
    std::fs::write(dir.path().join("real.jsonl"), b"").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked-dir")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("x.jsonl"), dir.path().join("linked.jsonl")).unwrap();
    let files = LogScanner::new(vec![dir.path().to_path_buf()]).transcript_files();
    assert_eq!(names(files, dir.path()), ["real.jsonl"]);
}

#[test]
fn missing_roots_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let scanner = LogScanner::new(vec![dir.path().join("nope"), common::fixtures().join("proj")]);
    assert_eq!(scanner.transcript_files().len(), 2);
}

#[test]
fn rescan_from_checkpoints_reads_nothing_new() {
    let scanner = LogScanner::new(vec![common::fixtures().join("proj")]);
    let first = scanner.scan(&Default::default());
    assert_eq!(first.len(), 2);
    let checkpoints: std::collections::HashMap<_, _> = first.into_iter().map(|r| (r.path, r.checkpoint)).collect();
    // The subagent file is complete, so it's skipped; the main file ends in a partial line, so it's re-read
    // from its checkpoint without producing records or moving.
    let again = scanner.scan(&checkpoints);
    assert_eq!(again.len(), 1);
    assert!(again[0].records.is_empty());
    assert_eq!(again[0].checkpoint, checkpoints[&again[0].path]);
    assert!(again[0].path.ends_with("sess1.jsonl"));
}

#[test]
fn default_roots_without_config_dir() {
    let home = PathBuf::from("/home/me");
    let expected = [home.join(".claude").join("projects"), home.join(".config").join("claude").join("projects")];
    assert_eq!(LogScanner::default_roots_with(&home, None), expected);
    assert_eq!(LogScanner::default_roots_with(&home, Some("")), expected);
    assert_eq!(LogScanner::default_roots_with(&home, Some(" , ,")), expected);
}

#[test]
fn default_roots_drop_a_config_dir_equal_to_home_claude() {
    let home = PathBuf::from("/home/me");
    let roots = LogScanner::default_roots_with(&home, Some("/home/me/.claude,/opt/a,/opt/a"));
    assert_eq!(
        roots,
        [home.join(".claude").join("projects"), home.join(".config").join("claude").join("projects"), PathBuf::from("/opt/a").join("projects")]
    );
}

#[test]
fn expand_tilde_leaves_other_paths_alone() {
    assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
    assert_eq!(expand_tilde("rel/path"), PathBuf::from("rel/path"));
    assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
    assert_eq!(expand_tilde("a/~/b"), PathBuf::from("a/~/b"));
}

#[test]
fn expand_tilde_uses_the_home_folder() {
    assert_eq!(expand_tilde("~"), home_dir());
    assert_eq!(expand_tilde("~/x"), home_dir().join("x"));
    assert_eq!(expand_tilde("~\\x"), home_dir().join("x"));
}

#[test]
fn path_key_is_the_lossy_path_string() {
    assert_eq!(path_key(Path::new("/a/b c/d.jsonl")), "/a/b c/d.jsonl");
}
