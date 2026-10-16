#![cfg(feature = "serde")]
//! Typed decoding reads a string where the target type is a number, a bool or
//! a char wherever the value sits, as the getters do: an environment variable
//! (`port = ${?PORT}`) is always a string. Two positions refused it:
//!
//! - a numeric-keyed object read as an array (S15) for the narrower number
//!   types, f32 and char, which went through `deserialize_any`;
//! - a map key whose type is a number or a bool.

use std::collections::{BTreeMap, HashMap};

use hocon::parse;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Port {
    port: u16,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Hash)]
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

#[test]
fn map_keys_parse_as_numbers() {
    let cfg = parse(r#"m { 80 = http, 443 = https }"#).unwrap();
    let m: HashMap<u16, String> = cfg.get_as("m").unwrap();
    assert_eq!(m.get(&80).map(String::as_str), Some("http"));
    assert_eq!(m.get(&443).map(String::as_str), Some("https"));
    let signed: BTreeMap<i64, String> = cfg.get_as("m").unwrap();
    assert_eq!(signed.keys().copied().collect::<Vec<_>>(), vec![80, 443]);
}

#[test]
fn map_keys_parse_as_bools_and_newtypes() {
    let cfg = parse(r#"flags { true = on, false = off }, ports { 80 = web }"#).unwrap();
    let flags: HashMap<bool, String> = cfg.get_as("flags").unwrap();
    assert_eq!(flags.get(&true).map(String::as_str), Some("on"));
    let ports: HashMap<PortNumber, String> = cfg.get_as("ports").unwrap();
    assert_eq!(ports.get(&PortNumber(80)).map(String::as_str), Some("web"));
}

#[test]
fn map_key_that_does_not_parse_is_an_error_at_the_key() {
    let cfg = parse(r#"m { http = 80 }"#).unwrap();
    let err = cfg.get_as::<HashMap<u16, i64>>("m").unwrap_err();
    assert_eq!(err.path, "m.http");
    assert!(err.message.contains("\"http\""), "{}", err.message);
}

#[test]
fn map_keys_parse_in_a_numeric_keyed_object_read_as_an_array() {
    // The owned map access (elements of an S15 array) uses the same key rule.
    let cfg = parse(r#"a { "0" { 80 = http } }"#).unwrap();
    let v: Vec<HashMap<u16, String>> = cfg.get_as("a").unwrap();
    assert_eq!(v[0].get(&80).map(String::as_str), Some("http"));
}

#[derive(Debug, Deserialize, PartialEq, Eq, Hash)]
enum Color {
    Red,
    Blue,
}

#[test]
fn unit_enum_map_keys_name_their_variant() {
    // In a numeric-keyed object read as an array (owned map access) as before,
    // and now in an ordinary object as well.
    let cfg = parse(r#"a { "0" { Red = 1 } }, m { Red = 1, Blue = 2 }"#).unwrap();
    let v: Vec<HashMap<Color, i32>> = cfg.get_as("a").unwrap();
    assert_eq!(v[0][&Color::Red], 1);
    let m: HashMap<Color, i32> = cfg.get_as("m").unwrap();
    assert_eq!(m[&Color::Blue], 2);
    let err = parse("m { Green = 1 }")
        .unwrap()
        .get_as::<HashMap<Color, i32>>("m")
        .unwrap_err();
    assert_eq!(err.path, "m.Green");
}

#[test]
fn key_type_that_also_takes_a_string_still_gets_one() {
    // A key that does not parse as the requested number goes to the visitor
    // as a string, as every key did before.
    use serde::de::{Deserializer, Visitor};
    use std::fmt;

    #[derive(Debug, PartialEq, Eq, Hash)]
    enum Lenient {
        Number(u64),
        Name(String),
    }
    impl<'de> Deserialize<'de> for Lenient {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Lenient;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a number or a name")
                }
                fn visit_u64<E>(self, v: u64) -> Result<Lenient, E> {
                    Ok(Lenient::Number(v))
                }
                fn visit_str<E>(self, v: &str) -> Result<Lenient, E> {
                    Ok(Lenient::Name(v.to_owned()))
                }
            }
            d.deserialize_u64(V)
        }
    }
    let cfg = parse(r#"m { 80 = a, http = b }"#).unwrap();
    let m: HashMap<Lenient, String> = cfg.get_as("m").unwrap();
    assert_eq!(m[&Lenient::Number(80)], "a");
    assert_eq!(m[&Lenient::Name("http".into())], "b");

    // The documented change for such a type: a key that parses is now the
    // number, so `"80"` and `"080"` are one key (the later wins), where 1.14.0
    // handed both over as names.
    let cfg = parse(r#"m { "80" = a, "080" = b }"#).unwrap();
    let m: HashMap<Lenient, String> = cfg.get_as("m").unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[&Lenient::Number(80)], "b");
}

#[test]
fn enum_key_variant_arrives_as_owned_string_in_owned_maps() {
    // As with serde's StringDeserializer, which the owned map access used
    // before: a variant identifier that only takes an owned string still works.
    use serde::de::{DeserializeSeed, Deserializer, EnumAccess, Error, VariantAccess, Visitor};
    use std::fmt;

    #[derive(Debug, PartialEq, Eq, Hash)]
    struct OwnedOnlyVariant;
    impl<'de> Deserialize<'de> for OwnedOnlyVariant {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Id;
            impl<'de> DeserializeSeed<'de> for Id {
                type Value = ();
                fn deserialize<D2: Deserializer<'de>>(self, d: D2) -> Result<(), D2::Error> {
                    struct IdVisitor;
                    #[allow(clippy::serde_api_misuse)]
                    impl<'de> Visitor<'de> for IdVisitor {
                        type Value = ();
                        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                            f.write_str("an owned variant name")
                        }
                        fn visit_string<E: Error>(self, v: String) -> Result<(), E> {
                            if v == "Red" {
                                Ok(())
                            } else {
                                Err(E::custom("unknown variant"))
                            }
                        }
                    }
                    d.deserialize_identifier(IdVisitor)
                }
            }
            struct EnumVisitor;
            impl<'de> Visitor<'de> for EnumVisitor {
                type Value = OwnedOnlyVariant;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("enum OwnedOnlyVariant")
                }
                fn visit_enum<A: EnumAccess<'de>>(
                    self,
                    a: A,
                ) -> Result<OwnedOnlyVariant, A::Error> {
                    let ((), variant) = a.variant_seed(Id)?;
                    variant.unit_variant()?;
                    Ok(OwnedOnlyVariant)
                }
            }
            d.deserialize_enum("OwnedOnlyVariant", &["Red"], EnumVisitor)
        }
    }
    let cfg = parse(r#"a { "0" { Red = 1 } }"#).unwrap();
    let v: Vec<HashMap<OwnedOnlyVariant, i32>> = cfg.get_as("a").unwrap();
    assert_eq!(v[0][&OwnedOnlyVariant], 1);
}

#[test]
fn string_map_keys_are_unchanged() {
    let cfg = parse(r#"m { "80" = a, b = c }"#).unwrap();
    let m: HashMap<String, String> = cfg.get_as("m").unwrap();
    assert_eq!(m.get("80").map(String::as_str), Some("a"));
    assert_eq!(m.get("b").map(String::as_str), Some("c"));
}
