//! S3.1 — An empty document is valid HOCON and parses to the empty object `{}`
//! (HOCON.md §Omit root braces L130-136).
//!
//! L130-132 ("Empty files are invalid documents") is the *JSON baseline*
//! description; the HOCON-normative sentence is L134-136: a file that does not
//! begin with `[` or `{` is parsed as if enclosed in `{}` — an empty document
//! vacuously qualifies. Confirmed by the Lightbend reference implementation,
//! whose "Empty document" error is ConfigSyntax.JSON-only.
//!
//! History: cluster 3h added an `assert_non_empty_document` guard rejecting
//! these inputs, misreading the JSON baseline as HOCON-normative — a behavior
//! regression. The posture was revoked 2026-07-23 (xx.hocon E10); these tests
//! pin the corrected behavior.

/// Assert that `input` parses successfully to an empty config.
fn assert_parses_to_empty(input: &str, label: &str) {
    let cfg = hocon::parse(input)
        .unwrap_or_else(|e| panic!("S3.1: {} must parse to an empty config, got error: {}", label, e));
    assert!(
        cfg.keys().is_empty(),
        "S3.1: {} must produce an empty config, got keys {:?}",
        label,
        cfg.keys()
    );
}

/// s3_1_1: completely empty string parses to `{}`.
#[test]
fn s3_1_1_empty_string() {
    assert_parses_to_empty("", "empty string");
}

/// s3_1_2: whitespace-only string parses to `{}`.
#[test]
fn s3_1_2_whitespace_only() {
    assert_parses_to_empty("   \n  ", "whitespace-only input");
}

/// s3_1_3: newlines-only parses to `{}`.
#[test]
fn s3_1_3_newlines_only() {
    assert_parses_to_empty("\n\n\n", "newlines-only input");
}

/// s3_1_4: comment-only parses to `{}` (comment has no semantic content).
#[test]
fn s3_1_4_comment_only() {
    assert_parses_to_empty("# only a comment\n", "comment-only input");
}

/// s3_1_5: BOM-only parses to `{}`.
#[test]
fn s3_1_5_bom_only() {
    assert_parses_to_empty("\u{FEFF}", "BOM-only input");
}

/// s3_1_6: mixed whitespace + comment parses to `{}`.
#[test]
fn s3_1_6_mixed_ws_comment() {
    assert_parses_to_empty("  # comment\n  \n", "mixed whitespace+comment input");
}

/// s3_1_pos1: explicit empty object `{}` succeeds and equals the empty-document result.
#[test]
fn s3_1_pos1_explicit_empty_object() {
    assert_parses_to_empty("{}", "explicit empty object");
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
