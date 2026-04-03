use hocon::parse;
use std::collections::HashMap;

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
fn test_include_required_existing_file_ok() {
    // base.conf lives in tests/testdata/; parse_file sets the base_dir correctly
    use std::path::PathBuf;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let conf = manifest.join("tests/testdata/required_base.conf");
    // Write a tiny fixture if it doesn't exist yet
    if !conf.exists() {
        std::fs::write(&conf, "req_key = 42\n").unwrap();
    }
    let content = format!("include required({:?})\nextra = 1", conf.to_str().unwrap());
    let result = hocon::parse(&content);
    assert!(
        result.is_ok(),
        "required include of existing file should succeed: {:?}",
        result.err()
    );
    let cfg = result.unwrap();
    assert_eq!(cfg.get_i64("req_key").unwrap(), 42);
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
