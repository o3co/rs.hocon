//! E11 — `include package(...)` qualifier — conformance tests.
//!
//! Covers all 14 ipk* fixtures from:
//!   xx.hocon/testdata/hocon/include-package/
//!
//! This test file is only compiled when the `include-package` feature is enabled.
//! Run with: `cargo test --features include-package --test include_package_test`
//!
//! Per-impl override: ipk03 (collision) is n/a for ts.hocon but IS tested here
//! since rs.hocon uses an explicit registry with panic-on-collision semantics
//! (E11 decision 3, per-impl recommendation 2).

#![cfg(feature = "include-package")]

use hocon::Parser;

// ── Package content (inlined as string literals) ──────────────────────────────

/// ipk01: ("github.com/example/lib", "reference.conf")
const PKG_LIB_REFERENCE: &str = r#"
host = "example.com"
port = 8080
app.name = "lib"
"#;

/// ipk03 variant A: first registration
const PKG_IPK03_A: &str = r#"
version = "1.0.0"
source = "package-A"
"#;

/// ipk03 variant B: different content — triggers collision panic
const PKG_IPK03_B: &str = r#"
version = "2.0.0"
source = "package-B"
"#;

/// ipk06: ("Foo/Bar", "x.conf") — uppercase identifier
const PKG_FOO_BAR_X: &str = r#"
registered = true
"#;

/// ipk07: ("github.com/example/lib", "Reference.conf") — uppercase R
const PKG_LIB_REFERENCE_UPPER: &str = r#"
registered = true
"#;

/// ipk08: ("github.com/example/lib", "empty.conf") — empty content
const PKG_EMPTY: &str = "";

/// ipk13: ("foo", "self.conf") — self-referential cycle
const PKG_SELF: &str = r#"include package("foo", "self.conf")"#;

/// ipk14: ("foo", "a.conf") — includes b.conf
const PKG_CYCLE_A: &str = r#"include package("foo", "b.conf")"#;

/// ipk14: ("foo", "b.conf") — includes a.conf
const PKG_CYCLE_B: &str = r#"include package("foo", "a.conf")"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a Parser for ipk01: registers ("github.com/example/lib", "reference.conf").
fn parser_ipk01() -> Parser {
    Parser::new().register_package(
        "github.com/example/lib",
        "reference.conf",
        PKG_LIB_REFERENCE,
    )
}

// ── ipk01: happy-path basic include ──────────────────────────────────────────

#[test]
fn ipk01_basic_success() {
    let input = r#"include package("github.com/example/lib", "reference.conf")"#;
    let cfg = parser_ipk01()
        .parse(input)
        .expect("ipk01: should parse successfully");
    assert_eq!(cfg.get_string("host").unwrap(), "example.com");
    assert_eq!(cfg.get_i64("port").unwrap(), 8080);
    assert_eq!(cfg.get_string("app.name").unwrap(), "lib");
}

// ── ipk02: one-arg form rejected ─────────────────────────────────────────────

#[test]
fn ipk02_one_arg_rejected() {
    let input = r#"include package("github.com/example/lib/reference.conf")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "ipk02: one-arg package(...) must be a parse error (E11 decision 2)"
    );
}

// ── ipk03: collision panics ───────────────────────────────────────────────────

#[test]
#[should_panic(expected = "conflicting content")]
fn ipk03_collision_panics() {
    // First registration succeeds
    let parser =
        Parser::new().register_package("github.com/example/lib", "reference.conf", PKG_IPK03_A);
    // Second registration with different content must panic
    let _parser = parser.register_package("github.com/example/lib", "reference.conf", PKG_IPK03_B);
}

#[test]
fn ipk03_idempotent_registration_ok() {
    // Re-registering byte-identical content is idempotent (no panic)
    let _parser = Parser::new()
        .register_package("github.com/example/lib", "reference.conf", PKG_IPK03_A)
        .register_package("github.com/example/lib", "reference.conf", PKG_IPK03_A);
}

// ── ipk04: lookup miss with empty registry ────────────────────────────────────

#[test]
fn ipk04_lookup_miss() {
    let input = r#"include package("github.com/example/missing", "x.conf")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "ipk04: registry miss must be an error (E11 decision 4)"
    );
}

// ── ipk05: required(package(...)) + miss ─────────────────────────────────────

#[test]
fn ipk05_required_miss() {
    let input = r#"include required(package("github.com/example/missing", "x.conf"))"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "ipk05: required(package(...)) with registry miss must be an error (E11 decision 7)"
    );
}

// ── ipk06: case-sensitive identifier ─────────────────────────────────────────

#[test]
fn ipk06_case_sensitive_id() {
    // Register uppercase "Foo/Bar" — fixture uses lowercase "foo/bar"
    let input = r#"include package("foo/bar", "x.conf")"#;
    let result = Parser::new()
        .register_package("Foo/Bar", "x.conf", PKG_FOO_BAR_X)
        .parse(input);
    assert!(
        result.is_err(),
        "ipk06: identifier is case-sensitive; 'foo/bar' != 'Foo/Bar' (E11 decision 5)"
    );
}

// ── ipk07: case-sensitive file argument ──────────────────────────────────────

#[test]
fn ipk07_case_sensitive_file() {
    // Register "Reference.conf" (uppercase R) — fixture uses "reference.conf" (lowercase r)
    let input = r#"include package("github.com/example/lib", "reference.conf")"#;
    let result = Parser::new()
        .register_package(
            "github.com/example/lib",
            "Reference.conf",
            PKG_LIB_REFERENCE_UPPER,
        )
        .parse(input);
    assert!(
        result.is_err(),
        "ipk07: file argument is case-sensitive; 'reference.conf' != 'Reference.conf' (E11 decision 5)"
    );
}

// ── ipk08: empty registered content succeeds ─────────────────────────────────

#[test]
fn ipk08_empty_content_succeeds() {
    let input = r#"
        app = host
        include package("github.com/example/lib", "empty.conf")
    "#;
    let cfg = Parser::new()
        .register_package("github.com/example/lib", "empty.conf", PKG_EMPTY)
        .parse(input)
        .expect("ipk08: empty registered content should succeed (E11 decision 4 note)");
    assert_eq!(
        cfg.get_string("app").unwrap(),
        "host",
        "ipk08: 'app' key from outer conf must survive empty-content include"
    );
}

// ── ipk09: empty string file argument ────────────────────────────────────────

#[test]
fn ipk09_file_arg_empty() {
    let input = r#"include package("foo", "")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "ipk09: empty file argument must be a parse error (E11 decision 6)"
    );
}

// ── ipk10: absolute path file argument ───────────────────────────────────────

#[test]
fn ipk10_file_arg_absolute() {
    let input = r#"include package("foo", "/etc/passwd")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "ipk10: absolute path file argument must be a parse error (E11 decision 6)"
    );
}

// ── ipk11: traversal in file argument ────────────────────────────────────────

#[test]
fn ipk11_file_arg_traversal() {
    let input = r#"include package("foo", "../escape.conf")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "ipk11: '..' traversal in file argument must be a parse error (E11 decision 6)"
    );
}

// ── ipk12: backslash in file argument (after HOCON unescape) ─────────────────

#[test]
fn ipk12_file_arg_backslash() {
    // HOCON string "x\\y.conf" unescapes to x\y.conf (one backslash)
    // E11 decision 6: backslash is rejected (after HOCON unescape)
    let input = r#"include package("foo", "x\\y.conf")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "ipk12: backslash in file argument (after unescape) must be a parse error (E11 decision 6)"
    );
}

// ── ipk13: self-include cycle ─────────────────────────────────────────────────

#[test]
fn ipk13_cycle_self() {
    let input = r#"include package("foo", "self.conf")"#;
    let result = Parser::new()
        .register_package("foo", "self.conf", PKG_SELF)
        .parse(input);
    assert!(
        result.is_err(),
        "ipk13: self-include cycle must be an error (E11 decision 8)"
    );
    // Optionally check the error message mentions cycle
    if let Err(e) = result {
        let msg = format!("{}", e);
        // Accept if message contains "cycle", "circular", or "recursive"
        assert!(
            msg.to_lowercase().contains("cycle")
                || msg.to_lowercase().contains("circular")
                || msg.to_lowercase().contains("recursive"),
            "ipk13: error message should mention cycle/circular (got: {})",
            msg
        );
    }
}

// ── ipk14: mutual cycle ───────────────────────────────────────────────────────

#[test]
fn ipk14_cycle_mutual() {
    let input = r#"include package("foo", "a.conf")"#;
    let result = Parser::new()
        .register_package("foo", "a.conf", PKG_CYCLE_A)
        .register_package("foo", "b.conf", PKG_CYCLE_B)
        .parse(input);
    assert!(
        result.is_err(),
        "ipk14: mutual include cycle must be an error (E11 decision 8)"
    );
}

// ── Additional unit tests for file-arg validation ─────────────────────────────

#[test]
fn file_arg_dot_segment_rejected() {
    // "./x.conf" contains a "." segment — decision 6
    let input = r#"include package("foo", "./x.conf")"#;
    assert!(Parser::new().parse(input).is_err());
}

#[test]
fn file_arg_consecutive_slash_rejected() {
    // "a//b.conf" — decision 6
    let input = r#"include package("foo", "a//b.conf")"#;
    assert!(Parser::new().parse(input).is_err());
}

#[test]
fn file_arg_valid_nested_accepted_with_registration() {
    // "conf/reference.conf" — valid per decision 6
    let input = r#"include package("foo", "conf/reference.conf")"#;
    // Without registration it's a lookup miss, but parse step should accept the file arg
    let result = Parser::new().parse(input);
    // Should fail due to lookup miss, NOT parse error about file arg
    assert!(result.is_err());
    // Check it's a ResolveError (lookup miss) rather than ParseError (invalid file arg)
    match result {
        Err(hocon::HoconError::Resolve(_)) => { /* expected — lookup miss */ }
        Err(hocon::HoconError::Parse(e)) => {
            panic!(
                "file_arg_valid_nested_accepted: got ParseError for valid file arg '{}': {}",
                "conf/reference.conf", e
            );
        }
        Err(e) => panic!("unexpected error type: {}", e),
        Ok(_) => panic!("expected error for unregistered package"),
    }
}

#[test]
fn empty_identifier_rejected() {
    // E11 decision 1 note: non-empty identifier required
    let input = r#"include package("", "x.conf")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "empty identifier must be a parse error (E11 decision 1)"
    );
}

// ── Closing paren required (review fix) ──────────────────────────────────────

#[test]
fn missing_closing_paren_is_parse_error() {
    // Without closing ")", must be a parse error, not a silent success.
    let input = r#"include package("foo", "x.conf""#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "missing closing ')' must be a parse error (review fix for Codex finding)"
    );
}

#[test]
fn required_missing_closing_paren_is_parse_error() {
    // required(package(...) without closing )) must also be a parse error.
    let input = r#"include required(package("foo", "x.conf")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "missing outer closing ')' for required(package(...)) must be a parse error"
    );
}

// ── Spaced form: `include package ("id", "file")` (Copilot review fix) ──────

#[test]
fn spaced_package_qualifier_accepted() {
    // `include package ("id", "file")` — space between `package` and `(`.
    // The lexer emits `package` as a separate Unquoted token; parser must handle it.
    let input = r#"include package ("github.com/example/lib", "reference.conf")"#;
    let result = Parser::new()
        .register_package(
            "github.com/example/lib",
            "reference.conf",
            PKG_LIB_REFERENCE,
        )
        .parse(input);
    assert!(
        result.is_ok(),
        "spaced `include package (...)` must parse successfully (Copilot review fix): {:?}",
        result.err()
    );
    let config = result.unwrap();
    assert_eq!(config.get_string("host").unwrap(), "example.com");
}

#[test]
fn spaced_package_qualifier_lookup_miss() {
    // Spaced form with no registration — should fail with a ResolveError (lookup miss),
    // not fall through to a confusing standard-include error.
    let input = r#"include package ("github.com/example/lib", "reference.conf")"#;
    let result = Parser::new().parse(input);
    assert!(
        result.is_err(),
        "unregistered spaced package must be an error"
    );
    match result {
        Err(hocon::HoconError::Resolve(_)) => { /* expected — lookup miss */ }
        Err(hocon::HoconError::Parse(e)) => {
            panic!(
                "spaced package form: got ParseError instead of ResolveError: {}",
                e
            );
        }
        Err(e) => panic!("unexpected error type: {}", e),
        Ok(_) => panic!("expected error for unregistered package"),
    }
}
