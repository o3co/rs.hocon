#![cfg(feature = "serde")]

use hocon::parse;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct ServerConfig {
    host: String,
    port: i64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct AppConfig {
    server: ServerConfig,
    debug: bool,
}

#[test]
fn deserialize_flat_struct() {
    let config = parse("host = \"localhost\"\nport = 8080").unwrap();
    let server: ServerConfig = config.deserialize().unwrap();
    assert_eq!(server.host, "localhost");
    assert_eq!(server.port, 8080);
}

#[test]
fn deserialize_nested_struct() {
    let config = parse(
        r#"
        server {
            host = "localhost"
            port = 8080
        }
        debug = false
    "#,
    )
    .unwrap();
    let app: AppConfig = config.deserialize().unwrap();
    assert_eq!(app.server.host, "localhost");
    assert_eq!(app.server.port, 8080);
    assert!(!app.debug);
}

#[test]
fn deserialize_with_defaults() {
    #[derive(Debug, Deserialize)]
    struct WithDefault {
        host: String,
        #[serde(default = "default_port")]
        port: i64,
    }
    fn default_port() -> i64 {
        3000
    }

    let config = parse("host = \"localhost\"").unwrap();
    let val: WithDefault = config.deserialize().unwrap();
    assert_eq!(val.host, "localhost");
    assert_eq!(val.port, 3000);
}

#[test]
fn deserialize_vec() {
    #[derive(Debug, Deserialize)]
    struct WithList {
        items: Vec<i64>,
    }
    let config = parse("items = [1, 2, 3]").unwrap();
    let val: WithList = config.deserialize().unwrap();
    assert_eq!(val.items, vec![1, 2, 3]);
}

#[test]
fn deserialize_optional_field() {
    #[derive(Debug, Deserialize)]
    struct WithOpt {
        host: String,
        port: Option<i64>,
    }
    let config = parse("host = \"localhost\"").unwrap();
    let val: WithOpt = config.deserialize().unwrap();
    assert_eq!(val.host, "localhost");
    assert_eq!(val.port, None);
}

#[test]
fn deserialize_string_numbers_coerced() {
    #[derive(Debug, Deserialize)]
    struct Cfg {
        port: i64,
    }
    let config = parse("port = \"8080\"").unwrap();
    let val: Cfg = config.deserialize().unwrap();
    assert_eq!(val.port, 8080);
}

#[test]
fn deserialize_bool_coercion() {
    #[derive(Debug, Deserialize)]
    struct Cfg {
        debug: bool,
    }
    let config = parse("debug = \"yes\"").unwrap();
    let val: Cfg = config.deserialize().unwrap();
    assert!(val.debug);
}

#[test]
fn deserialize_to_hashmap() {
    use std::collections::HashMap;
    let config = parse("a = 1\nb = 2").unwrap();
    let map: HashMap<String, i64> = config.deserialize().unwrap();
    assert_eq!(map.get("a"), Some(&1));
    assert_eq!(map.get("b"), Some(&2));
}

/// S15 §"Accessor behaviour" L160-161: every typed-array accessor must invoke
/// numeric_object_to_array. `deserialize_seq` is a typed-array accessor, so
/// Vec<T> deserialization on a numeric-keyed object must succeed.
#[test]
fn deserialize_vec_from_numeric_keyed_object() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Cfg {
        items: Vec<String>,
    }
    // Numeric-keyed object: equivalent to a 2-element list ["a", "b"]
    let config = parse(r#"items = {"0":"a","1":"b"}"#).unwrap();
    let val: Cfg = config
        .deserialize()
        .expect("Vec<String> deserialization on numeric-keyed object must succeed");
    assert_eq!(val.items, vec!["a", "b"]);
}

/// Verify that serde Vec<T> deserialization on a non-numeric-keyed object
/// still fails with an appropriate error (not a panic).
#[test]
fn deserialize_vec_from_non_numeric_object_errors() {
    #[derive(Debug, Deserialize)]
    struct Cfg {
        #[allow(dead_code)]
        items: Vec<String>,
    }
    let config = parse(r#"items = {"foo":"a","bar":"b"}"#).unwrap();
    assert!(
        config.deserialize::<Cfg>().is_err(),
        "Vec<T> deserialization on non-numeric-keyed object must error"
    );
}

/// Verify sort + gap-compaction semantics are preserved through serde.
#[test]
fn deserialize_vec_from_numeric_keyed_object_sorted_and_compacted() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Cfg {
        items: Vec<String>,
    }
    // Keys out of order and with a gap: {1:b, 0:a, 3:d} → ["a","b","d"]
    let config = parse(r#"items = {"1":"b","0":"a","3":"d"}"#).unwrap();
    let val: Cfg = config
        .deserialize()
        .expect("Vec<String> deserialization must sort by integer key");
    assert_eq!(val.items, vec!["a", "b", "d"]);
}

// ---------------------------------------------------------------------------
// from_str / from_file — one-step text → T entry points
// ---------------------------------------------------------------------------

#[test]
fn from_str_deserializes_in_one_step() {
    let server: ServerConfig = hocon::from_str("host = localhost, port = 8080").unwrap();
    assert_eq!(
        server,
        ServerConfig {
            host: "localhost".to_string(),
            port: 8080
        }
    );
}

#[test]
fn from_str_resolves_substitutions() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Cfg {
        a: i64,
        b: i64,
    }
    let cfg: Cfg = hocon::from_str("a = 1, b = ${a}").unwrap();
    assert_eq!(cfg, Cfg { a: 1, b: 1 });
}

#[test]
fn from_str_parse_error_surfaces_as_parse_variant() {
    let err = hocon::from_str::<ServerConfig>("host = {").unwrap_err();
    assert!(
        matches!(err, hocon::HoconError::Parse(_)),
        "syntax error must surface as HoconError::Parse, got {err:?}"
    );
}

#[test]
fn from_str_type_mismatch_surfaces_as_config_variant_with_empty_path() {
    let err = hocon::from_str::<ServerConfig>("host = localhost, port = not-a-number").unwrap_err();
    match err {
        hocon::HoconError::Config(e) => {
            assert!(
                e.path.is_empty(),
                "root-level decode error carries an empty path"
            );
        }
        other => panic!("expected HoconError::Config, got {other:?}"),
    }
}

#[test]
fn from_file_deserializes_with_relative_includes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("defaults.conf"), "port = 8080\n").unwrap();
    let main = dir.path().join("app.conf");
    std::fs::write(&main, "include \"defaults.conf\"\nhost = localhost\n").unwrap();

    let server: ServerConfig = hocon::from_file(&main).unwrap();
    assert_eq!(
        server,
        ServerConfig {
            host: "localhost".to_string(),
            port: 8080
        }
    );
}
