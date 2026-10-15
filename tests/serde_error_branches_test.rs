#![cfg(feature = "serde")]
//! Every type-mismatch branch of the borrowed and the owned deserializer
//! reports its message and the failing key's path.
//!
//! The owned deserializer is reached through a numeric-keyed object read as a
//! sequence (S15): `xs { "0" = … }` deserialized as `Vec<T>`.

use hocon::{parse, parse_string_with_options, ParseOptions};
use serde::de::{Deserializer, Visitor};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;

/// `v = <literal>` deserialized as `{ v: T }`.
fn borrowed_err<T: serde::de::DeserializeOwned>(literal: &str) -> hocon::DeserializeError {
    #[derive(Deserialize)]
    struct W<T> {
        #[allow(dead_code)]
        v: T,
    }
    let cfg = parse(&format!("v = {}", literal)).unwrap();
    match cfg.deserialize::<W<T>>() {
        Ok(_) => panic!("v = {} unexpectedly deserialized", literal),
        Err(e) => e,
    }
}

/// `xs { "0" = <literal> }` deserialized as `{ xs: Vec<T> }` (owned path).
fn owned_err<T: serde::de::DeserializeOwned>(literal: &str) -> hocon::DeserializeError {
    #[derive(Deserialize)]
    struct W<T> {
        #[allow(dead_code)]
        xs: Vec<T>,
    }
    let cfg = parse(&format!("xs {{ \"0\" = {} }}", literal)).unwrap();
    match cfg.deserialize::<W<T>>() {
        Ok(_) => panic!("xs.0 = {} unexpectedly deserialized", literal),
        Err(e) => e,
    }
}

fn assert_err(e: hocon::DeserializeError, path: &str, message_starts: &str) {
    assert_eq!(e.path, path, "message: {}", e.message);
    assert!(
        e.message.starts_with(message_starts),
        "expected {:?}…, got {:?}",
        message_starts,
        e.message
    );
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
enum Color {
    Red,
}

#[test]
fn borrowed_scalar_mismatches() {
    assert_err(borrowed_err::<i32>("{ a = 1 }"), "v", "expected i32, got ");
    assert_err(
        borrowed_err::<bool>("maybe"),
        "v",
        "cannot coerce \"maybe\" to bool",
    );
    assert_err(borrowed_err::<bool>("[1]"), "v", "expected bool, got ");
    assert_err(
        borrowed_err::<f32>("abc"),
        "v",
        "cannot parse \"abc\" as f32",
    );
    assert_err(borrowed_err::<f32>("[1]"), "v", "expected f32, got ");
    assert_err(
        borrowed_err::<f64>("abc"),
        "v",
        "cannot parse \"abc\" as f64",
    );
    assert_err(borrowed_err::<f64>("[1]"), "v", "expected f64, got ");
    assert_err(
        borrowed_err::<char>("ab"),
        "v",
        "expected single char, got \"ab\"",
    );
    assert_err(borrowed_err::<char>("[1]"), "v", "expected char, got ");
    assert_err(borrowed_err::<String>("[1]"), "v", "expected string, got ");
    assert_err(borrowed_err::<()>("1"), "v", "expected null/unit, got ");
}

#[test]
fn borrowed_container_mismatches() {
    assert_err(borrowed_err::<Vec<i32>>("1"), "v", "expected array, got ");
    assert_err(
        borrowed_err::<HashMap<String, i32>>("1"),
        "v",
        "expected object, got ",
    );
    assert_err(
        borrowed_err::<Color>("[1]"),
        "v",
        "expected string for enum variant, got ",
    );
}

#[test]
fn owned_scalar_mismatches() {
    assert_err(owned_err::<i64>("{ a = 1 }"), "xs[0]", "expected i64, got ");
    assert_err(owned_err::<u64>("{ a = 1 }"), "xs[0]", "expected u64, got ");
    assert_err(
        owned_err::<bool>("maybe"),
        "xs[0]",
        "cannot coerce \"maybe\" to bool",
    );
    assert_err(owned_err::<bool>("[1]"), "xs[0]", "expected bool, got ");
    assert_err(
        owned_err::<f64>("abc"),
        "xs[0]",
        "cannot parse \"abc\" as f64",
    );
    assert_err(owned_err::<f64>("[1]"), "xs[0]", "expected f64, got ");
    assert_err(owned_err::<String>("[1]"), "xs[0]", "expected string, got ");
    assert_err(owned_err::<()>("1"), "xs[0]", "expected null/unit, got ");
}

#[test]
fn owned_container_mismatches() {
    assert_err(owned_err::<Vec<i32>>("1"), "xs[0]", "expected array, got ");
    // A non-numeric-keyed object is not convertible to an array.
    assert_err(
        owned_err::<Vec<i32>>("{ a = 1 }"),
        "xs[0]",
        "expected array, got ",
    );
    assert_err(
        owned_err::<HashMap<String, i32>>("1"),
        "xs[0]",
        "expected object, got ",
    );
    assert_err(
        owned_err::<Color>("[1]"),
        "xs[0]",
        "expected string for enum variant, got ",
    );
}

#[test]
fn byte_arrays_are_refused() {
    struct Bytes;
    impl<'de> Deserialize<'de> for Bytes {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Bytes;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("bytes")
                }
            }
            d.deserialize_bytes(V)
        }
    }
    struct ByteBuf;
    impl<'de> Deserialize<'de> for ByteBuf {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = ByteBuf;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("bytes")
                }
            }
            d.deserialize_byte_buf(V)
        }
    }
    assert_err(
        borrowed_err::<Bytes>("x"),
        "v",
        "HOCON does not support byte arrays",
    );
    assert_err(
        borrowed_err::<ByteBuf>("x"),
        "v",
        "HOCON does not support byte arrays",
    );
}

#[test]
fn unresolved_substitution_is_reported_with_its_path() {
    let opts = || ParseOptions::defaults().with_resolve_substitutions(false);
    // Borrowed deserializer, through deserialize_any.
    let cfg = parse_string_with_options("v = ${missing}", opts()).unwrap();
    let e = cfg
        .deserialize::<HashMap<String, serde_json::Value>>()
        .unwrap_err();
    assert_err(e, "v", "cannot deserialize unresolved substitution");
    // Owned deserializer (numeric-keyed object element).
    let cfg = parse_string_with_options(r#"xs { "0" = ${missing} }"#, opts()).unwrap();
    #[derive(Debug, Deserialize)]
    struct W {
        #[allow(dead_code)]
        xs: Vec<serde_json::Value>,
    }
    let e = cfg.deserialize::<W>().unwrap_err();
    assert_err(e, "xs[0]", "cannot deserialize unresolved substitution");
}

#[test]
fn quoted_key_escapes() {
    for (source_key, rendered) in [
        (r#""a\"b""#, r#"m."a\"b""#),
        (r#""c\\d""#, r#"m."c\\d""#),
        (r#""e\rf""#, r#"m."e\rf""#),
        (r#""g\th""#, r#"m."g\th""#),
    ] {
        let cfg = parse(&format!("m {{ {} = x }}", source_key)).unwrap();
        let e = cfg.get_as::<HashMap<String, i32>>("m").unwrap_err();
        assert_eq!(e.path, rendered, "key {}", source_key);
    }
}
