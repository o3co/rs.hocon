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
