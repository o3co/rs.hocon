#![cfg(feature = "serde")]

//! Task A (rs.hocon 1.8): public `from_value` + public `HoconDeserializer`.
//! Deserialize an arbitrary `&HoconValue` fragment (object / scalar / array)
//! into a typed value, not just the whole config root.

use hocon::serde::HoconDeserializer;
use hocon::{from_value, parse, HoconValue};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Server {
    host: String,
    port: i64,
}

#[test]
fn from_value_object_fragment() {
    let config = parse(r#"server { host = "localhost", port = 8080 }"#).unwrap();
    let v: &HoconValue = config.get("server").unwrap();
    let server: Server = from_value(v).unwrap();
    assert_eq!(
        server,
        Server {
            host: "localhost".into(),
            port: 8080
        }
    );
}

#[test]
fn from_value_scalar_fragment() {
    let config = parse("port = 8080").unwrap();
    let v = config.get("port").unwrap();
    let port: i64 = from_value(v).unwrap();
    assert_eq!(port, 8080);
}

#[test]
fn from_value_array_fragment() {
    let config = parse("items = [1, 2, 3]").unwrap();
    let v = config.get("items").unwrap();
    let items: Vec<i64> = from_value(v).unwrap();
    assert_eq!(items, vec![1, 2, 3]);
}

#[test]
fn deserializer_is_public_for_composition() {
    // HoconDeserializer must be constructible by external code so it can be
    // handed to serde composition points (DeserializeSeed / deserialize_with).
    let config = parse("port = 8080").unwrap();
    let v = config.get("port").unwrap();
    let port = i64::deserialize(HoconDeserializer::new(v)).unwrap();
    assert_eq!(port, 8080);
}
