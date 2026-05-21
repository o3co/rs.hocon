//! Cross-impl fix for go.hocon#105 — empty / whitespace-only / comment-only /
//! BOM-only INCLUDED files contribute an empty config instead of erroring
//! with S3.1's "empty file is not a valid HOCON document". Top-level parses
//! (`parse` / `parse_with_env` / `parse_file_with_options` on a top-level
//! empty file) continue to error
//! per S3.1.

use tempfile::tempdir;

#[test]
fn issue105_zero_byte_include_is_noop() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("empty.conf"), "").unwrap();
    let input = format!("include \"{}/empty.conf\"\na = 1\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn issue105_hash_comment_only_include_is_noop() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("c.conf"), "# only a comment\n# another\n").unwrap();
    let input = format!("include \"{}/c.conf\"\na = 1\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn issue105_slash_comment_only_include_is_noop() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("c.conf"), "// only a comment\n").unwrap();
    let input = format!("include \"{}/c.conf\"\na = 1\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn issue105_whitespace_only_include_is_noop() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("ws.conf"), "   \n\t\n\n").unwrap();
    let input = format!("include \"{}/ws.conf\"\na = 1\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn issue105_unicode_whitespace_only_include_is_noop() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    // NBSP (U+00A0) + en-quad (U+2000) + line separator (U+2028) + LF
    std::fs::write(dir.path().join("uws.conf"), "\u{A0}\u{2000}\u{2028}\n").unwrap();
    let input = format!("include \"{}/uws.conf\"\na = 1\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn issue105_bom_only_include_is_noop() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    // UTF-8 BOM bytes + newline
    std::fs::write(dir.path().join("bom.conf"), [0xEF, 0xBB, 0xBF, b'\n']).unwrap();
    let input = format!("include \"{}/bom.conf\"\na = 1\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn issue105_top_level_empty_still_rejected() {
    // S3.1 enforcement for top-level parses must remain intact.
    assert!(hocon::parse("").is_err(), "empty top-level must error");
    assert!(
        hocon::parse("   \n\t  ").is_err(),
        "whitespace-only top-level must error",
    );
    assert!(
        hocon::parse("# only a comment\n").is_err(),
        "hash-comment-only top-level must error",
    );
    assert!(
        hocon::parse("// only a comment\n").is_err(),
        "slash-comment-only top-level must error",
    );
}

#[test]
fn issue105_non_empty_include_still_parses() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("c.conf"), "# leading\nb = 2\n# trailing\n").unwrap();
    let input = format!("include \"{}/c.conf\"\na = 1\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
    assert_eq!(cfg.get_i64("b").unwrap(), 2);
}
