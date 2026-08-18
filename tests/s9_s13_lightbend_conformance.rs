#![cfg(feature = "serde")]

// S9.2 + S13.12 — two Lightbend-conformance fixes surfaced by the py.hocon
// spec-verification wave (2026-08-18). Both were shared ts/py/rs port bugs;
// go.hocon conformed all along. Expected values verified against the
// Lightbend oracle (typesafe-config 1.4.6 probes):
//   - `"""<LF>hello"""` → "\nhello" (every character between the quotes is
//     preserved; the lexer used to strip a leading newline)
//   - `[1, ${?missing}, 3]` → [1, 3] (an undefined optional substitution in
//     element position is NOT added; the resolver used to null-fill it).
//     The prior ✅ cited equiv04/missing-substitutions.conf, which contains
//     no array-element case — a stale citation.

use serde_json::{json, Value};

fn parse_json(src: &str) -> Value {
    let cfg = hocon::parse(src).expect("parse should succeed");
    cfg.deserialize().expect("deserialize should succeed")
}

#[test]
fn s9_2_leading_newline_preserved() {
    let v = parse_json("x = \"\"\"\nhello\"\"\"");
    assert_eq!(v, json!({"x": "\nhello"}));
}

#[test]
fn s9_2_inner_whitespace_still_preserved() {
    let v = parse_json("x = \"\"\"a\n  b\"\"\"");
    assert_eq!(v, json!({"x": "a\n  b"}));
}

#[test]
fn s13_12_undefined_optional_element_omitted() {
    let v = parse_json("arr = [1, ${?missing}, 3]");
    assert_eq!(v, json!({"arr": [1, 3]}));
}

#[test]
fn s13_12_literal_null_element_kept() {
    let v = parse_json("arr = [1, null, 3]");
    assert_eq!(v, json!({"arr": [1, null, 3]}));
}

#[test]
fn s13_12_nested_arrays_drop_independently() {
    let v = parse_json("arr = [[${?m}], [1]]");
    assert_eq!(v, json!({"arr": [[], [1]]}));
}

#[test]
fn s13_12_concat_element_keeps_literal_remainder() {
    // S13.13 semantics inside the element: the optional contributes an empty
    // string, so the element itself survives as the concatenated remainder.
    let v = parse_json("arr = [${?m} foo]");
    assert_eq!(v, json!({"arr": [" foo"]}));
}
