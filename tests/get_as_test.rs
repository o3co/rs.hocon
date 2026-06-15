#![cfg(feature = "serde")]

//! Task B (rs.hocon 1.8): `Config::get_as` — deserialize the node at a path into
//! a typed value. Unlike `get_config().deserialize()` (object subtrees only),
//! this accepts any node (object / array / scalar). Error mapping pinned in spec:
//! missing -> missing; (transitive) placeholder -> not-resolved; serde failure ->
//! ConfigError{path, message}.

use hocon::{parse, parse_string_with_options, ParseOptions};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Server {
    host: String,
    port: i64,
}

#[test]
fn get_as_object_subtree() {
    let c = parse(r#"server { host = "h", port = 1 }"#).unwrap();
    let s: Server = c.get_as("server").unwrap();
    assert_eq!(
        s,
        Server {
            host: "h".into(),
            port: 1
        }
    );
}

#[test]
fn get_as_list_at_path() {
    let c = parse("ports = [1, 2, 3]").unwrap();
    let v: Vec<i64> = c.get_as("ports").unwrap();
    assert_eq!(v, vec![1, 2, 3]);
}

#[test]
fn get_as_scalar_at_path() {
    let c = parse("port = 8080").unwrap();
    let p: i64 = c.get_as("port").unwrap();
    assert_eq!(p, 8080);
}

#[test]
fn get_as_missing_path_is_not_not_resolved() {
    let c = parse("a = 1").unwrap();
    let err = c.get_as::<i64>("nope").unwrap_err();
    assert_eq!(err.path, "nope");
    assert!(!err.is_not_resolved());
}

#[test]
fn get_as_unresolved_path_is_not_resolved() {
    let c = parse_string_with_options(
        "a = ${b}",
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let err = c.get_as::<i64>("a").unwrap_err();
    assert!(err.is_not_resolved(), "got: {}", err.message);
    assert_eq!(err.path, "a");
}

#[derive(Debug, Deserialize)]
struct Obj {
    #[allow(dead_code)]
    x: i64,
}

#[test]
fn get_as_nested_unresolved_is_not_resolved() {
    // Exercises the recursion in value_contains_placeholder: the placeholder is
    // nested under the object at `obj`, not at the looked-up node directly.
    let c = parse_string_with_options(
        "obj { x = ${b} }",
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let err = c.get_as::<Obj>("obj").unwrap_err();
    assert!(err.is_not_resolved(), "got: {}", err.message);
    assert_eq!(err.path, "obj");
}

#[test]
fn get_as_coerces_quoted_numeric_like_get_i64() {
    // Integer coercion is consistent across get_i64 / get_as / from_value:
    // a quoted float-like numeric string coerces to the integer.
    let c = parse(r#"port = "1e3""#).unwrap();
    assert_eq!(c.get_as::<i64>("port").unwrap(), 1000);
    assert_eq!(c.get_i64("port").unwrap(), 1000);
}

#[test]
fn get_as_type_mismatch_errors() {
    let c = parse(r#"port = "not_a_number""#).unwrap();
    let err = c.get_as::<i64>("port").unwrap_err();
    assert_eq!(err.path, "port");
    assert!(!err.is_not_resolved());
}
