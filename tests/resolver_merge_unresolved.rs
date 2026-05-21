// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::resolver::{merge_unresolved, types::{ResObj, ResolverValue}};
use hocon::value::{HoconValue, ScalarValue};

fn scalar(s: &str) -> ResolverValue {
    ResolverValue::Resolved(HoconValue::Scalar(ScalarValue::string(s.into())))
}

fn num(s: &str) -> ResolverValue {
    ResolverValue::Resolved(HoconValue::Scalar(ScalarValue::number(s.into())))
}

fn obj_with(key: &str, val: ResolverValue) -> ResolverValue {
    let mut o = ResObj::new();
    o.fields.insert(key.into(), val);
    ResolverValue::Obj(o)
}

#[test]
fn set_prior_stores_and_retrieves() {
    let mut o = ResObj::new();
    o.fields.insert("a".into(), scalar("current"));
    o.prior_values.insert("a".into(), scalar("old"));
    let prior = o.prior_values.get("a").expect("expected prior");
    match prior {
        ResolverValue::Resolved(HoconValue::Scalar(sv)) => assert_eq!(sv.raw, "old"),
        other => panic!("unexpected prior {:?}", other),
    }
}

#[test]
fn merge_unresolved_receiver_wins_captures_prior() {
    let mut receiver = ResObj::new();
    receiver.fields.insert("a".into(), scalar("current"));

    let mut fallback = ResObj::new();
    fallback.fields.insert("a".into(), scalar("old"));

    let merged = merge_unresolved(receiver, fallback);

    match merged.fields.get("a") {
        Some(ResolverValue::Resolved(HoconValue::Scalar(sv))) => assert_eq!(sv.raw, "current"),
        other => panic!("expected current, got {:?}", other),
    }
    match merged.prior_values.get("a") {
        Some(ResolverValue::Resolved(HoconValue::Scalar(sv))) => assert_eq!(sv.raw, "old"),
        None => panic!("expected prior to be captured"),
        other => panic!("unexpected prior {:?}", other),
    }
}

#[test]
fn merge_unresolved_both_obj_recurses() {
    let mut receiver = ResObj::new();
    receiver.fields.insert("a".into(), obj_with("x", num("1")));

    let mut fallback = ResObj::new();
    fallback.fields.insert("a".into(), obj_with("y", num("2")));

    let merged = merge_unresolved(receiver, fallback);

    let inner = match merged.fields.get("a") {
        Some(ResolverValue::Obj(o)) => o,
        other => panic!("expected Obj, got {:?}", other),
    };
    assert!(inner.fields.contains_key("x"), "x must be present");
    assert!(inner.fields.contains_key("y"), "y must be present");
}

#[test]
fn merge_unresolved_receiver_scalar_blocks_fallback_obj() {
    let mut receiver = ResObj::new();
    receiver.fields.insert("a".into(), num("42"));

    let mut fallback = ResObj::new();
    fallback.fields.insert("a".into(), obj_with("y", num("2")));

    let merged = merge_unresolved(receiver, fallback);

    // Receiver scalar wins — a must not be Obj
    match merged.fields.get("a") {
        Some(ResolverValue::Resolved(HoconValue::Scalar(sv))) => assert_eq!(sv.raw, "42"),
        other => panic!("expected scalar 42, got {:?}", other),
    }
    // Fallback obj captured as prior
    assert!(merged.prior_values.contains_key("a"), "fallback obj must be captured as prior");
}

#[test]
fn merge_unresolved_fallback_only_key_appears() {
    let receiver = ResObj::new();
    let mut fallback = ResObj::new();
    fallback.fields.insert("b".into(), scalar("from-fallback"));

    let merged = merge_unresolved(receiver, fallback);
    assert!(merged.fields.contains_key("b"), "fallback-only key b must appear in merged");
}
