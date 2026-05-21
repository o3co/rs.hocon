// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::{ParseOptions, ResolveOptions};

#[test]
fn source_keys_absent_from_result() {
    let r = hocon::parse_string_with_options(
        r#"r = ${value}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let src = hocon::parse(r#"value = "found""#).unwrap();
    let out = r.resolve_with(&src, ResolveOptions::defaults()).unwrap();
    assert_eq!(out.get_string("r").unwrap(), "found");
    assert!(!out.has("value"), "source key 'value' must not appear in result");
}

#[test]
fn unresolved_source_raises_not_resolved_error() {
    let r = hocon::parse_string_with_options(
        r#"r = ${value}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let src = hocon::parse_string_with_options(
        r#"value = ${still_missing}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let err = r.resolve_with(&src, ResolveOptions::defaults()).unwrap_err();
    match err {
        hocon::HoconError::NotResolved(_) => {}
        other => panic!("expected HoconError::NotResolved, got {:?}", other),
    }
}

#[test]
fn on_resolved_receiver_is_noop() {
    let r = hocon::parse(r#"r = 5"#).unwrap();
    let src = hocon::parse(r#"unused = 99"#).unwrap();
    let out = r.resolve_with(&src, ResolveOptions::defaults()).unwrap();
    assert_eq!(out.get_i64("r").unwrap(), 5);
    assert!(!out.has("unused"));
}

#[test]
fn nested_keys_do_not_leak_from_source() {
    // Regression: recursive filter. receiver {a:{x:${y}}}, source {a:{z:99}, y:"ok"}
    // result must be {a:{x:"ok"}} NOT {a:{x:"ok",z:99}}.
    let r = hocon::parse_string_with_options(
        r#"a { x = ${y} }"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let src = hocon::parse(r#"a { z = 99 }
                               y = "ok""#).unwrap();
    let out = r.resolve_with(&src, ResolveOptions::defaults()).unwrap();
    assert_eq!(out.get_string("a.x").unwrap(), "ok");
    assert!(
        !out.get_config("a").unwrap().has("z"),
        "nested source key a.z must NOT appear (recursive filter required)"
    );
    assert!(!out.has("y"));
}
