//! S3.1 — Empty-file guard applies to included files (rs-T1 / HOCON.md L130).
//!
//! Regression for the include-loader path: when `include required("empty.conf")` is
//! processed, the S3.1 guard must fire just as it does for top-level `parse_file`.
//! Convergent with ts Codex P2 (same issue shape in the TypeScript implementation).

use std::io::Write;
use tempfile::Builder;
use tempfile::NamedTempFile;

/// Write content to a NamedTempFile with a `.conf` suffix and return the file (keeping it alive).
///
/// Using `.conf` suffix ensures the include loader uses the exact-path code path (has_extension)
/// rather than the probe-extensions path, so the empty-file guard fires correctly.
fn tmp_conf(content: &str) -> NamedTempFile {
    let mut f = Builder::new().suffix(".conf").tempfile().expect("tempfile");
    f.write_all(content.as_bytes()).expect("write");
    f
}

/// Build a top-level conf string that includes the given file path.
fn required_include_conf(path: &str) -> String {
    // Escape backslashes for Windows paths in HOCON string literals.
    let escaped = path.replace('\\', "/");
    format!(r#"include required("{}")"#, escaped)
}

// ---------------------------------------------------------------------------
// Negative cases — included empty-ish files must error
// ---------------------------------------------------------------------------

/// s3_1_inc_1: include required of a completely empty file must error.
#[test]
fn s3_1_inc_1_empty_required_include_errors() {
    let included = tmp_conf("");
    let top = required_include_conf(included.path().to_str().unwrap());
    assert!(
        hocon::parse(&top).is_err(),
        "S3.1: include required of empty file must error (HOCON.md L130)"
    );
}

/// s3_1_inc_2: include required of a whitespace-only file must error.
#[test]
fn s3_1_inc_2_whitespace_only_required_include_errors() {
    let included = tmp_conf("   \n  \n");
    let top = required_include_conf(included.path().to_str().unwrap());
    assert!(
        hocon::parse(&top).is_err(),
        "S3.1: include required of whitespace-only file must error (HOCON.md L130)"
    );
}

/// s3_1_inc_3: include required of a comment-only file must error.
#[test]
fn s3_1_inc_3_comment_only_required_include_errors() {
    let included = tmp_conf("# only a comment\n");
    let top = required_include_conf(included.path().to_str().unwrap());
    assert!(
        hocon::parse(&top).is_err(),
        "S3.1: include required of comment-only file must error (HOCON.md L130)"
    );
}

/// s3_1_inc_4: include required of a BOM-only file must error.
#[test]
fn s3_1_inc_4_bom_only_required_include_errors() {
    let included = tmp_conf("\u{FEFF}");
    let top = required_include_conf(included.path().to_str().unwrap());
    assert!(
        hocon::parse(&top).is_err(),
        "S3.1: include required of BOM-only file must error (HOCON.md L130)"
    );
}

/// s3_1_inc_5: bare (non-required) include of an empty file must also error.
///
/// S3.1 is unconditional — "empty file is not a valid HOCON document" regardless of
/// whether the include keyword is `required(...)` or bare.  An empty *existing* file is
/// always invalid; the optional-silencing rule only applies to *missing* files.
#[test]
fn s3_1_inc_5_bare_include_of_empty_file_errors() {
    let included = tmp_conf("");
    let escaped = included.path().to_str().unwrap().replace('\\', "/");
    let top = format!(r#"include "{}""#, escaped);
    assert!(
        hocon::parse(&top).is_err(),
        "S3.1: bare include of empty *existing* file must error (HOCON.md L130)"
    );
}

// ---------------------------------------------------------------------------
// Positive case — include of a file with real content must succeed
// ---------------------------------------------------------------------------

/// s3_1_inc_pos1: include required of a file with `a = 1` must succeed.
#[test]
fn s3_1_inc_pos1_non_empty_include_succeeds() {
    let included = tmp_conf("a = 1\n");
    let top = required_include_conf(included.path().to_str().unwrap());
    let cfg = hocon::parse(&top).expect("S3.1 (positive): non-empty include must succeed");
    assert_eq!(
        cfg.get_string("a").unwrap(),
        "1",
        "S3.1 (positive): included field must be accessible"
    );
}
