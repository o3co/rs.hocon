// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::{ParseOptions, ResolveOptions};

#[test]
fn resolve_on_already_resolved_is_idempotent() {
    let c = hocon::parse(r#"a = 1"#).unwrap();
    assert!(c.is_resolved());
    let r = c.resolve(ResolveOptions::defaults()).unwrap();
    assert!(r.is_resolved());
    assert_eq!(r.get_i64("a").unwrap(), 1);
    let r2 = r.resolve(ResolveOptions::defaults()).unwrap();
    assert_eq!(r2.get_i64("a").unwrap(), 1);
}

#[test]
fn resolve_deferred_path_succeeds() {
    let c = hocon::parse_string_with_options(
        r#"a = ${b}
           b = 1"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(!c.is_resolved());
    let r = c.resolve(ResolveOptions::defaults()).unwrap();
    assert!(r.is_resolved());
    assert_eq!(r.get_i64("a").unwrap(), 1);
}

#[test]
fn resolve_allow_unresolved_does_not_error() {
    let c = hocon::parse_string_with_options(
        r#"a = ${avail}
           b = ${unavail}
           avail = "hello""#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let r = c
        .resolve(
            ResolveOptions::defaults()
                .with_allow_unresolved(true)
                .with_use_system_environment(false),
        )
        .unwrap();
    assert!(!r.is_resolved());
    assert_eq!(r.get_string("a").unwrap(), "hello");
    assert!(r.get_string("b").is_err());
}

#[test]
fn resolve_no_system_environment_errors_on_missing() {
    std::env::set_var("RS_HOCON_TEST_RESOLVE_ENV", "from-env");
    let c = hocon::parse_string_with_options(
        r#"a = ${RS_HOCON_TEST_RESOLVE_ENV}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let result = c.resolve(ResolveOptions::defaults().with_use_system_environment(false));
    std::env::remove_var("RS_HOCON_TEST_RESOLVE_ENV");
    assert!(result.is_err());
}

/// T1 fix: resolve(allow_unresolved=true) preserves unresolved_tree so that a
/// subsequent with_fallback() / resolve() cycle can re-resolve once missing
/// values become available. Previously unresolved_tree was set to None, making
/// re-resolution impossible.
#[test]
fn resolve_allow_unresolved_then_fallback_rereresolves() {
    // Parse `a = ${b}\nx = 1` with deferred resolution.
    let c = hocon::parse_string_with_options(
        "a = ${b}\nx = 1",
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(!c.is_resolved());

    // First resolve: allow_unresolved=true — `b` is missing, `a` stays unresolved.
    let partial = c
        .resolve(
            ResolveOptions::defaults()
                .with_allow_unresolved(true)
                .with_use_system_environment(false),
        )
        .unwrap();
    assert!(!partial.is_resolved(), "a should still be unresolved");
    assert_eq!(partial.get_i64("x").unwrap(), 1);

    // Now make `b` available via a resolved fallback config.
    let fallback = hocon::parse("b = 2").unwrap();
    assert!(fallback.is_resolved());

    // Merge: fallback provides `b`, then re-resolve.
    let full = partial
        .with_fallback(&fallback)
        .resolve(
            ResolveOptions::defaults()
                .with_allow_unresolved(false)
                .with_use_system_environment(false),
        )
        .unwrap();
    assert!(full.is_resolved());
    assert_eq!(full.get_i64("a").unwrap(), 2, "a should now resolve to b=2");
    assert_eq!(full.get_i64("x").unwrap(), 1);
}

/// T2 fix: scalar-concat placeholders use a sentinel path rather than `+`-joined
/// operand paths. After T1, the unresolved_tree carries the real ConcatPlaceholder
/// structure; re-resolution produces the correct concatenated string.
#[test]
fn resolve_allow_unresolved_concat_then_fallback_rereresolves() {
    // `a = ${x} ${y}` — concat with two substitutions; `x` is defined, `y` is not.
    let c = hocon::parse_string_with_options(
        "a = ${x} ${y}\nx = \"hello\"",
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(!c.is_resolved());

    // First resolve: allow_unresolved=true — `y` is missing, concat stays unresolved.
    let partial = c
        .resolve(
            ResolveOptions::defaults()
                .with_allow_unresolved(true)
                .with_use_system_environment(false),
        )
        .unwrap();
    assert!(
        !partial.is_resolved(),
        "concat a should still be unresolved"
    );

    // Provide `y` via fallback and re-resolve.
    let fallback = hocon::parse("y = \"world\"").unwrap();
    let full = partial
        .with_fallback(&fallback)
        .resolve(
            ResolveOptions::defaults()
                .with_allow_unresolved(false)
                .with_use_system_environment(false),
        )
        .unwrap();
    assert!(full.is_resolved());
    assert_eq!(
        full.get_string("a").unwrap(),
        "hello world",
        "concat should resolve to 'hello world' after fallback provides y"
    );
}
