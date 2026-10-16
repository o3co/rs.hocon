#![cfg(feature = "serde")]
//! Typed decoding reads a string where the target type is a number, a bool or
//! a char wherever the value sits, as the getters do: an environment variable
//! (`port = ${?PORT}`) is always a string. A numeric-keyed object read as an
//! array (S15) refused it for the narrower number types, f32 and char, which
//! went through `deserialize_any`.

use hocon::parse;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Port {
    port: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
struct PortNumber(u16);

#[test]
fn numeric_keyed_object_as_array_coerces_narrow_numbers() {
    let cfg = parse(r#"a { "0" = "5", "1" = "6" }"#).unwrap();
    assert_eq!(cfg.get_as::<Vec<u16>>("a").unwrap(), vec![5, 6]);
    assert_eq!(cfg.get_as::<Vec<i8>>("a").unwrap(), vec![5, 6]);
    assert_eq!(cfg.get_as::<Vec<i16>>("a").unwrap(), vec![5, 6]);
    assert_eq!(cfg.get_as::<Vec<i32>>("a").unwrap(), vec![5, 6]);
    assert_eq!(cfg.get_as::<Vec<u8>>("a").unwrap(), vec![5, 6]);
    assert_eq!(cfg.get_as::<Vec<u32>>("a").unwrap(), vec![5, 6]);
    assert_eq!(
        cfg.get_as::<Vec<PortNumber>>("a").unwrap(),
        vec![PortNumber(5), PortNumber(6)]
    );
}

#[test]
fn negative_zero_reads_as_zero_for_unsigned_types() {
    // 1.14.0 read `-0` as 0 here through deserialize_any; it still does.
    let cfg = parse(r#"a { "0" = -0 }, b = [-0], c = -0"#).unwrap();
    assert_eq!(cfg.get_as::<Vec<u8>>("a").unwrap(), vec![0]);
    assert_eq!(cfg.get_as::<Vec<u16>>("a").unwrap(), vec![0]);
    assert_eq!(cfg.get_as::<Vec<u32>>("a").unwrap(), vec![0]);
    assert_eq!(cfg.get_as::<Vec<u64>>("b").unwrap(), vec![0]);
    assert_eq!(cfg.get_as::<u16>("c").unwrap(), 0);
    assert!(parse("c = -1").unwrap().get_as::<u16>("c").is_err());
}

#[test]
fn numeric_keyed_object_as_array_coerces_f32_and_char() {
    let cfg = parse(r#"f { "0" = "1.5" }, c { "0" = "x" }"#).unwrap();
    assert_eq!(cfg.get_as::<Vec<f32>>("f").unwrap(), vec![1.5]);
    assert_eq!(cfg.get_as::<Vec<char>>("c").unwrap(), vec!['x']);
}

#[test]
fn numeric_keyed_object_as_array_coerces_inside_structs() {
    let cfg = parse(r#"a { "0" { port = "80" }, "1" { port = "443" } }"#).unwrap();
    assert_eq!(
        cfg.get_as::<Vec<Port>>("a").unwrap(),
        vec![Port { port: 80 }, Port { port: 443 }]
    );
}

#[test]
fn numeric_keyed_object_as_array_names_the_failing_element() {
    let cfg = parse(r#"a { "0" { port = "80" }, "1" { port = "x" } }"#).unwrap();
    let err = cfg.get_as::<Vec<Port>>("a").unwrap_err();
    assert_eq!(err.path, "a[1].port");
}
