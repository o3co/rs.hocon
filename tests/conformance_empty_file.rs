//! S3.1 conformance — empty-file fixtures from xx.hocon.
//!
//! All ef01-ef06 fixtures must produce a parse error per HOCON.md L130.
//! Fixture dir: tests/testdata/hocon/empty-file/
//! Expected dir: tests/testdata/expected/empty-file/ (contains `-expected.json`
//! with `{}` that marks the ground truth; however per the cluster override-list
//! these are known-error fixtures — the impl MUST error for all of them).
//!
//! RED: fails until S3.1 empty-stream guard is implemented.

use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/empty-file")
}

fn run_empty_file_fixture(name: &str) {
    let path = fixture_dir().join(format!("{}.conf", name));
    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_file_with_env(&path, &env);
    assert!(
        result.is_err(),
        "S3.1 conformance: {}.conf must produce an error (HOCON.md L130 — empty file invalid), got Ok",
        name
    );
}

#[test]
fn ef01_empty() {
    run_empty_file_fixture("ef01-empty");
}

#[test]
fn ef02_whitespace_only() {
    run_empty_file_fixture("ef02-whitespace-only");
}

#[test]
fn ef03_newlines_only() {
    run_empty_file_fixture("ef03-newlines-only");
}

#[test]
fn ef04_comment_only() {
    run_empty_file_fixture("ef04-comment-only");
}

#[test]
fn ef05_bom_only() {
    run_empty_file_fixture("ef05-bom-only");
}

#[test]
fn ef06_mixed_ws_comment() {
    run_empty_file_fixture("ef06-mixed-ws-comment");
}
