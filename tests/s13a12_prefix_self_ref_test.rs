#![cfg(feature = "serde")]

// S13a.12 (HOCON.md L791) — prefix self-reference resolves to "below".
//
// A substitution whose target lies INSIDE the field being defined
// (`foo : ${foo.a}`) resolves against the field's below value (the merge of
// the stack beneath the substitution), never the final tree. Found 2026-08-18
// by a cross-impl probe: all four siblings resolved the spec example against
// the final tree, yielding {a:2} instead of {a:2, c:1} — this crate's
// recorded ✅ cited lightbend_test06, which cannot discriminate (its later
// object overrides every key the substitution contributes). Fixed in lockstep
// with ts.hocon / py.hocon / go.hocon.
//
// The prefix rule applies only in value-stack positions: a substitution
// nested inside an object literal that references a sibling branch of the
// same field is a lazy final-tree lookup (S13a.14), pinned here too.

use serde_json::{json, Value};

fn parse_json(src: &str) -> Value {
    let cfg = hocon::parse(src).expect("parse should succeed");
    cfg.deserialize().expect("deserialize should succeed")
}

#[test]
fn spec_example_sandwich() {
    let v = parse_json("foo : { a : { c : 1 } }\nfoo : ${foo.a}\nfoo : { a : 2 }");
    assert_eq!(v["foo"], json!({"a": 2, "c": 1}));
}

#[test]
fn two_layers_subst_last() {
    let v = parse_json("foo : { a : { c : 1 } }\nfoo : ${foo.a}");
    assert_eq!(v["foo"], json!({"a": {"c": 1}, "c": 1}));
}

#[test]
fn below_layer_keys_survive() {
    let v = parse_json("foo : { a : { c : 1 }, keep : 9 }\nfoo : ${foo.a}\nfoo : { a : 2 }");
    assert_eq!(v["foo"], json!({"a": 2, "keep": 9, "c": 1}));
}

#[test]
fn scalar_navigation_resets_stack() {
    let v = parse_json("foo : { a : 5 }\nfoo : ${foo.a}\nfoo : { b : 2 }");
    assert_eq!(v["foo"], json!({"b": 2}));
}

#[test]
fn optional_miss_vanishes_transparently() {
    let v = parse_json("foo : { a : 1 }\nfoo : ${?foo.nope}\nfoo : { b : 2 }");
    assert_eq!(v["foo"], json!({"a": 1, "b": 2}));
}

#[test]
fn required_miss_is_undefined_error() {
    let err = hocon::parse("foo : { a : 1 }\nfoo : ${foo.nope}\nfoo : { b : 2 }")
        .expect_err("required prefix self-ref with nothing below must error");
    assert!(
        err.to_string().contains("could not resolve substitution"),
        "expected undefined classification, got: {err}"
    );
}

#[test]
fn nested_paths() {
    let v = parse_json(
        "srv : { foo : { a : { c : 1 } } }\nsrv : { foo : ${srv.foo.a} }\nsrv : { foo : { a : 2 } }",
    );
    assert_eq!(v["srv"]["foo"], json!({"a": 2, "c": 1}));
}

#[test]
fn regression_non_self_ref_sandwich_unchanged() {
    let v =
        parse_json("d = { x : { c : 1 } }\nfoo : { a : { c : 9 } }\nfoo : ${d.x}\nfoo : { a : 2 }");
    assert_eq!(v["foo"], json!({"c": 1, "a": 2}));
}

#[test]
fn regression_sibling_ref_in_deeper_prior_sees_final_tree() {
    let v = parse_json("bar { nested { x = { q: 10 }\na = ${bar.nested.x}\na = { c: 3 } } }");
    assert_eq!(v["bar"]["nested"]["a"], json!({"q": 10, "c": 3}));
}

#[test]
fn interior_sibling_ref_stays_lazy_final_tree() {
    // ${a.p.v} sits INSIDE a's object literal — an object-interior sibling
    // reference, not a value-stack layer. It must keep S13a.14 lazy final-tree
    // semantics (the allow_prefix narrowing), not fold to a below value.
    let v = parse_json("a = { p : { v : 1 }, x : ${a.p.v} }\na = { y : 2 }");
    assert_eq!(v["a"], json!({"p": {"v": 1}, "x": 1, "y": 2}));
}

#[test]
fn two_layers_required_miss_errors() {
    let err = hocon::parse("foo : { a : 1 }\nfoo : ${foo.nope}")
        .expect_err("two-layer required miss must error");
    assert!(err.to_string().contains("could not resolve substitution"));
}

#[test]
fn two_layers_optional_miss_keeps_prior() {
    let v = parse_json("foo : { a : 1 }\nfoo : ${?foo.nope}");
    assert_eq!(v["foo"], json!({"a": 1}));
}

#[test]
fn unnavigable_below_optional_stays_resolvable() {
    let v = parse_json("foo : { a : ${x} }\nfoo : ${?foo.a.b}\nfoo : { z : 1 }\nx : { b : 7 }");
    assert_eq!(v["foo"], json!({"z": 1}));
}
