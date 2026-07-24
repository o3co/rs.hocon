//! Issue #149: `include file("…")` inside an object literal swallowed every
//! field that followed it on the same line (`aa = {include file("x"), b = 1}`
//! lost `b`), because the parser skipped "anything else on this line" after
//! the quoted path instead of consuming just the closing `)`. The bug was
//! independent of whether the included file exists; a missing optional
//! include must contribute an empty object (HOCON.md §Include semantics) and
//! leave sibling fields intact either way.

use tempfile::tempdir;

fn setup() -> (tempfile::TempDir, String) {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("exists.conf"), "x = 1\n").unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    (dir, dir_str)
}

#[test]
fn issue149_existing_file_include_keeps_trailing_field() {
    let (_d, dir) = setup();
    let input = format!("aa = {{include file(\"{}/exists.conf\"), b = \"ww\"}}\n", dir);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("aa.x").unwrap(), 1, "included content lost");
    assert_eq!(cfg.get_string("aa.b").unwrap(), "ww", "trailing field swallowed");
}

#[test]
fn issue149_missing_file_include_keeps_trailing_field() {
    let (_d, dir) = setup();
    let input = format!("aa = {{include file(\"{}/nope.conf\"), b = \"ww\"}}\n", dir);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_string("aa.b").unwrap(), "ww", "trailing field swallowed");
}

// Spaced closing parens lex as separate ")" tokens (unlike fused `))`); the
// paren-consume loop must eat all of them without touching the `, b = …`.
#[test]
fn issue149_spaced_parens_keep_trailing_field() {
    let (_d, dir) = setup();
    let input = format!(
        "aa = {{include required( file( \"{}/exists.conf\" ) ), b = \"ww\"}}\n",
        dir
    );
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("aa.x").unwrap(), 1);
    assert_eq!(cfg.get_string("aa.b").unwrap(), "ww", "trailing field swallowed");
}

// Guard (passed pre-fix too): fused `required(file("…"))` routes through the
// QuotedString+required branch, not the file() branch — kept as a regression
// guard for that adjacent code path.
#[test]
fn issue149_required_file_include_keeps_trailing_field() {
    let (_d, dir) = setup();
    let input = format!(
        "aa = {{include required(file(\"{}/exists.conf\")), b = \"ww\"}}\n",
        dir
    );
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("aa.x").unwrap(), 1);
    assert_eq!(cfg.get_string("aa.b").unwrap(), "ww", "trailing field swallowed");
}

// Fused junk after the closing paren lexes into the same Unquoted token
// (`)junk`); that token is not a pure closing paren and must surface as a
// parse error rather than being silently swallowed (Copilot review).
#[test]
fn issue149_fused_junk_after_paren_is_parse_error() {
    let (_d, dir) = setup();
    let input = format!(
        "aa = {{include file(\"{}/exists.conf\")junk, b = \"ww\"}}\n",
        dir
    );
    assert!(
        hocon::parse(&input).is_err(),
        "fused junk after ')' must be a parse error, not silently swallowed"
    );
}

// A file() include whose closing paren never appears is a parse error.
#[test]
fn issue149_missing_close_paren_is_parse_error() {
    let (_d, dir) = setup();
    let input = format!("aa = {{include file(\"{}/exists.conf\", b = \"ww\"}}\n", dir);
    assert!(
        hocon::parse(&input).is_err(),
        "missing ')' after include file path must be a parse error"
    );
}

// Guards: behaviours that already worked and must not regress.

#[test]
fn issue149_bare_include_keeps_trailing_field() {
    let (_d, dir) = setup();
    let input = format!("aa = {{include \"{}/nope.conf\", b = \"ww\"}}\n", dir);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_string("aa.b").unwrap(), "ww");
}

#[test]
fn issue149_field_before_file_include_still_survives() {
    let (_d, dir) = setup();
    let input = format!("aa = {{b = \"ww\", include file(\"{}/nope.conf\")}}\n", dir);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_string("aa.b").unwrap(), "ww");
}
