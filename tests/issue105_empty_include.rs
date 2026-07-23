//! Cross-impl fix for go.hocon#105 — empty / whitespace-only / comment-only /
//! BOM-only INCLUDED files contribute an empty config. Originally a narrow
//! include-path carve-out while top-level parses still rejected; since the
//! S3.1 correction (xx.hocon E10, 2026-07-23) the same rule applies
//! everywhere — an empty document parses to `{}` at top level too
//! (HOCON.md §Omit root braces L134-136).

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
fn issue105_top_level_empty_parses_to_empty_object() {
    // Corrected S3.1: top-level parity with the include path — an empty
    // document parses to {} everywhere (xx.hocon E10).
    for (input, label) in [
        ("", "empty"),
        ("   \n\t  ", "whitespace-only"),
        ("# only a comment\n", "hash-comment-only"),
        ("// only a comment\n", "slash-comment-only"),
    ] {
        let cfg = hocon::parse(input).unwrap_or_else(|e| {
            panic!("{} top-level must parse to {{}} per corrected S3.1, got error: {}", label, e)
        });
        assert!(
            cfg.keys().is_empty(),
            "{} top-level must produce an empty config, got keys {:?}",
            label,
            cfg.keys()
        );
    }
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
