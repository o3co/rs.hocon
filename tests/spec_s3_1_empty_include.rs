//! Lightbend-compat carve-out for go.hocon#105 — empty / whitespace-only /
//! comment-only / BOM-only INCLUDED files contribute an empty config
//! instead of erroring with S3.1.
//!
//! S3.1 (HOCON.md L130) "empty files are invalid documents" remains
//! enforced for TOP-LEVEL parses — see `tests/spec_s3_1_empty_file.rs`.
//! This file pins the narrower include-path carve-out and serves as a
//! regression guard against the previous strict-reject behaviour returning.

use std::io::Write;
use tempfile::Builder;
use tempfile::NamedTempFile;

/// Write content to a NamedTempFile with a `.conf` suffix and return the file (keeping it alive).
///
/// Using `.conf` suffix ensures the include loader uses the exact-path code path (has_extension)
/// rather than the probe-extensions path.
fn tmp_conf_bytes(content: &[u8]) -> NamedTempFile {
    let mut f = Builder::new().suffix(".conf").tempfile().expect("tempfile");
    f.write_all(content).expect("write");
    f
}

fn tmp_conf(content: &str) -> NamedTempFile {
    tmp_conf_bytes(content.as_bytes())
}

/// Build a top-level conf string that includes the given file path and a
/// field after it so the test can assert the include itself didn't error.
fn include_with_after(path: &str) -> String {
    let escaped = path.replace('\\', "/");
    format!(
        r#"include required("{}")
a = 1"#,
        escaped
    )
}

// ---------------------------------------------------------------------------
// Carve-out cases — included empty-ish files contribute empty (no-op)
// ---------------------------------------------------------------------------

#[test]
fn s3_1_inc_1_empty_required_include_is_noop() {
    let included = tmp_conf("");
    let top = include_with_after(included.path().to_str().unwrap());
    let cfg =
        hocon::parse(&top).expect("empty included file must be no-op (Lightbend-compat #105)");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn s3_1_inc_2_whitespace_only_required_include_is_noop() {
    let included = tmp_conf("   \n  \n");
    let top = include_with_after(included.path().to_str().unwrap());
    let cfg = hocon::parse(&top).expect("whitespace-only included file must be no-op");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn s3_1_inc_3_hash_comment_only_required_include_is_noop() {
    let included = tmp_conf("# only a comment\n");
    let top = include_with_after(included.path().to_str().unwrap());
    let cfg = hocon::parse(&top).expect("hash-comment-only included file must be no-op");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn s3_1_inc_3b_slash_comment_only_required_include_is_noop() {
    let included = tmp_conf("// only a comment\n");
    let top = include_with_after(included.path().to_str().unwrap());
    let cfg = hocon::parse(&top).expect("slash-comment-only included file must be no-op");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn s3_1_inc_4_bom_only_required_include_is_noop() {
    let included = tmp_conf_bytes(&[0xEF, 0xBB, 0xBF, b'\n']);
    let top = include_with_after(included.path().to_str().unwrap());
    let cfg = hocon::parse(&top).expect("BOM-only included file must be no-op");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn s3_1_inc_5_bare_include_of_empty_file_is_noop() {
    let included = tmp_conf("");
    let escaped = included.path().to_str().unwrap().replace('\\', "/");
    let top = format!(
        r#"include "{}"
a = 1"#,
        escaped
    );
    let cfg = hocon::parse(&top).expect("bare include of empty file must be no-op");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

// ---------------------------------------------------------------------------
// Positive control — include of a file with real content must succeed
// ---------------------------------------------------------------------------

#[test]
fn s3_1_inc_pos1_non_empty_include_succeeds() {
    let included = tmp_conf("a = 1\n");
    let escaped = included.path().to_str().unwrap().replace('\\', "/");
    let top = format!(r#"include required("{}")"#, escaped);
    let cfg = hocon::parse(&top).expect("S3.1 (positive): non-empty include must succeed");
    assert_eq!(
        cfg.get_string("a").unwrap(),
        "1",
        "included field must be accessible",
    );
}
