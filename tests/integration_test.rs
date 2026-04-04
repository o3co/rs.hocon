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
