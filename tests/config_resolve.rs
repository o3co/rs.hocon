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
