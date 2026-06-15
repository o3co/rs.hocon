//! Task C (rs.hocon 1.8): `HoconValue` introspection accessors.
//! `as_str` is strict (string scalars only); `as_i64/as_f64/as_bool` apply
//! HOCON-aware coercion; `as_array` is structural (Array variant only — a
//! numeric-keyed object is NOT coerced, unlike `get_list`/serde seq).

use hocon::parse;

#[test]
fn as_str_is_strict() {
    let c = parse(
        r#"s = "hello"
           n = 42"#,
    )
    .unwrap();
    assert_eq!(c.get("s").unwrap().as_str(), Some("hello"));
    // strict: a number scalar is NOT a string
    assert_eq!(c.get("n").unwrap().as_str(), None);
}

#[test]
fn as_i64_coerces_from_string() {
    let c = parse(
        r#"n = 42
           q = "8080""#,
    )
    .unwrap();
    assert_eq!(c.get("n").unwrap().as_i64(), Some(42));
    assert_eq!(c.get("q").unwrap().as_i64(), Some(8080));
    assert_eq!(c.get("n").unwrap().as_f64(), Some(42.0));
}

#[test]
fn as_i64_coerces_whole_number_float_and_exponent() {
    // Must match Config::get_i64 / serde integer coercion: a whole-number float
    // or exponent numeric scalar coerces; a non-whole one does not.
    let c = parse(
        r#"a = 1.0
           b = 1e3
           c = 1.5"#,
    )
    .unwrap();
    assert_eq!(c.get("a").unwrap().as_i64(), Some(1));
    assert_eq!(c.get("b").unwrap().as_i64(), Some(1000));
    assert_eq!(c.get("c").unwrap().as_i64(), None);
}

#[test]
fn as_i64_coerces_quoted_numeric_strings() {
    // Consistent with Config::get_i64: a quoted float-like numeric string coerces too.
    let c = parse(
        r#"a = "1e3"
           b = "1.0"
           s = "hello""#,
    )
    .unwrap();
    assert_eq!(c.get("a").unwrap().as_i64(), Some(1000));
    assert_eq!(c.get("b").unwrap().as_i64(), Some(1));
    assert_eq!(c.get("s").unwrap().as_i64(), None);
}

#[test]
fn as_bool_coerces() {
    let c = parse(
        r#"b = true
           y = "yes""#,
    )
    .unwrap();
    assert_eq!(c.get("b").unwrap().as_bool(), Some(true));
    assert_eq!(c.get("y").unwrap().as_bool(), Some(true));
}

#[test]
fn as_object_and_array_are_structural() {
    let c = parse(
        r#"o { x = 1 }
           a = [1, 2]"#,
    )
    .unwrap();
    assert!(c.get("o").unwrap().as_object().is_some());
    assert_eq!(c.get("a").unwrap().as_array().unwrap().len(), 2);

    // numeric-keyed object is NOT coerced to array by `as_array` (Codex pin)
    let c2 = parse(r#"items { "0" = "a", "1" = "b" }"#).unwrap();
    assert_eq!(c2.get("items").unwrap().as_array(), None);
    assert!(c2.get("items").unwrap().as_object().is_some());
}

#[test]
fn is_predicates() {
    let c = parse(
        r#"o { x = 1 }
           a = [1]
           s = "x"
           z = null"#,
    )
    .unwrap();
    assert!(c.get("o").unwrap().is_object());
    assert!(c.get("a").unwrap().is_array());
    assert!(c.get("s").unwrap().is_scalar());
    assert!(c.get("z").unwrap().is_null());
    assert!(!c.get("o").unwrap().is_scalar());
    assert!(!c.get("s").unwrap().is_null());
}
