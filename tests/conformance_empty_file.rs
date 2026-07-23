//! S3.1 conformance — empty-file fixtures from xx.hocon.
//!
//! All ef01-ef06 fixtures parse to the empty object `{}` per HOCON.md §Omit
//! root braces L134-136 (L130-132 is the JSON baseline, not HOCON-normative).
//! Fixture dir: tests/testdata/hocon/empty-file/
//! Expected dir: tests/testdata/expected/empty-file/ — the `-expected.json`
//! sidecars contain `{}` and are normative as-is; the former per-impl
//! override list (assert-error) was revoked 2026-07-23 (xx.hocon E10).

use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/empty-file")
}

fn run_empty_file_fixture(name: &str) {
    let path = fixture_dir().join(format!("{}.conf", name));
    let env: HashMap<String, String> = HashMap::new();
    let cfg = hocon::parse_file_with_env(&path, &env).unwrap_or_else(|e| {
        panic!(
            "S3.1 conformance: {}.conf must parse to {{}} per the expected sidecar, got error: {}",
            name, e
        )
    });
    assert!(
        cfg.keys().is_empty(),
        "S3.1 conformance: {}.conf must produce an empty config, got keys {:?}",
        name,
        cfg.keys()
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
