//! The json5 adapter's conformance battery, ported case-for-case from
//! go.hocon's adapters/json5/json5_test.go so the two implementations pin the
//! same JSON5 1.0.0 semantics (spec F3.3) with the same expected values.
//!
//! Two go tests have no direct rs equivalent, deliberately:
//! - invalid UTF-8 inside strings/comments cannot reach `parse`, whose input
//!   is `&str`; the rs boundary is `parse_file`, pinned below.
//! - go has no document-depth cap; rs caps at 128 like the crate's core
//!   parser (a Rust stack overflow aborts the process), pinned below.
#![cfg(feature = "adapters-json5")]

use hocon::adapters::json5;
use hocon::HoconValue;
use tempfile::tempdir;

fn parse(src: &str) -> hocon::Config {
    json5::parse(src, Some("test.json5")).unwrap_or_else(|e| panic!("parse({src:?}): {e}"))
}

fn parse_err(src: &str, want: &str) {
    let err = json5::parse(src, Some("test.json5"))
        .err()
        .unwrap_or_else(|| panic!("parse({src:?}): expected error containing {want:?}, got Ok"));
    assert!(
        err.message.contains(want),
        "parse({src:?}): error {:?} does not contain {want:?}",
        err.message
    );
}

// ---------------------------------------------------------------------------
// The json5.org front-page example, minus Infinity/NaN (spec F0.6 rejects
// those — pinned separately below).
// ---------------------------------------------------------------------------

#[test]
fn front_page_example() {
    let cfg = parse(
        "{
  // comments
  unquoted: 'and you can quote me on that',
  singleQuotes: 'I can use \"double quotes\" here',
  lineBreaks: \"Look, Mom! \\
No \\\\n's!\",
  hexadecimal: 0xdecaf,
  leadingDecimalPoint: .8675309, andTrailing: 8675309.,
  positiveSign: +1,
  trailingComma: 'in objects', andIn: ['arrays',],
  \"backwardsCompatible\": \"with JSON\",
}",
    );

    for (path, want) in [
        ("unquoted", "and you can quote me on that"),
        ("singleQuotes", "I can use \"double quotes\" here"),
        ("lineBreaks", "Look, Mom! No \\n's!"),
        ("trailingComma", "in objects"),
        ("backwardsCompatible", "with JSON"),
    ] {
        assert_eq!(cfg.get_string(path).unwrap(), want, "{path}");
    }
    assert_eq!(cfg.get_i64("hexadecimal").unwrap(), 0xdecaf);
    assert_eq!(cfg.get_f64("leadingDecimalPoint").unwrap(), 0.8675309);
    assert_eq!(cfg.get_f64("andTrailing").unwrap(), 8675309.0);
    assert_eq!(cfg.get_i64("positiveSign").unwrap(), 1);
    let and_in = cfg.get_list("andIn").unwrap();
    assert_eq!(and_in.len(), 1);
    match &and_in[0] {
        HoconValue::Scalar(sv) => assert_eq!(sv.raw, "arrays"),
        other => panic!("andIn[0] = {other:?}, want the string \"arrays\""),
    }
}

// ---------------------------------------------------------------------------
// Identifier keys (ES5 IdentifierName)
// ---------------------------------------------------------------------------

#[test]
fn identifier_keys() {
    let cfg = parse("{a: 1, $b: 2, _c: 3, é: 4, a1: 5}");
    for (path, want) in [("a", 1), ("\"$b\"", 2), ("_c", 3), ("\"é\"", 4), ("a1", 5)] {
        assert_eq!(cfg.get_i64(path).unwrap(), want, "{path}");
    }
}

#[test]
fn identifier_key_unicode_escape() {
    // \u0061 = 'a'; ES5 allows \u escapes inside IdentifierName.
    let cfg = parse(r"{\u0061\u0062: 7}");
    assert_eq!(cfg.get_i64("ab").unwrap(), 7);
}

#[test]
fn identifier_key_errors() {
    parse_err("{1a: 1}", "expected an object key");
    // \u0031 = '1' — a legal escape, but not a legal identifier START.
    parse_err(r"{\u0031x: 1}", "not a valid identifier character");
    parse_err(r"{\x61: 1}", r"only \uXXXX escapes");
}

// ---------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------

#[test]
fn string_escapes() {
    let cfg = parse(
        "{
  hex: \"\\x41\\x42\",
  vtab: \"a\\vb\",
  nul: \"a\\0b\",
  self: \"\\q\\'\\\"\",
  astral: \"\\uD83D\\uDE00\",
}",
    );
    for (path, want) in [
        ("hex", "AB"),
        ("vtab", "a\u{000b}b"),
        ("nul", "a\u{0000}b"),
        ("self", "q'\""),
        ("astral", "😀"),
    ] {
        assert_eq!(cfg.get_string(path).unwrap(), want, "{path}");
    }
}

#[test]
fn string_line_continuations() {
    // LF, CRLF, and LS continuations all contribute nothing.
    let cfg = parse("{a: 'x\\\ny', b: 'x\\\r\ny', c: 'x\\\u{2028}y'}");
    for path in ["a", "b", "c"] {
        assert_eq!(cfg.get_string(path).unwrap(), "xy", "{path}");
    }
}

#[test]
fn string_unescaped_separators_allowed() {
    // LS/PS are legal unescaped inside JSON5 strings (the ES5 quirk).
    let cfg = parse("{a: 'x\u{2028}y'}");
    assert_eq!(cfg.get_string("a").unwrap(), "x\u{2028}y");
}

#[test]
fn string_errors() {
    parse_err("{a: 'x\ny'}", "unescaped line terminator");
    parse_err("{a: 'oops}", "unterminated string");
    parse_err(r"{a: '\01'}", "octal escape");
    parse_err(r"{a: '\7'}", "digits cannot be escaped");
    // F3.5: a lone surrogate is an error, high or low, paired-wrong or alone.
    parse_err(r#"{a: "\uD800"}"#, "spec F3.5");
    parse_err(r#"{a: "\uD800\u0041"}"#, "spec F3.5");
    parse_err(r#"{a: "\uDE00"}"#, "spec F3.5");
}

// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

#[test]
fn numbers() {
    let cfg = parse(
        "{
  hex: 0xFF, hexneg: -0x10, hexplus: +0xA,
  min: -0x8000000000000000, max: 0x7FFFFFFFFFFFFFFF,
  lead: .5, trail: 5., plus: +5, exp: 1e3, negzero: -0,
}",
    );
    assert_eq!(cfg.get_i64("hex").unwrap(), 255);
    assert_eq!(cfg.get_i64("hexneg").unwrap(), -16);
    assert_eq!(cfg.get_i64("hexplus").unwrap(), 10);
    assert_eq!(cfg.get_i64("min").unwrap(), i64::MIN);
    assert_eq!(cfg.get_i64("max").unwrap(), i64::MAX);
    assert_eq!(cfg.get_f64("lead").unwrap(), 0.5);
    assert_eq!(cfg.get_f64("trail").unwrap(), 5.0);
    assert_eq!(cfg.get_i64("plus").unwrap(), 5);
    assert_eq!(cfg.get_f64("exp").unwrap(), 1000.0);
    assert_eq!(cfg.get_i64("negzero").unwrap(), 0);
}

#[test]
fn number_errors() {
    // F0.5: integers that do not fit in i64 are errors, not silent floats.
    parse_err("{a: 0x10000000000000000}", "spec F0.5");
    parse_err("{a: -0x8000000000000001}", "spec F0.5");
    parse_err("{a: 9223372036854775808}", "spec F0.5");
    parse_err("{a: 0x}", "hex literal needs at least one digit");
    // F0.6: Infinity and NaN in every spelling.
    for lit in ["Infinity", "-Infinity", "+Infinity", "NaN", "-NaN", "+NaN"] {
        parse_err(&format!("{{a: {lit}}}"), "spec F0.6");
    }
    // A longer identifier that merely STARTS with those spellings is not the
    // F0.6 case — it errors as an unexpected token / malformed number.
    parse_err("{a: Infinityx}", "unexpected character");
    parse_err("{a: NaNx}", "unexpected character");
    parse_err("{a: -Infinityx}", "malformed number");
}

/// A literal whose magnitude falls outside f64's range: overflow saturates to
/// ±Inf, which F0.6 rejects, but underflow is the representable value 0 — the
/// dialect owner (JS `Number`) and the go/ts/py siblings all read `1e-400` as
/// 0, and go's strconv.ParseFloat only errors on the overflow side.
#[test]
fn float_range_edges() {
    parse_err("{a: 1e999}", "malformed number");
    parse_err("{a: -1e999}", "malformed number");
    assert_eq!(parse("{a: 1e-400}").get_f64("a").unwrap(), 0.0);
    assert_eq!(parse("{a: -1e-400}").get_f64("a").unwrap(), 0.0);
    // The smallest denormal is in range and survives exactly.
    assert_eq!(parse("{a: 5e-324}").get_f64("a").unwrap(), 5e-324);
}

// ---------------------------------------------------------------------------
// Comments, whitespace, structure
// ---------------------------------------------------------------------------

#[test]
fn comments_and_whitespace() {
    // The LS after "line comment" TERMINATES the // comment (deliberately
    // different from jsonc, whose dialect owner ends comments at LF/CR only),
    // so `a: 1,` on the same source line is live. NBSP and EM SPACE (Zs) are
    // whitespace.
    let cfg = parse(
        "{\n  // line comment\u{2028} a: 1,\n  /* block\n comment */ b: 2,\u{00a0}c:\u{2003}3\n}",
    );
    for (path, want) in [("a", 1), ("b", 2), ("c", 3)] {
        assert_eq!(cfg.get_i64(path).unwrap(), want, "{path}");
    }
}

#[test]
fn comment_and_structure_errors() {
    parse_err("{a: 1} /* open", "unterminated /* comment");
    parse_err("{a: 1} }", "unexpected content after top-level value");
    parse_err("{a: 1", "unterminated object");
    parse_err("[1, 2", "unterminated array");
    parse_err("[,1]", "unexpected character");
    parse_err("[1,,2]", "unexpected character");
    parse_err("{a 1}", "expected ':'");
    // F0.3: the root must be an object.
    parse_err("[1, 2]", "spec F0.3");
    parse_err("\"just a string\"", "spec F0.3");
}

/// Trailing whitespace and comments after the value are fine (only content is
/// an error).
#[test]
fn trailing_trivia_accepted() {
    let cfg = parse("{a: 1} // done\n/* and a block */\n\n");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

/// Errors carry the origin and a 1-based line/col so a reader can find the
/// spot. (go pins the same shape through its origin wrapper.)
#[test]
fn errors_name_the_origin_and_the_line() {
    let err = json5::parse("{\na: 'x\ny'}", Some("test.json5")).unwrap_err();
    assert!(
        err.message.contains("json5: test.json5:"),
        "{}",
        err.message
    );
    assert!(err.message.contains("line 2"), "{}", err.message);

    let err = json5::parse("{", None).unwrap_err();
    assert!(err.message.contains("json5: document:"), "{}", err.message);
}

// ---------------------------------------------------------------------------
// Duplicate keys (spec F0.7)
// ---------------------------------------------------------------------------

#[test]
fn duplicate_keys_follow_hocon_semantics() {
    // Two objects merge…
    let cfg = parse("{a: {x: 1, shared: {p: 1}}, a: {y: 2, shared: {q: 2}}}");
    assert_eq!(cfg.get_i64("a.x").unwrap(), 1);
    assert_eq!(cfg.get_i64("a.y").unwrap(), 2);
    assert_eq!(cfg.get_i64("a.shared.p").unwrap(), 1);
    assert_eq!(cfg.get_i64("a.shared.q").unwrap(), 2);

    // …anything else is last-wins.
    let cfg = parse("{a: 1, a: 2}");
    assert_eq!(cfg.get_i64("a").unwrap(), 2);
    let cfg = parse("{a: {x: 1}, a: 2}");
    assert_eq!(cfg.get_i64("a").unwrap(), 2, "scalar over object");
}

// ---------------------------------------------------------------------------
// BOM and encoding (spec F0.9, S1.1 posture)
// ---------------------------------------------------------------------------

#[test]
fn bom_stripped_leading_and_whitespace_interior() {
    let cfg = parse("\u{feff}{a: \u{feff}1}");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

/// go pins that invalid UTF-8 is rejected wherever it hides; in rs the type
/// system moves that boundary to `parse_file` — `parse` takes `&str`, which
/// cannot hold invalid bytes — so the file entry point is what gets pinned.
#[test]
fn parse_file_rejects_invalid_utf8() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bad.json5");
    std::fs::write(&path, b"{a: \"\xff\"}").unwrap();
    let err = json5::parse_file(&path).unwrap_err();
    assert!(err.message.contains("bad.json5"), "{}", err.message);
}

// ---------------------------------------------------------------------------
// Depth (rs-specific: the cap the core parser also enforces)
// ---------------------------------------------------------------------------

/// go has no depth cap; in Rust a stack overflow is SIGABRT, so the adapter
/// refuses deep nesting with the core parser's own limit instead of aborting
/// the process.
#[test]
fn deep_nesting_is_an_error_not_an_abort() {
    let deep = |n: usize| format!("{}1{}", "{\"a\":".repeat(n), "}".repeat(n));
    assert!(json5::parse(&deep(128), None).is_ok(), "128 must fit");
    let err = json5::parse(&deep(129), None).unwrap_err();
    assert!(
        err.message.contains("nests deeper than 128"),
        "{}",
        err.message
    );
    // Deep enough to have aborted without the cap, even on a small stack.
    assert!(json5::parse(&deep(5000), None).is_err());
}

// ---------------------------------------------------------------------------
// Merge with a HOCON document (the adapter's purpose)
// ---------------------------------------------------------------------------

#[test]
fn with_fallback_merge() {
    let base = json5::parse("{db: {host: 'localhost', port: 5432}}", Some("base.json5")).unwrap();
    let opts = hocon::ParseOptions::defaults().with_resolve_substitutions(false);
    let cfg =
        hocon::parse_string_with_options("db { host = db.example.com }\nurl = ${db.host}", opts)
            .unwrap();
    let merged = cfg
        .with_fallback(&base)
        .resolve(hocon::ResolveOptions::defaults())
        .unwrap();
    assert_eq!(merged.get_string("db.host").unwrap(), "db.example.com");
    assert_eq!(merged.get_i64("db.port").unwrap(), 5432);
    assert_eq!(merged.get_string("url").unwrap(), "db.example.com");
}

#[test]
fn parse_file_uses_path_as_origin() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("conf.json5");
    std::fs::write(&path, "{a: [}").unwrap();
    let err = json5::parse_file(&path).unwrap_err();
    assert!(
        err.message.contains("conf.json5"),
        "expected the error to name the file: {}",
        err.message
    );
}
