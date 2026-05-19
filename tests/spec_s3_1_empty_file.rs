//! S3.1 — Empty file is invalid (HOCON.md L130).
//!
//! RED tests: these must FAIL until the S3.1 empty-stream guard is added to
//! `parse_with_env` and `parse_file_with_env` in `src/lib.rs`.

/// s3_1_1: completely empty string must error.
#[test]
fn s3_1_1_empty_string() {
    assert!(
        hocon::parse("").is_err(),
        "S3.1: empty string must be a parse error (HOCON.md L130)"
    );
}

/// s3_1_2: whitespace-only string must error.
#[test]
fn s3_1_2_whitespace_only() {
    assert!(
        hocon::parse("   \n  ").is_err(),
        "S3.1: whitespace-only input must be a parse error (HOCON.md L130)"
    );
}

/// s3_1_3: newlines-only must error.
#[test]
fn s3_1_3_newlines_only() {
    assert!(
        hocon::parse("\n\n\n").is_err(),
        "S3.1: newlines-only input must be a parse error (HOCON.md L130)"
    );
}

/// s3_1_4: comment-only must error (comment has no semantic content).
#[test]
fn s3_1_4_comment_only() {
    assert!(
        hocon::parse("# only a comment\n").is_err(),
        "S3.1: comment-only input must be a parse error (HOCON.md L130)"
    );
}

/// s3_1_5: BOM-only must error.
#[test]
fn s3_1_5_bom_only() {
    assert!(
        hocon::parse("\u{FEFF}").is_err(),
        "S3.1: BOM-only input must be a parse error (HOCON.md L130)"
    );
}

/// s3_1_6: mixed whitespace + comment must error.
#[test]
fn s3_1_6_mixed_ws_comment() {
    assert!(
        hocon::parse("  # comment\n  \n").is_err(),
        "S3.1: mixed whitespace+comment input must be a parse error (HOCON.md L130)"
    );
}

/// s3_1_pos1: explicit empty object `{}` must succeed (not trigger empty guard).
#[test]
fn s3_1_pos1_explicit_empty_object() {
    assert!(
        hocon::parse("{}").is_ok(),
        "S3.1 (positive): explicit empty object must succeed"
    );
}

/// s3_1_pos2: single-field document must succeed.
#[test]
fn s3_1_pos2_single_field() {
    assert!(
        hocon::parse("a = 1").is_ok(),
        "S3.1 (positive): 'a = 1' must succeed"
    );
}

/// s3_1_pos3: comment followed by real content must succeed.
#[test]
fn s3_1_pos3_comment_then_field() {
    assert!(
        hocon::parse("# comment\na = 1").is_ok(),
        "S3.1 (positive): comment + field must succeed"
    );
}
