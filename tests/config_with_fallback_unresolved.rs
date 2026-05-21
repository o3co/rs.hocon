// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::{ParseOptions, ResolveOptions};

#[test]
fn both_resolved_preserves_existing_semantics() {
    let a = hocon::parse(r#"a = 1"#).unwrap();
    let b = hocon::parse(r#"a = 99
                             c = 3"#).unwrap();
    let m = a.with_fallback(&b);
    assert!(m.is_resolved());
    assert_eq!(m.get_i64("a").unwrap(), 1);
    assert_eq!(m.get_i64("c").unwrap(), 3);
}

#[test]
fn unresolved_receiver_result_is_unresolved() {
    let r = hocon::parse_string_with_options(
        r#"a = ${b}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let f = hocon::parse(r#"b = 7"#).unwrap();
    let m = r.with_fallback(&f);
    assert!(!m.is_resolved());
}

#[test]
fn object_merge_recursive() {
    let a = hocon::parse_string_with_options(
        r#"a { x = 1 }"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let b = hocon::parse_string_with_options(
        r#"a { y = 2 }"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let m = a.with_fallback(&b);
    assert!(m.is_resolved());
    assert_eq!(m.get_i64("a.x").unwrap(), 1);
    assert_eq!(m.get_i64("a.y").unwrap(), 2);
}

#[test]
fn s13a_self_ref_across_fallback() {
    // S13a cross-layer: receiver a = ${?a} extra, fallback a = "base"
    // After merge + resolve: a = "base extra"
    let r = hocon::parse_string_with_options(
        r#"a = ${?a} extra"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let f = hocon::parse_string_with_options(
        r#"a = base"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let merged = r.with_fallback(&f);
    let resolved = merged
        .resolve(ResolveOptions::defaults().with_use_system_environment(false))
        .unwrap();
    assert_eq!(resolved.get_string("a").unwrap(), "base extra");
}
