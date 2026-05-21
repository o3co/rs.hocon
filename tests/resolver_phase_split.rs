// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::resolver::{build_tree, contains_placeholders, resolve_tree, InternalResolveOptions};
use hocon::{lexer::tokenize, parser::parse_tokens};
use std::collections::HashMap;

fn base_opts() -> InternalResolveOptions {
    InternalResolveOptions::new(HashMap::new())
}

#[test]
fn build_tree_leaves_substitution_placeholders() {
    let tokens = tokenize("a = ${b}\nb = 1").unwrap();
    let ast = parse_tokens(&tokens).unwrap();
    let tree = build_tree(ast, &base_opts()).unwrap();
    use hocon::resolver::types::ResolverValue;
    let val = tree.fields.get("a").expect("expected field a");
    assert!(
        matches!(val, ResolverValue::Subst(_)),
        "expected Subst placeholder for a after phase 1, got {:?}",
        val
    );
}

#[test]
fn resolve_tree_resolves_placeholders() {
    let tokens = tokenize("a = ${b}\nb = 1").unwrap();
    let ast = parse_tokens(&tokens).unwrap();
    let tree = build_tree(ast, &base_opts()).unwrap();
    let resolved = resolve_tree(tree, &base_opts()).unwrap();
    use hocon::value::{HoconValue, ScalarValue};
    let obj = match &resolved {
        HoconValue::Object(m) => m,
        other => panic!("expected Object, got {:?}", other),
    };
    assert_eq!(
        obj.get("a"),
        Some(&HoconValue::Scalar(ScalarValue::number("1".into()))),
        "expected a=1 after phase 2"
    );
}

#[test]
fn resolve_tree_allow_unresolved_does_not_error() {
    let tokens = tokenize("a = ${missing}").unwrap();
    let ast = parse_tokens(&tokens).unwrap();
    let tree = build_tree(ast, &base_opts()).unwrap();
    assert!(contains_placeholders(&tree), "expected placeholder in unresolved tree");
    let opts = InternalResolveOptions {
        allow_unresolved: true,
        ..base_opts()
    };
    // With allow_unresolved, resolve_tree must succeed (not Err).
    resolve_tree(tree, &opts).expect("allow_unresolved must not return Err");
}

#[test]
fn contains_placeholders_true_before_resolve_false_for_concrete() {
    use hocon::resolver::types::{ResObj, ResolverValue};
    use hocon::value::{HoconValue, ScalarValue};

    let tokens = tokenize("a = ${b}\nb = 1").unwrap();
    let ast = parse_tokens(&tokens).unwrap();
    let tree = build_tree(ast, &base_opts()).unwrap();
    assert!(contains_placeholders(&tree), "pre-resolve: must contain placeholders");

    let mut concrete = ResObj::new();
    concrete
        .fields
        .insert("x".into(), ResolverValue::Resolved(HoconValue::Scalar(ScalarValue::number("1".into()))));
    assert!(!contains_placeholders(&concrete), "fully concrete ResObj must not contain placeholders");
}
