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
fn test_trailing_garbage_after_braced_root() {
    let result = hocon::parse("{ a = 1 } garbage");
    assert!(
        result.is_err(),
        "expected error for trailing garbage after braced root"
    );
}

#[test]
fn test_trailing_tokens_after_braced_root() {
    let result = hocon::parse("{ a = 1 } extra tokens here");
    assert!(
        result.is_err(),
        "expected error for trailing tokens after braced root"
    );
}

#[test]
fn test_trailing_comments_after_braced_root_ok() {
    // Comments after root should be OK (lexer strips them)
    let result = hocon::parse("{ a = 1 } // comment");
    assert!(result.is_ok(), "trailing comments should be accepted");
    let result2 = hocon::parse("{ a = 1 } # comment");
    assert!(result2.is_ok(), "trailing # comments should be accepted");
}
