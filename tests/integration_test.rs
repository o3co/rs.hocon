use hocon::parse;
use std::collections::HashMap;

/// Create (and return) a temporary directory for tests.
/// The caller is responsible for cleanup via `std::fs::remove_dir_all`.
fn test_tmp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hocon_test_{}", name));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn parse_simple_config() {
    let config = parse("host = \"localhost\"\nport = 8080").unwrap();
    assert_eq!(config.get_string("host").unwrap(), "localhost");
    assert_eq!(config.get_i64("port").unwrap(), 8080);
}

#[test]
fn parse_nested_config() {
    let config = parse(
        r#"
        server {
            host = "localhost"
            port = 8080
        }
    "#,
    )
    .unwrap();
    assert_eq!(config.get_string("server.host").unwrap(), "localhost");
    assert_eq!(config.get_i64("server.port").unwrap(), 8080);
}

#[test]
fn parse_with_substitutions() {
    let config = parse(
        r#"
        host = "localhost"
        url = "http://"${host}":8080"
    "#,
    )
    .unwrap();
    assert_eq!(config.get_string("url").unwrap(), "http://localhost:8080");
}

#[test]
fn parse_with_env_fallback() {
    let config = hocon::parse_with_env("port = 50051\nport = ${?GRPC_PORT}", &{
        let mut m = HashMap::new();
        m.insert("GRPC_PORT".into(), "9090".into());
        m
    })
    .unwrap();
    assert_eq!(config.get_string("port").unwrap(), "9090");
}

#[test]
fn parse_with_optional_substitution_fallback() {
    let config =
        hocon::parse_with_env("port = 50051\nport = ${?GRPC_PORT}", &HashMap::new()).unwrap();
    assert_eq!(config.get_i64("port").unwrap(), 50051);
}

#[test]
fn parse_with_deep_merge() {
    let config = parse(
        r#"
        server { host = "a" }
        server { port = 8080 }
    "#,
    )
    .unwrap();
    assert_eq!(config.get_string("server.host").unwrap(), "a");
    assert_eq!(config.get_i64("server.port").unwrap(), 8080);
}

#[test]
fn parse_with_arrays() {
    let config = parse("list = [1, 2, 3]").unwrap();
    let list = config.get_list("list").unwrap();
    assert_eq!(list.len(), 3);
}

#[test]
fn parse_with_plus_equals() {
    let config = parse("list = [1, 2]\nlist += 3").unwrap();
    let list = config.get_list("list").unwrap();
    assert_eq!(list.len(), 3);
}

#[test]
fn parse_with_comments() {
    let config = parse(
        r#"
        # this is a comment
        host = "localhost" // inline comment
        port = 8080
    "#,
    )
    .unwrap();
    assert_eq!(config.get_string("host").unwrap(), "localhost");
    assert_eq!(config.get_i64("port").unwrap(), 8080);
}

#[test]
fn parse_with_triple_quoted_string() {
    let config = parse(
        r#"
        msg = """
hello
world"""
    "#,
    )
    .unwrap();
    assert_eq!(config.get_string("msg").unwrap(), "hello\nworld");
}

#[test]
fn parse_bool_coercion() {
    let config = parse(
        r#"
        a = true
        b = "false"
        c = "yes"
        d = "OFF"
    "#,
    )
    .unwrap();
    assert_eq!(config.get_bool("a").unwrap(), true);
    assert_eq!(config.get_bool("b").unwrap(), false);
    assert_eq!(config.get_bool("c").unwrap(), true);
    assert_eq!(config.get_bool("d").unwrap(), false);
}

#[test]
fn parse_with_fallback() {
    let c1 = parse("host = \"prod\"").unwrap();
    let c2 = parse("host = \"dev\"\nport = 8080").unwrap();
    let merged = c1.with_fallback(&c2);
    assert_eq!(merged.get_string("host").unwrap(), "prod");
    assert_eq!(merged.get_i64("port").unwrap(), 8080);
}

#[test]
fn parse_dot_notation() {
    let config = parse("a.b.c = 1").unwrap();
    assert_eq!(config.get_i64("a.b.c").unwrap(), 1);
}

#[test]
fn parse_self_referential_substitution() {
    let config = parse("path = \"/usr\"\npath = ${path}:/extra").unwrap();
    let path = config.get_string("path").unwrap();
    assert!(path.contains("/usr"));
    assert!(path.contains("/extra"));
}

#[test]
fn test_braced_root_object_concat() {
    let cfg = hocon::parse("{ a = 1 } { b = 2 }").unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
    assert_eq!(cfg.get_i64("b").unwrap(), 2);
}

#[test]
fn test_braced_root_with_trailing_fields() {
    let cfg = hocon::parse("{ a = 1 }\nb = 2").unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
    assert_eq!(cfg.get_i64("b").unwrap(), 2);
}

#[test]
fn test_trailing_comments_after_braced_root_ok() {
    // Comments after root should be OK (lexer strips them)
    let result = hocon::parse("{ a = 1 } // comment");
    assert!(result.is_ok(), "trailing comments should be accepted");
    let result2 = hocon::parse("{ a = 1 } # comment");
    assert!(result2.is_ok(), "trailing # comments should be accepted");
}

// Task 14: quoted path segments in Config lookups
#[test]
fn test_quoted_path_lookup() {
    let cfg = hocon::parse(r#""a.b" = 1"#).unwrap();
    assert!(cfg.has(r#""a.b""#));
    assert_eq!(cfg.get_i64(r#""a.b""#).unwrap(), 1);
}

#[test]
fn test_nested_quoted_path_lookup() {
    let cfg = hocon::parse(r#"server { "web.api" { port = 8080 } }"#).unwrap();
    assert_eq!(cfg.get_i64(r#"server."web.api".port"#).unwrap(), 8080);
}

// Task 13: parse_bytes fractional number support
#[test]
fn test_parse_bytes_fractional() {
    let cfg = hocon::parse("size = 0.5M").unwrap();
    let bytes = cfg.get_bytes("size").unwrap();
    assert_eq!(bytes, 500_000);
}

#[test]
fn test_parse_bytes_fractional_binary() {
    let cfg = hocon::parse("size = 1.5MiB").unwrap();
    let bytes = cfg.get_bytes("size").unwrap();
    assert_eq!(bytes, 1_572_864);
}

// Task 12: duration parsing missing units
#[test]
fn test_duration_missing_units() {
    let tests = vec![
        ("dur = 1 milli", "dur", 1_000_000u128),
        ("dur = 2000 micros", "dur", 2_000_000u128),
        ("dur = 500 nano", "dur", 500u128),
        ("dur = 500 nanos", "dur", 500u128),
        ("dur = 1 nanosecond", "dur", 1u128),
        ("dur = 1 microsecond", "dur", 1_000u128),
        ("dur = 1 millis", "dur", 1_000_000u128),
        ("dur = 1 millisecond", "dur", 1_000_000u128),
        ("dur = 1w", "dur", 604_800_000_000_000u128),
    ];
    for (input, path, expected_nanos) in tests {
        let cfg = hocon::parse(input).unwrap();
        let dur = cfg.get_duration(path).unwrap();
        assert_eq!(
            dur.as_nanos(),
            expected_nanos,
            "failed for input: {}",
            input
        );
    }
}

// Task 11: get_string coercion for non-string scalars
#[test]
fn test_get_string_coerces_int() {
    let cfg = hocon::parse("port = 8080").unwrap();
    assert_eq!(cfg.get_string("port").unwrap(), "8080");
}

#[test]
fn test_get_string_coerces_float() {
    let cfg = hocon::parse("ratio = 3.14").unwrap();
    assert_eq!(cfg.get_string("ratio").unwrap(), "3.14");
}

#[test]
fn test_get_string_coerces_bool() {
    let cfg = hocon::parse("enabled = true").unwrap();
    assert_eq!(cfg.get_string("enabled").unwrap(), "true");
}

#[test]
fn test_get_string_coerces_null() {
    let cfg = hocon::parse("val = null").unwrap();
    assert_eq!(cfg.get_string("val").unwrap(), "null");
}

// Task 10: object concatenation deep-merge
#[test]
fn test_object_concat_deep_merge() {
    let cfg = hocon::parse(r#"a = {x: {y: 1}} {x: {z: 2}}"#).unwrap();
    assert_eq!(cfg.get_i64("a.x.y").unwrap(), 1);
    assert_eq!(cfg.get_i64("a.x.z").unwrap(), 2);
}

#[test]
fn test_object_concat_deep_merge_multiple() {
    let cfg = hocon::parse(r#"a = {nested: {a: 1}} {nested: {b: 2}} {nested: {c: 3}}"#).unwrap();
    assert_eq!(cfg.get_i64("a.nested.a").unwrap(), 1);
    assert_eq!(cfg.get_i64("a.nested.b").unwrap(), 2);
    assert_eq!(cfg.get_i64("a.nested.c").unwrap(), 3);
}

#[test]
fn test_stray_brace_after_root() {
    assert!(hocon::parse("{ a = 1 } }").is_err());
}

#[test]
fn test_parse_bytes_overflow_returns_none() {
    // Fractional value that overflows i64 should return error, not bogus value
    let cfg = hocon::parse("size = 99999999999999999.0TiB").unwrap();
    assert!(cfg.get_bytes("size").is_err());
}

#[test]
fn test_unterminated_quoted_path_fallback() {
    // Unterminated quoted path should fall back to literal (no panic)
    let cfg = hocon::parse("a = 1").unwrap();
    assert!(cfg.get_i64(r#""unterminated"#).is_err());
}

// Fix 1: include required(file("...")) form
#[test]
fn test_include_required_file_form() {
    let dir = test_tmp_dir("required_file_form");
    let conf = dir.join("base.conf");
    std::fs::write(&conf, "x = 1").unwrap();

    let path_str = conf.display().to_string().replace('\\', "\\\\");
    let input = format!(r#"include required(file("{}"))"#, path_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("x").unwrap(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// Fix 2: include required (file("...")) with space before paren
#[test]
fn test_include_required_space_file_form() {
    let dir = test_tmp_dir("required_space_file_form");
    let conf = dir.join("spaced.conf");
    std::fs::write(&conf, "y = 42").unwrap();

    let path_str = conf.display().to_string().replace('\\', "\\\\");
    let input = format!(r#"include required (file("{}"))"#, path_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("y").unwrap(), 42);
    let _ = std::fs::remove_dir_all(&dir);
}

// Task 1c/2c: include required() support
#[test]
fn test_include_required_missing_file_errors() {
    let result = hocon::parse(r#"include required("nonexistent.conf")"#);
    assert!(
        result.is_err(),
        "required include of missing file should error"
    );
}

#[test]
fn test_include_required_file_form_missing_errors() {
    let result = hocon::parse(r#"include required(file("nonexistent.conf"))"#);
    assert!(
        result.is_err(),
        "required include with file() form of missing file should error"
    );
}

#[test]
fn test_include_required_existing_file_ok() {
    let dir = test_tmp_dir("required_existing");
    let conf = dir.join("required_base.conf");
    std::fs::write(&conf, "req_key = 42\n").unwrap();
    let path_str = conf.display().to_string().replace('\\', "/");
    let content = format!("include required(\"{}\")\nextra = 1", path_str);
    let result = hocon::parse(&content);
    assert!(
        result.is_ok(),
        "required include of existing file should succeed: {:?}",
        result.err()
    );
    let cfg = result.unwrap();
    assert_eq!(cfg.get_i64("req_key").unwrap(), 42);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_include_optional_missing_file_ok() {
    let result = hocon::parse("include \"nonexistent.conf\"\na = 1");
    assert!(
        result.is_ok(),
        "optional include of missing file should succeed"
    );
    let cfg = result.unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

// Task 3c: parse errors in existing included files must propagate
#[test]
fn test_include_probing_propagates_parse_error() {
    let dir = test_tmp_dir("probing_parse_error");
    let broken_path = dir.join("broken.conf");
    std::fs::write(&broken_path, "{ invalid = }").unwrap();

    let stem = dir.join("broken");
    let path_str = stem.display().to_string().replace('\\', "/");
    let input = format!(r#"include "{}""#, path_str);
    let result = hocon::parse(&input);
    assert!(
        result.is_err(),
        "parse error in included file should propagate"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Task 4c: url() and classpath() include forms must produce errors
#[test]
fn test_include_url_not_supported() {
    let result = hocon::parse(r#"include url("http://example.com/config")"#);
    assert!(result.is_err(), "include url(...) should return an error");
}

#[test]
fn test_include_classpath_not_supported() {
    let result = hocon::parse(r#"include classpath("reference.conf")"#);
    assert!(
        result.is_err(),
        "include classpath(...) should return an error"
    );
}

// Task 2c: unknown escape sequences should error
#[test]
fn test_unknown_escape_sequence_error() {
    let result = hocon::parse(r#"key = "hello\qworld""#);
    assert!(result.is_err(), "unknown escape \\q should error");
}

#[test]
fn test_unknown_escape_a_error() {
    let result = hocon::parse(r#"key = "\a""#);
    assert!(result.is_err(), "unknown escape \\a should error");
}

// Task 4: Debug and Clone derives for Config
#[test]
fn test_config_debug() {
    let cfg = hocon::parse("a = 1").unwrap();
    let debug_str = format!("{:?}", cfg);
    assert!(!debug_str.is_empty(), "Debug output should not be empty");
}

#[test]
fn test_config_clone() {
    let cfg = hocon::parse("a = 1").unwrap();
    let cloned = cfg.clone();
    assert_eq!(cloned.get_i64("a").unwrap(), 1);
}

#[test]
fn test_config_partial_eq() {
    let cfg1 = hocon::parse("a = 1").unwrap();
    let cfg2 = hocon::parse("a = 1").unwrap();
    assert_eq!(cfg1, cfg2);
}

#[test]
fn unquoted_forbids_spec_special_chars() {
    let specials = ['?', '!', '@', '*', '&', '^', '\\'];
    for ch in &specials {
        let input = format!("key = foo{}bar", ch);
        assert!(
            hocon::parse(&input).is_err(),
            "char '{}' should be rejected in unquoted strings, but parsed successfully",
            ch,
        );
    }
}

#[test]
fn parse_error_is_hocon_error_parse_variant() {
    let result = hocon::parse("{ unterminated");
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        hocon::HoconError::Parse(pe) => {
            assert!(
                pe.line > 0 && pe.col > 0,
                "should have position info (line and col)"
            );
        }
        other => panic!("expected HoconError::Parse, got {:?}", other),
    }
}

#[test]
fn resolve_error_is_hocon_error_resolve_variant() {
    let result = hocon::parse("a = ${missing.required.key}");
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        hocon::HoconError::Resolve(re) => {
            assert!(!re.path.is_empty(), "should have substitution path");
        }
        other => panic!("expected HoconError::Resolve, got {:?}", other),
    }
}

#[test]
fn io_error_is_hocon_error_io_variant() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "hocon_test_nonexistent_{}.conf",
        std::process::id()
    ));
    let result = hocon::parse_file(&path);
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        hocon::HoconError::Io(io_err) => {
            assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound);
        }
        other => panic!("expected HoconError::Io, got {:?}", other),
    }
}

#[test]
fn test_unterminated_triple_quoted_string_errors() {
    let result = hocon::parse(r#"a = """unterminated"#);
    assert!(
        result.is_err(),
        "expected error for unterminated triple-quoted string"
    );
}

// =============================================================================
// Spec compliance Phase 1 (issue #60): parser-level comma rules and full-parse
// items. See src/lexer.rs (spec compliance block) for the lexer-level tests and
// the convention comment explaining the #[ignore] pattern.
// =============================================================================

// --- S2.3: comment markers inside quoted strings are literal (full-parse) ----
// Spec L126. The lexer already handles this correctly (the quoted-string scanner
// runs to the closing '"' without treating '//' or '#' as comment starters).
// This test verifies the end-to-end path through parse().
#[test]
fn s2_3_comment_markers_in_quoted_values_are_literal() {
    let cfg = parse(r#"url = "http://example.com""#).unwrap();
    assert_eq!(cfg.get_string("url").unwrap(), "http://example.com");

    let cfg = parse("note = \"# not a comment\"").unwrap();
    assert_eq!(cfg.get_string("note").unwrap(), "# not a comment");
}

// --- S5.2: single trailing comma is allowed and ignored ----------------------
// Spec L155. A single trailing comma after the last element/field must be
// accepted and must not produce an extra element/field.
// rs.hocon: parse_array() advances past a trailing comma, then the loop head
// sees ']' and breaks — the phantom-element path never runs. ✅
#[test]
fn s5_2_single_trailing_comma_in_array_allowed() {
    let cfg = parse("list = [1, 2, 3,]").unwrap();
    // Exactly 3 elements; trailing comma must not produce a 4th.
    let items = cfg.get_list("list").unwrap();
    assert_eq!(
        items.len(),
        3,
        "trailing comma must not produce an extra element"
    );
}

#[test]
fn s5_2_single_trailing_comma_in_object_allowed() {
    // parse_object() advances past the trailing comma, then sees '}' and breaks.
    let cfg = parse("{ a = 1, b = 2, }").unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
    assert_eq!(cfg.get_i64("b").unwrap(), 2);
}

// --- S5.3: two trailing commas is invalid ------------------------------------
// Spec L160. [1,2,3,,] must be rejected.
// rs.hocon: after advancing past the first trailing comma, parse_value() is
// called for the "next" element; it immediately hits the second Comma and tries
// to return an empty Concat — the parser propagates an "expected value" error. ✅
#[test]
fn s5_3_two_trailing_commas_in_array_rejected() {
    assert!(
        parse("list = [1, 2, 3,,]").is_err(),
        "two trailing commas in array must be a parse error per HOCON L160"
    );
}

#[test]
fn s5_3_two_trailing_commas_in_object_rejected() {
    // parse_object(): second Comma sits at key position; parse_key() errors.
    assert!(
        parse("{ a = 1, b = 2,, }").is_err(),
        "two trailing commas in object must be a parse error per HOCON L160"
    );
}

// --- S5.4: leading comma is invalid ------------------------------------------
// Spec L161. [,1,2,3] must be rejected.
// rs.hocon: parse_array() calls parse_value() before the first element;
// parse_value() sees Comma and returns an empty Concat → "expected value". ✅
// For objects: parse_key() sees Comma at the very first position and errors.
#[test]
fn s5_4_leading_comma_in_array_rejected() {
    assert!(
        parse("list = [,1, 2, 3]").is_err(),
        "leading comma in array must be a parse error per HOCON L161"
    );
}

#[test]
fn s5_4_leading_comma_in_object_rejected() {
    assert!(
        parse("{ , a = 1 }").is_err(),
        "leading comma in object must be rejected per HOCON L161"
    );
}

// --- S5.5: two consecutive commas is invalid ---------------------------------
// Spec L162. [1,,2,3] must be rejected.
// rs.hocon: same mechanism as S5.4 — the second Comma triggers "expected value". ✅
#[test]
fn s5_5_two_consecutive_commas_in_array_rejected() {
    assert!(
        parse("list = [1,, 2, 3]").is_err(),
        "two consecutive commas in array must be a parse error per HOCON L162"
    );
}

// --- S5.6: same comma rules apply to object fields ---------------------------
// Spec L163. Verified above via S5.3 / S5.4 object variants and below.
// Two consecutive commas between object fields:
// parse_object(): after 'a=1' + first comma advance, the next loop iteration
// calls parse_key() with Comma as the peeked token → error. ✅
#[test]
fn s5_6_two_consecutive_commas_between_object_fields_rejected() {
    assert!(
        parse("{ a = 1,, b = 2 }").is_err(),
        "consecutive commas between object fields must be rejected per HOCON L163"
    );
}

#[test]
fn nested_include_resolves_substitutions_in_scope() {
    // test10.conf includes test09.conf inside foo{} and bar{nested{}}
    // Substitutions like ${y} inside the included file must resolve
    // within the include scope (bar.nested.y, not root y).
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/hocon/test10.conf");
    let config = hocon::parse_file(&path).unwrap_or_else(|e| panic!("parse_file failed: {}", e));

    // bar.nested.y should be 5
    assert_eq!(config.get_i64("bar.nested.y").unwrap(), 5);
    // bar.nested.b should be 5 (resolved from ${y} -> bar.nested.y)
    assert_eq!(config.get_i64("bar.nested.b").unwrap(), 5);
    // bar.nested.a should be an object with c:3 and q:10 (delayed merge)
    assert_eq!(config.get_i64("bar.nested.a.c").unwrap(), 3);
    assert_eq!(config.get_i64("bar.nested.a.q").unwrap(), 10);
}

// =============================================================================
// Spec compliance Phase 2 — concatenation, paths, +=
// Convention: for ✅ items a single #[test] documents current (correct) behavior.
// For ❌ items a _pin test (no #[ignore]) captures the current broken behavior as
// a regression guard, and a companion _spec test with
// #[ignore = "spec violation, see #NN"] asserts the spec-correct expectation.
// =============================================================================

// --- S3.2: root non-object/non-array is invalid (HOCON L131) -----------------
// Spec: a HOCON file that contains only a bare string (neither an object nor an
// array) must be rejected.  rs.hocon already returns Err for this case. ✅
#[test]
fn s3_2_root_bare_string_rejected() {
    assert!(
        parse("\"just a string\"").is_err(),
        "bare string at root (no enclosing object or array) must be a parse error per HOCON L131"
    );
    assert!(
        parse("42").is_err(),
        "bare number at root must be a parse error per HOCON L131"
    );
}

// --- S10.4: mixing arrays + objects in concat → error (HOCON L385) -----------
// Closes #65. Formerly pinned as silent-accept; fixed in Phase 6 #3b.
#[test]
fn s10_4_array_object_concat_is_error() {
    // literal array + literal object must error per HOCON L385
    assert!(
        matches!(parse("a = [1,2] {b:3}"), Err(hocon::HoconError::Resolve(_))),
        "array+object concat must raise ResolveError per HOCON L385"
    );
    assert!(
        matches!(parse("a = {b:3} [1,2]"), Err(hocon::HoconError::Resolve(_))),
        "object+array concat must raise ResolveError per HOCON L385"
    );
}

#[test]
fn s10_4_subst_obj_plus_array_is_error() {
    // S10.19: substitution-resolved object mixed with literal array → error
    assert!(
        matches!(
            parse("obj = { b: 2 }\na = [1] ${obj}"),
            Err(hocon::HoconError::Resolve(_))
        ),
        "array + subst-resolved-object must raise ResolveError per HOCON L385/L387"
    );
    assert!(
        matches!(
            parse("arr = [1]\na = ${arr} { b: 2 }"),
            Err(hocon::HoconError::Resolve(_))
        ),
        "subst-resolved-array + object must raise ResolveError per HOCON L385/L387"
    );
}

#[test]
fn s10_4_numeric_obj_concat_still_works() {
    // REGRESSION GUARD (S15): numeric-keyed object → array conversion still ok
    let cfg = parse("obj = {\"0\":\"x\",\"1\":\"y\"}\na = [1] ${obj}")
        .expect("numeric-keyed object concat must still succeed (S15)");
    let items = cfg.get_list("a").expect("a must be a list");
    assert_eq!(items.len(), 3, "a must have 3 elements: [1, x, y]");
}

#[test]
fn s10_4_empty_edge_array_plus_empty_object_is_error() {
    // empty object still fails numeric conversion (S15.4), so must error
    assert!(
        matches!(parse("a = [1] {}"), Err(hocon::HoconError::Resolve(_))),
        "array+empty-object must raise ResolveError (S15.4: empty object not converted)"
    );
    assert!(
        matches!(parse("a = [] {b:1}"), Err(hocon::HoconError::Resolve(_))),
        "empty-array+object must raise ResolveError per HOCON L385"
    );
}

// --- S10.7: concatenation does not span a newline (HOCON L335) ---------------
// Spec: string-value concatenation must stop at a newline.
// rs.hocon already implements this correctly. ✅
#[test]
fn s10_7_concat_does_not_span_newline() {
    // Same-line concat works
    let cfg = parse("a = foo bar").unwrap();
    assert_eq!(
        cfg.get_string("a").unwrap(),
        "foo bar",
        "same-line unquoted string concat must produce 'foo bar'"
    );
    // After a newline the second token starts a new field, not a continuation
    let cfg2 = parse("a = foo\nb = 1").unwrap();
    assert_eq!(
        cfg2.get_string("a").unwrap(),
        "foo",
        "value concat must not span a newline per HOCON L335"
    );
}

// --- S10.8: string concat allowed in field keys (HOCON L317) -----------------
// Spec L553-560: path expressions work like value concatenations, so
// `a b c : 42` is a single-element path with key "a b c". ✅ for quoted keys;
// unquoted space-separated keys are ❌.
#[test]
fn s10_8_quoted_key_with_space_allowed() {
    // A quoted string with spaces as a key is unambiguous and must be accepted.
    let cfg = parse("\"foo bar\" = 42").unwrap();
    assert_eq!(
        cfg.get_i64("foo bar").unwrap(),
        42,
        "quoted key containing a space must be accepted per HOCON L317"
    );
}

#[test]
fn s10_8_unquoted_space_key_pin() {
    // BUG: `a b c : 42` (spec example L556) should set key "a b c".
    // Currently rs.hocon rejects this as a parse error.
    assert!(
        parse("foo bar = 42").is_err(),
        "[pin] unquoted space-concat key currently rejected — update when fixed"
    );
}

#[test]
#[ignore = "spec violation: unquoted space-concat key `a b c` must be accepted per HOCON L317/L556, see #66"]
fn s10_8_unquoted_space_key_spec() {
    // Spec example (L556): `a b c : 42` is equivalent to `"a b c" : 42`
    let cfg = parse("foo bar = 42").expect("parse must succeed per HOCON L317/L556");
    assert_eq!(
        cfg.get_i64("foo bar")
            .expect("key 'foo bar' must exist per HOCON L556"),
        42,
        "unquoted space-concat key must produce key 'foo bar' per HOCON L556"
    );
}

// --- S10.13: array/object in string concat → error (HOCON L373) --------------
// Closes #67. Formerly pinned as silent-accept; fixed in Phase 6 #3b.
#[test]
fn s10_13_scalar_object_concat_is_error() {
    // scalar + object must error per HOCON L373
    assert!(
        matches!(parse("a = hello {b:1}"), Err(hocon::HoconError::Resolve(_))),
        "scalar+object in string concat must raise ResolveError per HOCON L373"
    );
    assert!(
        matches!(parse("a = {b:1} hello"), Err(hocon::HoconError::Resolve(_))),
        "object+scalar in string concat must raise ResolveError per HOCON L373"
    );
}

#[test]
fn s10_13_scalar_array_concat_is_error() {
    // scalar + array or array + scalar must error per HOCON L373
    assert!(
        matches!(parse("a = hello [1,2]"), Err(hocon::HoconError::Resolve(_))),
        "scalar+array in string concat must raise ResolveError per HOCON L373"
    );
    assert!(
        matches!(parse("a = [1,2] hello"), Err(hocon::HoconError::Resolve(_))),
        "array+scalar in string concat must raise ResolveError per HOCON L373"
    );
}

#[test]
fn s10_13_subst_resolved_array_plus_scalar_is_error() {
    // substitution-resolved array in scalar concat → error (S10.19 variant)
    assert!(
        matches!(
            parse("arr = [1]\na = x ${arr}"),
            Err(hocon::HoconError::Resolve(_))
        ),
        "scalar + subst-resolved-array must raise ResolveError per HOCON L373/L387"
    );
}

#[test]
fn s10_13_subst_resolved_object_plus_scalar_is_error() {
    // substitution-resolved object in scalar concat → error (S10.19 variant)
    assert!(
        matches!(
            parse("obj = { b: 1 }\na = x ${obj}"),
            Err(hocon::HoconError::Resolve(_))
        ),
        "scalar + subst-resolved-object must raise ResolveError per HOCON L373/L387"
    );
}

// --- S10.4 optional-substitution interaction (S10.19 + spec §Optional omission) --
#[test]
fn s10_4_optional_missing_mid_concat_is_error() {
    // missing piece is omitted, leaving [1] + {b:2} → array+object → error
    assert!(
        matches!(
            parse("a = [1] ${?missing} { b: 2 }"),
            Err(hocon::HoconError::Resolve(_))
        ),
        "optional-omission must not shield type-mismatch between neighbours (S10.4)"
    );
}

#[test]
fn s10_4_optional_missing_only_piece_ok() {
    // only remaining piece is [1] → no error, value is [1]
    let cfg = parse("a = [1] ${?missing}")
        .expect("optional omission of trailing piece must leave a=[1] with no error");
    let items = cfg.get_list("a").expect("a must be a list");
    assert_eq!(items.len(), 1, "a must have 1 element");
}

// --- S10.14: whitespace around obj/array substitutions is ignored (HOCON L440) --
// Spec: when a substitution resolves to an object or array, surrounding
// non-newline whitespace is stripped and the object/array is the result.
// rs.hocon already handles this correctly. ✅
#[test]
fn s10_14_whitespace_around_obj_subst_ignored() {
    // Whitespace before/after a substitution that resolves to an object is stripped;
    // the result is the object itself, not a string.
    let cfg = parse("b = {x:1}\na =   ${b}  ").unwrap();
    assert_eq!(
        cfg.get_i64("a.x").unwrap(),
        1,
        "whitespace around obj substitution must be ignored per HOCON L440"
    );
}

#[test]
fn s10_14_whitespace_around_arr_subst_ignored() {
    let cfg = parse("b = [1,2,3]\na =   ${b}  ").unwrap();
    assert_eq!(
        cfg.get_list("a").unwrap().len(),
        3,
        "whitespace around array substitution must be ignored per HOCON L440"
    );
}

// --- S10.19: subst-resolved obj + literal array → error (HOCON L385-389) ----
#[test]
fn s10_19_subst_obj_concat_literal_array_pin() {
    // BUG: ${b} resolves to object; concatenating with [1,2] must be rejected.
    assert!(
        parse("b = {x:1}\na = ${b} [1,2]").is_ok(),
        "[pin] subst-obj + literal array currently accepted — update when fixed"
    );
}

#[test]
#[ignore = "spec violation: subst-resolved object concatenated with literal array must error per HOCON L385-389, see #68"]
fn s10_19_subst_obj_concat_literal_array_spec() {
    assert!(
        parse("b = {x:1}\na = ${b} [1,2]").is_err(),
        "subst resolving to object + literal array must be a resolve-time error per HOCON L385-389"
    );
}

#[test]
fn s10_19_subst_arr_concat_literal_obj_pin() {
    // BUG: ${b} resolves to array; concatenating with {x:1} must be rejected.
    assert!(
        parse("b = [1,2]\na = ${b} {x:1}").is_ok(),
        "[pin] subst-array + literal object currently accepted — update when fixed"
    );
}

#[test]
#[ignore = "spec violation: subst-resolved array concatenated with literal object must error per HOCON L385-389, see #68"]
fn s10_19_subst_arr_concat_literal_obj_spec() {
    assert!(
        parse("b = [1,2]\na = ${b} {x:1}").is_err(),
        "subst resolving to array + literal object must be a resolve-time error per HOCON L385-389"
    );
}

// --- S11.4: `10.0foo` → path [10, 0foo] (HOCON L496) -----------------------
// Spec L496: `10.0foo` is a number then unquoted string `foo`, producing a
// two-element path with segments `10` and `0foo`. rs.hocon is compliant —
// verified by top-level-keys inspection (probe: `cfg.keys() == ["10"]`).
#[test]
fn s11_4_numeric_dot_unquoted_path() {
    let cfg = parse("10.0foo = 42").unwrap();
    assert_eq!(
        cfg.keys(),
        vec!["10"],
        "top-level key must be \"10\" (not flat \"10.0foo\") per HOCON L496"
    );
    assert_eq!(
        cfg.get_i64("10.0foo").unwrap(),
        42,
        "value must be reachable via the nested path 10.0foo"
    );
}

// --- S11.5: `foo10.0` → path [foo10, 0] (HOCON L498) -----------------------
// Spec L498: `foo10.0` is an unquoted string with a dot, producing path [foo10, 0].
#[test]
fn s11_5_unquoted_dot_numeric_path() {
    let cfg = parse("foo10.0 = 42").unwrap();
    assert_eq!(
        cfg.keys(),
        vec!["foo10"],
        "top-level key must be \"foo10\" (not flat \"foo10.0\") per HOCON L498"
    );
    assert_eq!(
        cfg.get_i64("foo10.0").unwrap(),
        42,
        "value must be reachable via the nested path foo10.0"
    );
}

// --- S11.8: path expression always stringifies (HOCON L504) -----------------
// Spec: even a single boolean/number value in a path expression is stringified.
// `true : 42` becomes key "true" → 42. rs.hocon handles this correctly. ✅
#[test]
fn s11_8_path_expression_stringifies_boolean() {
    let cfg = parse("true = 42").unwrap();
    assert_eq!(
        cfg.get_i64("true").unwrap(),
        42,
        "boolean `true` used as a path key must be stringified to \"true\" per HOCON L504"
    );
}

#[test]
fn s11_8_path_expression_stringifies_number() {
    // `3 : 42` is key "3" → 42 (not a numeric index)
    let cfg = parse("3 = 42").unwrap();
    assert_eq!(
        cfg.get_i64("3").unwrap(),
        42,
        "number `3` used as a path key must be stringified to \"3\" per HOCON L504"
    );
}

// --- S11.9: substitutions not allowed inside path expressions (HOCON L479) --
// Spec: substitutions cannot appear in path expressions (keys).
// rs.hocon already rejects this. ✅
#[test]
fn s11_9_subst_in_key_rejected() {
    assert!(
        parse("${a} = 42").is_err(),
        "substitution at start of key path must be rejected per HOCON L479"
    );
    assert!(
        parse("a.${b} = 42").is_err(),
        "substitution inside dotted key path must be rejected per HOCON L479"
    );
}

// --- S12.5: `include` may NOT begin a key path (HOCON L570) -----------------
#[test]
fn s12_5_include_as_key_pin() {
    // BUG: `include.foo = 42` is currently accepted; spec forbids `include` as the
    // first element of a path expression in a key position.
    assert!(
        parse("include.foo = 42").is_ok(),
        "[pin] include.foo currently accepted as a key — update when fixed"
    );
}

#[test]
#[ignore = "spec violation: unquoted `include` must not begin a key path per HOCON L570, see #71"]
fn s12_5_include_as_key_spec() {
    assert!(
        parse("include.foo = 42").is_err(),
        "unquoted `include` at start of a key path must be a parse error per HOCON L570"
    );
}

// --- S13b.2: `+=` on non-array prior value → error (HOCON L732) -------------
#[test]
fn s13b_2_plus_eq_on_non_array_pin() {
    // BUG: `a = 42 \n a += 1` should error because prior value is a number not an array.
    // Currently rs.hocon silently produces Array([42, 1]).
    assert!(
        parse("a = 42\na += 1").is_ok(),
        "[pin] += on non-array currently accepted — update when fixed"
    );
    assert!(
        parse("a = \"str\"\na += 1").is_ok(),
        "[pin] += on string currently accepted — update when fixed"
    );
}

#[test]
#[ignore = "spec violation: += on non-array prior value must error per HOCON L732, see #72"]
fn s13b_2_plus_eq_on_non_array_spec() {
    assert!(
        parse("a = 42\na += 1").is_err(),
        "+= on numeric prior value must be a resolve-time error per HOCON L732"
    );
    assert!(
        parse("a = \"str\"\na += 1").is_err(),
        "+= on string prior value must be a resolve-time error per HOCON L732"
    );
}

// =============================================================================
// Spec compliance Phase 3 (issue #73): substitution & include coverage (12 items)
// S13.3, S13.5, S13.9, S13.13, S13.14, S13.16, S13a.10, S13a.13,
// S14a.6, S14a.8, S14a.9, S14b.1
// =============================================================================

// --- S13.3: `${?` is exactly 3 chars — space before `?` breaks optional marker ---
// Spec L584.  `${ ?foo}` must NOT behave like `${?foo}`.  The correct `${?foo}`
// produces an optional substitution (field dropped when undefined); a space before
// `?` is not part of the optional-marker syntax and must produce a different outcome
// (parse error, or a required substitution whose path begins with whitespace).
#[test]
fn s13_3_space_before_question_differs_from_optional() {
    // ${?foo} with no definition → field is absent (optional substitution)
    let optional = hocon::parse_with_env("x = ${?foo}", &std::collections::HashMap::new())
        .expect("${?foo} should parse");
    assert!(
        optional.get("x").is_none(),
        "optional substitution with undefined var must drop the field"
    );

    // ${ ?foo} (space before ?) must NOT silently behave as optional
    // Spec says the marker is exactly the 3-char sequence `${?`.
    // Acceptable: parse error, or a required substitution that then fails resolve.
    let spaced = hocon::parse_with_env(r#"x = ${ ?foo}"#, &std::collections::HashMap::new());
    assert!(
        spaced.is_err(),
        "space-before-? form must not silently act as optional substitution; expected parse or resolve error"
    );

    // Probe with foo defined: if rs.hocon were ever to silently treat ${ ?foo}
    // as required ${foo}, the previous case could pass for the wrong reason
    // (undefined required substitution). With foo defined the only spec-correct
    // outcomes are still parse / resolve error.
    let mut env = std::collections::HashMap::new();
    env.insert("foo".to_string(), "x".to_string());
    let spaced_defined = hocon::parse_with_env(r#"x = ${ ?foo}"#, &env);
    assert!(
        spaced_defined.is_err(),
        "space-before-? form must still error when the path is defined; got Ok value"
    );
}

// --- S13.5: substitutions not parsed inside quoted strings (spec L593) ----------
#[test]
fn s13_5_no_subst_in_quoted_string() {
    let cfg = parse(r#"x = "${foo}""#).expect("parse failed");
    assert_eq!(
        cfg.get_string("x").unwrap(),
        "${foo}",
        "substitution syntax inside a quoted string must be treated as literal text"
    );
}

// --- S13.9: `null` in config blocks env var lookup (spec L618) ------------------
// Spec: if the config tree has `key = null`, an optional substitution `${?key}`
// must NOT fall back to the environment; the explicit null takes precedence.
// BUG: rs.hocon currently falls through to the env var.
#[test]
fn s13_9_null_blocks_env_var_lookup_pin() {
    // [pin] Current behaviour: rs.hocon does NOT leak the env value, but it
    // also does NOT treat null-as-missing (which would erase the field per
    // L618). Instead it resolves ${?HOME} to the explicit null scalar, so
    // `result` ends up present with value null. The spec wants the field
    // absent. Pinning the exact Some(Scalar(null)) shape catches both
    // (a) regression to env leak ("/x/y") and
    // (b) accidental progress to None (which the spec test would then catch).
    let mut env = std::collections::HashMap::new();
    env.insert("HOME".to_string(), "/x/y".to_string());
    let cfg = hocon::parse_with_env("HOME = null\nresult = ${?HOME}", &env)
        .expect("parse should succeed");
    let v = cfg.get("result").expect("[pin] result must be present");
    match v {
        hocon::HoconValue::Scalar(s) => assert_eq!(
            s.value_type,
            hocon::ScalarType::Null,
            "[pin] result must be the explicit null scalar — env value must not leak"
        ),
        other => panic!("[pin] result must be a null scalar, got {:?}", other),
    }
}

#[test]
#[ignore = "spec violation: null in config must block env fallback per HOCON L618, see #74"]
fn s13_9_null_blocks_env_var_lookup_spec() {
    let mut env = std::collections::HashMap::new();
    env.insert("HOME".to_string(), "/x/y".to_string());
    let cfg = hocon::parse_with_env("HOME = null\nresult = ${?HOME}", &env)
        .expect("parse should succeed");
    assert!(
        cfg.get("result").is_none(),
        "null in config must block env var fallback; result must be absent per HOCON L618"
    );
}

// --- S13.13: optional undefined in string concat → empty string (spec L636) -----
#[test]
fn s13_13_optional_undefined_in_string_concat_is_empty() {
    // Use an explicitly empty env so an ambient `missing` env var cannot leak in
    // and resolve the ${?missing} substitution to a real value.
    let cfg = hocon::parse_with_env(
        r#"x = "pre"${?missing}"post""#,
        &std::collections::HashMap::new(),
    )
    .expect("parse failed");
    assert_eq!(
        cfg.get_string("x").unwrap(),
        "prepost",
        "optional undefined substitution in string concat must contribute empty string"
    );
}

// --- S13.14: optional undefined in array/object concat (spec L637) --------------
// Array: [1] ${?missing} [2] → [1, 2]
// Fixed as a side effect of the S15.3 array-concat separator-skip fix (fix/s15-numeric-obj-array).
// The is_sep whitespace tokens are now discarded in the array-concat branch, eliminating artefacts.
#[test]
fn s13_14_optional_undefined_in_array_concat_spec() {
    let cfg = hocon::parse_with_env("x = [1] ${?missing} [2]", &std::collections::HashMap::new())
        .expect("parse failed");
    let items = cfg.get_list("x").unwrap();
    assert_eq!(
        items.len(),
        2,
        "array concat must collapse to exactly two elements"
    );
    // Beyond length, both elements must be the original numeric values — a
    // malformed two-element result (e.g. surviving whitespace artefacts) would
    // also fail.
    let val = |v: &hocon::HoconValue| match v {
        hocon::HoconValue::Scalar(s) => Some((s.raw.clone(), s.value_type)),
        _ => None,
    };
    assert_eq!(
        val(&items[0]),
        Some(("1".to_string(), hocon::ScalarType::Number)),
        "items[0] must be numeric 1"
    );
    assert_eq!(
        val(&items[1]),
        Some(("2".to_string(), hocon::ScalarType::Number)),
        "items[1] must be numeric 2"
    );
}

// Object: {a:1} ${?missing} {b:2} → {a:1, b:2}  (currently passes)
#[test]
fn s13_14_optional_undefined_in_object_concat() {
    let cfg = hocon::parse_with_env(
        "x = {a:1} ${?missing} {b:2}",
        &std::collections::HashMap::new(),
    )
    .expect("parse failed");
    let sub = cfg.get_config("x").expect("x must be an object");
    assert_eq!(sub.get_i64("a").unwrap(), 1);
    assert_eq!(sub.get_i64("b").unwrap(), 2);
}

// --- S13.16: substitutions only in values/elements — not in keys (spec L644) ----
#[test]
fn s13_16_substitution_in_key_is_rejected() {
    assert!(
        parse("${foo} = 1").is_err(),
        "substitution in key position must be a parse error per HOCON L644"
    );
}

// --- S13a.10: substitution memoized by instance, not by path (spec L885) --------
// This property is not externally observable from a pure black-box parse API
// (it affects evaluation order, not final value).  Marking 🤷 with a note.
// No test added; see docs/spec-compliance.md S13a.10.

// --- S13a.13: `a = ${?a}foo` resolves to "foo" (spec L841) ----------------------
// BUG: rs.hocon currently evaluates ${?a} as the (already-set) value of `a`
// ("foo") and produces "foofoo".
#[test]
fn s13a_13_optional_self_ref_concat_with_no_prior_pin() {
    // [pin] Current (broken) behaviour: ${?a} sees "foo" instead of undefined.
    // Use empty env so an ambient `a` env var cannot leak in (single-letter env
    // names like `a` are common on POSIX shells — a real-world flake risk).
    let cfg = hocon::parse_with_env("a = ${?a}foo", &std::collections::HashMap::new())
        .expect("parse failed");
    assert_eq!(
        cfg.get_string("a").unwrap(),
        "foofoo",
        "[pin] a currently resolves to \"foofoo\" — update when fixed"
    );
}

#[test]
#[ignore = "spec violation: a = ${?a}foo must resolve to \"foo\" when a has no prior value, see #76"]
fn s13a_13_optional_self_ref_concat_with_no_prior_spec() {
    let cfg = hocon::parse_with_env("a = ${?a}foo", &std::collections::HashMap::new())
        .expect("parse failed");
    assert_eq!(
        cfg.get_string("a").unwrap(),
        "foo",
        "with no prior value, the self-referencing optional subst is undefined; result must be \"foo\""
    );
}

// --- S14a.6: unquoted `include` at non-start-of-key is literal (spec L962) ------
#[test]
fn s14a_6_include_in_dotted_key_is_literal() {
    let cfg = parse("x.include = 1").expect("parse failed");
    assert_eq!(
        cfg.get_i64("x.include").unwrap(),
        1,
        "unquoted `include` that is not at the start of a key must be treated as literal"
    );
}

// --- S14a.8: no value concatenation on include argument (spec L957) -------------
#[test]
fn s14a_8_no_concatenation_on_include_arg() {
    assert!(
        parse(r#"include "a.conf" "b.conf""#).is_err(),
        "multiple strings after `include` must be a parse error per HOCON L957"
    );
}

// --- S14a.9: no substitutions in include argument (spec L959) -------------------
#[test]
fn s14a_9_no_substitution_in_include_arg() {
    assert!(
        parse("include ${path}").is_err(),
        "substitution as include argument must be a parse error per HOCON L959"
    );
}

// --- S14b.1: included root must be an object; array root → error (spec L993) ----
#[test]
fn s14b_1_array_root_include_is_error() {
    let dir = test_tmp_dir("s14b1_array_root");
    let arr_file = dir.join("arr.conf");
    std::fs::write(&arr_file, "[1, 2, 3]").unwrap();
    let path_str = arr_file.display().to_string().replace('\\', "/");
    let input = format!(r#"include "{}""#, path_str);
    let result = hocon::parse(&input);
    assert!(
        result.is_err(),
        "including a file whose root is an array must produce an error per HOCON L993"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- S15.1: numeric-keyed object → array when array context (spec L1191) ----------
//
// Implemented in fix/s15-numeric-obj-array (closes #79).
// Helper: src/numeric_array.rs::numeric_object_to_array
// Accessor site: src/config.rs::Config::get_list
// Extended fixture tests: tests/s15_fixtures.rs
#[test]
fn s15_1_num_indexed_obj_to_array_spec() {
    let cfg = hocon::parse_with_env(r#"v = {"0":"a","1":"b"}"#, &HashMap::new()).unwrap();
    let items = cfg
        .get_list("v")
        .expect("numeric-keyed object must be convertible to array per HOCON L1191");
    assert_eq!(items.len(), 2, "converted array must have 2 elements");
    match &items[0] {
        hocon::HoconValue::Scalar(sv) => assert_eq!(sv.raw, "a", "first element must be \"a\""),
        other => panic!("expected Scalar, got {:?}", other),
    }
    match &items[1] {
        hocon::HoconValue::Scalar(sv) => assert_eq!(sv.raw, "b", "second element must be \"b\""),
        other => panic!("expected Scalar, got {:?}", other),
    }
}

// --- S15.2: conversion is lazy — only when array type is requested (spec L1204) ---
//
// Laziness is preserved: get_config/get on a numeric-keyed object returns the object
// as-is; conversion only triggers from get_list (accessor-time, not parse/resolve time).
#[test]
fn s15_2_conversion_is_lazy_spec() {
    let cfg = hocon::parse_with_env(r#"v = {"0":"a","1":"b"}"#, &HashMap::new()).unwrap();
    // Object access must still work (lazy: not converted until array type is requested).
    assert!(
        cfg.get_config("v").is_ok(),
        "get_config must still succeed before conversion is triggered"
    );
    // Array access must trigger conversion.
    assert!(
        cfg.get_list("v").is_ok(),
        "get_list must trigger lazy conversion of numeric-keyed object to array"
    );
}

// --- S15.3: conversion in concatenation when list expected (spec L1210) -----------
//
// Resolver pairwise-join site: src/resolver/substitution_resolver.rs::resolve_concat
// When one side is Array and the other is Object, numeric_object_to_array fires.
#[test]
fn s15_3_conversion_in_concatenation_spec() {
    let cfg = hocon::parse_with_env(
        r#"obj = {"0":"x","1":"y"}
arr = [a] ${obj}"#,
        &HashMap::new(),
    )
    .unwrap();
    let items = cfg.get_list("arr").expect("concat produces an array");
    // Spec L1210: obj converts to ["x","y"] and flattens → ["a","x","y"] (3 elements).
    assert_eq!(
        items.len(),
        3,
        "expected ['a','x','y'] after conversion, got {:?}",
        items
    );
    let raws: Vec<&str> = items
        .iter()
        .map(|v| match v {
            hocon::HoconValue::Scalar(s) => s.raw.as_str(),
            _ => panic!("expected scalar after conversion, got {:?}", v),
        })
        .collect();
    assert_eq!(raws, vec!["a", "x", "y"]);
}

// --- S15.4: empty object NOT converted (spec L1212) --------------------------------
//
// numeric_object_to_array returns None for empty objects; get_list then errors.
// Now backed by explicit empty-guard rather than incidental pass.
#[test]
fn s15_4_empty_object_not_converted() {
    let cfg = hocon::parse_with_env(r#"v = {}"#, &HashMap::new()).unwrap();
    // Empty object must NOT be converted to an array (per HOCON L1212).
    // get_list should return an error (it is not an array, and must not become one).
    assert!(
        cfg.get_list("v").is_err(),
        "empty object must not be converted to array per HOCON L1212"
    );
}

// --- S15.5: non-integer keys ignored during conversion (spec L1214) ----------------
//
// Eligible filter in numeric_object_to_array: only ^(0|[1-9][0-9]*)$ keys count.
#[test]
fn s15_5_non_integer_keys_ignored_spec() {
    let cfg = hocon::parse_with_env(r#"v = {"0":"a","foo":"b","1":"c"}"#, &HashMap::new()).unwrap();
    // "foo" key is ignored; result must be ["a","c"].
    let items = cfg
        .get_list("v")
        .expect("mixed-key object must convert, ignoring non-integer keys per HOCON L1214");
    assert_eq!(
        items.len(),
        2,
        "only integer-keyed entries remain: [\"a\",\"c\"]"
    );
}

// --- S15.6: missing indices compacted in resulting array (spec L1216) --------------
//
// Gaps are naturally eliminated by sorting eligible (key, value) pairs and projecting.
#[test]
fn s15_6_missing_indices_compacted_spec() {
    let cfg = hocon::parse_with_env(r#"v = {"0":"a","2":"c"}"#, &HashMap::new()).unwrap();
    let items = cfg
        .get_list("v")
        .expect("sparse numeric-keyed object must convert to compacted array per HOCON L1216");
    assert_eq!(
        items.len(),
        2,
        "gaps eliminated: keys 0+2 → array of 2 elements"
    );
}

// --- S15.7: sorted by integer key value (spec L1216) --------------------------------
//
// numeric_object_to_array sorts eligible pairs by parsed integer before projecting.
#[test]
fn s15_7_sorted_by_key_value_spec() {
    let cfg = hocon::parse_with_env(r#"v = {"2":"c","0":"a"}"#, &HashMap::new()).unwrap();
    let items = cfg.get_list("v").expect(
        "out-of-order numeric-keyed object must convert sorted by integer key per HOCON L1216",
    );
    assert_eq!(items.len(), 2, "must produce 2-element array");
    // After sort by integer key: 0→"a", 2→"c" → ["a","c"]
    match &items[0] {
        hocon::HoconValue::Scalar(sv) => {
            assert_eq!(sv.raw, "a", "first element must be key-0's value")
        }
        other => panic!("expected Scalar, got {:?}", other),
    }
    match &items[1] {
        hocon::HoconValue::Scalar(sv) => {
            assert_eq!(sv.raw, "c", "second element must be key-2's value")
        }
        other => panic!("expected Scalar, got {:?}", other),
    }
}

// --- S17.5: "null" string → null when null requested (spec L1244) ------------------
//
// The spec says: "the string 'null' should be converted to a null value if the
// application specifically asks for a null value." rs.hocon has no get_null() API,
// so this conversion path is not testable at the typed-getter level.
// The underlying storage correctly distinguishes String "null" from Null scalar.
// Marked ➖ (out-of-scope for this API surface).
#[test]
fn s17_5_null_string_stored_as_string_not_null() {
    // Verify the internal representation: "null" (quoted) is stored as String, not Null.
    let cfg = hocon::parse_with_env(r#"v = "null""#, &HashMap::new()).unwrap();
    match cfg.get("v") {
        Some(hocon::HoconValue::Scalar(sv)) => {
            assert_eq!(sv.raw, "null");
            assert_eq!(
                sv.value_type,
                hocon::ScalarType::String,
                "quoted \"null\" must be stored as String scalar, not Null"
            );
        }
        other => panic!("expected Scalar, got {:?}", other),
    }
}

// --- S17.6: null → other type: error (spec L1252) ----------------------------------
//
// Partial conformance: get_i64 and get_bool on null correctly error.
// get_string on null incorrectly returns Ok("null") — bug, see #80.
#[test]
fn s17_6_null_to_numeric_and_bool_errors() {
    let cfg = hocon::parse_with_env(r#"v = null"#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_i64("v").is_err(),
        "null → i64 must error per HOCON L1252"
    );
    assert!(
        cfg.get_bool("v").is_err(),
        "null → bool must error per HOCON L1252"
    );
}

#[test]
fn s17_6_null_to_string_pin() {
    let cfg = hocon::parse_with_env(r#"v = null"#, &HashMap::new()).unwrap();
    // [pin] Buggy: get_string on a null value returns Ok("null") instead of Err.
    // The spec (L1252) requires null → any type to be an error.
    assert!(
        cfg.get_string("v").is_ok(),
        "[pin] get_string on null currently returns Ok(\"null\") — update when fixed"
    );
}

#[ignore = "spec violation: null → string must error per HOCON L1252, but get_string returns Ok(\"null\"), see #80"]
#[test]
fn s17_6_null_to_string_spec() {
    let cfg = hocon::parse_with_env(r#"v = null"#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_string("v").is_err(),
        "null → string must return an error per HOCON L1252"
    );
}

// --- S17.8: array → other type (except numeric-indexed): error (spec L1255) --------
#[test]
fn s17_8_array_to_other_type_errors() {
    let cfg = hocon::parse_with_env(r#"v = [1,2,3]"#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_string("v").is_err(),
        "array → string must error per HOCON L1255"
    );
    assert!(
        cfg.get_i64("v").is_err(),
        "array → i64 must error per HOCON L1255"
    );
    assert!(
        cfg.get_bool("v").is_err(),
        "array → bool must error per HOCON L1255"
    );
    // get_list on a plain array must still succeed (not an error, it IS an array).
    assert!(
        cfg.get_list("v").is_ok(),
        "get_list on an array must succeed"
    );
}
