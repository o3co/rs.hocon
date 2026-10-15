#![cfg(feature = "serde")]
//! Serde decode errors carry the path of the failing key (xx.hocon#102,
//! contract 3): `DeserializeError::path`, and the `ConfigError::path` of
//! `get_as` / `from_str`.

use hocon::{parse, HoconError};
use serde::Deserialize;
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Server {
    host: String,
    port: u16,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct App {
    server: Server,
    servers: Vec<Server>,
}

const VALID_SERVERS: &str = "servers = [{ host = a, port = 1 }]";

#[test]
fn nested_key_path() {
    let cfg = parse(&format!(
        "server {{ host = a, port = big }}\n{}",
        VALID_SERVERS
    ))
    .unwrap();
    let err = cfg.deserialize::<App>().unwrap_err();
    assert_eq!(err.path, "server.port");
    assert_eq!(err.message, "cannot parse \"big\" as u16");
    assert_eq!(
        err.to_string(),
        "HOCON deserialization error at server.port: cannot parse \"big\" as u16"
    );
}

#[test]
fn array_element_path() {
    let cfg = parse(
        "server { host = a, port = 1 }\n\
         servers = [{ host = a, port = 1 }, { host = b, port = -1 }]",
    )
    .unwrap();
    let err = cfg.deserialize::<App>().unwrap_err();
    assert_eq!(err.path, "servers[1].port");
}

#[test]
fn numeric_keyed_object_element_path() {
    // S15 numeric-keyed object → array goes through the owned accessors.
    let cfg = parse(
        "server { host = a, port = 1 }\n\
         servers { \"0\" { host = a, port = 1 }, \"1\" { host = b, port = x } }",
    )
    .unwrap();
    let err = cfg.deserialize::<App>().unwrap_err();
    assert_eq!(err.path, "servers[1].port");
}

#[test]
fn missing_field_path_is_the_enclosing_object() {
    let cfg = parse(&format!("server {{ host = a }}\n{}", VALID_SERVERS)).unwrap();
    let err = cfg.deserialize::<App>().unwrap_err();
    assert_eq!(err.path, "server");
    assert_eq!(err.message, "missing field `port`");
}

#[test]
fn unknown_field_path_names_the_key() {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Strict {
        a: i32,
    }
    let cfg = parse("s { a = 1, b = 2 }").unwrap();
    let err = cfg.get_as::<Strict>("s").unwrap_err();
    assert_eq!(err.path, "s.b");
}

#[test]
fn key_needing_quotes_is_quoted() {
    let cfg = parse(r#"m { "web.api" = x }"#).unwrap();
    let err = cfg.get_as::<HashMap<String, i32>>("m").unwrap_err();
    assert_eq!(err.path, r#"m."web.api""#);
}

#[test]
fn root_level_error_has_empty_path() {
    let err =
        hocon::from_value::<Vec<i32>>(&hocon::HoconValue::Object(Default::default())).unwrap_err();
    assert_eq!(err.path, "");
    assert!(err.to_string().starts_with("HOCON deserialization error: "));
}

#[test]
fn get_as_joins_its_path() {
    let cfg = parse("app { servers = [{ host = a, port = x }] }").unwrap();
    let err = cfg.get_as::<Vec<Server>>("app.servers").unwrap_err();
    assert_eq!(err.path, "app.servers[0].port");
}

#[test]
fn from_str_reports_the_path() {
    let err = hocon::from_str::<Server>("host = a, port = x").unwrap_err();
    match err {
        HoconError::Config(e) => assert_eq!(e.path, "port"),
        other => panic!("expected HoconError::Config, got {:?}", other),
    }
}

#[test]
fn flatten_path_stops_at_the_buffered_value() {
    // serde buffers a flattened struct's input; the inner key no longer
    // passes through our accessors, so the path ends at the buffered object.
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct Inner {
        #[serde(flatten)]
        server: Server,
    }
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct Outer {
        a: Inner,
    }
    let cfg = parse("a { host = h, port = x }").unwrap();
    let err = cfg.deserialize::<Outer>().unwrap_err();
    assert_eq!(err.path, "a");
}

#[test]
fn owned_map_keys_arrive_as_owned_strings() {
    // Keys of a numeric-keyed object element (owned accessors) still reach a
    // key visitor through visit_string.
    use serde::de::{Deserializer, Visitor};
    use std::fmt;

    #[derive(Debug, PartialEq, Eq, Hash)]
    struct OwnedOnlyKey(String);
    impl<'de> Deserialize<'de> for OwnedOnlyKey {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            // Deliberately visit_string-only: models a key type that only
            // accepts owned strings.
            #[allow(clippy::serde_api_misuse)]
            impl<'de> Visitor<'de> for V {
                type Value = OwnedOnlyKey;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("an owned string")
                }
                fn visit_string<E>(self, v: String) -> Result<OwnedOnlyKey, E> {
                    Ok(OwnedOnlyKey(v))
                }
            }
            d.deserialize_string(V)
        }
    }
    let cfg = parse(r#"xs { "0" { k = 1 } }"#).unwrap();
    let xs = cfg.get_as::<Vec<HashMap<OwnedOnlyKey, i32>>>("xs").unwrap();
    assert_eq!(xs[0][&OwnedOnlyKey("k".into())], 1);
}

#[test]
fn quoted_key_stays_on_one_line() {
    let cfg = parse("m { \"a\\nb\" = x }").unwrap();
    let err = cfg.get_as::<HashMap<String, i32>>("m").unwrap_err();
    assert_eq!(err.path, r#"m."a\nb""#);
    assert!(!err.to_string().contains('\n'));

    let cfg = parse("m { \"t\\u0001\" = y }").unwrap();
    let err = cfg.get_as::<HashMap<String, i32>>("m").unwrap_err();
    assert_eq!(err.path, r#"m."t\u0001""#);
}

#[test]
fn nested_array_indices() {
    let cfg = parse("grid = [[1, 2], [3, x]]").unwrap();
    let err = cfg.get_as::<Vec<Vec<i32>>>("grid").unwrap_err();
    assert_eq!(err.path, "grid[1][1]");
}

#[test]
fn numeric_keyed_object_reports_the_compacted_position() {
    // Documented: [i] is the position after sort + compaction, not the key.
    let cfg = parse(r#"gap { "0" { host = a, port = 1 }, "5" { host = b, port = x } }"#).unwrap();
    let err = cfg.get_as::<Vec<Server>>("gap").unwrap_err();
    assert_eq!(err.path, "gap[1].port");
}

#[test]
fn key_error_inside_an_owned_element() {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Strict {
        a: i32,
    }
    let cfg = parse(r#"xs { "0" { a = 1, zz = 2 } }"#).unwrap();
    let err = cfg.get_as::<Vec<Strict>>("xs").unwrap_err();
    assert_eq!(err.path, "xs[0].zz");
}

#[test]
fn get_as_renders_its_lookup_path_like_the_decode_path() {
    // `"servers[0]"` is one key, not an index: the prefix is quoted like any
    // other key that is not a bare identifier.
    let cfg = parse(r#""servers[0]" { port = x }"#).unwrap();
    let err = cfg
        .get_as::<HashMap<String, u16>>(r#""servers[0]""#)
        .unwrap_err();
    assert_eq!(err.path, r#""servers[0]".port"#);
    // The same key looked up unquoted (`[` is not a path separator).
    let err = cfg
        .get_as::<HashMap<String, u16>>("servers[0]")
        .unwrap_err();
    assert_eq!(err.path, r#""servers[0]".port"#);

    let cfg = parse(r#"a { "web.api" { port = x } }"#).unwrap();
    let err = cfg
        .get_as::<HashMap<String, u16>>(r#"a."web.api""#)
        .unwrap_err();
    assert_eq!(err.path, r#"a."web.api".port"#);
}

#[test]
fn line_separator_in_key_is_escaped() {
    let cfg = parse("m { \"a\u{2028}b\" = x }").unwrap();
    let err = cfg.get_as::<HashMap<String, i32>>("m").unwrap_err();
    assert_eq!(err.path, r#"m."a\u2028b""#);
}
