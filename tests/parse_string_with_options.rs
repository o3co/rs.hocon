// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::ParseOptions;

#[test]
fn resolve_substitutions_true_equivalent_to_parse() {
    let a = hocon::parse(r#"a = 1"#).unwrap();
    let b = hocon::parse_string_with_options(r#"a = 1"#, ParseOptions::defaults()).unwrap();
    assert!(a.is_resolved());
    assert!(b.is_resolved());
    assert_eq!(a.get_i64("a").unwrap(), 1);
    assert_eq!(b.get_i64("a").unwrap(), 1);
}

#[test]
fn resolve_substitutions_false_is_not_resolved_when_subst_present() {
    let c = hocon::parse_string_with_options(
        r#"a = ${b}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(!c.is_resolved());
}

#[test]
fn resolve_substitutions_false_no_subst_is_resolved() {
    let c = hocon::parse_string_with_options(
        r#"a = 1"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(c.is_resolved());
}

#[test]
fn origin_description_stored_on_config() {
    let c = hocon::parse_string_with_options(
        r#"a = 1"#,
        ParseOptions::defaults().with_origin_description("unit-test-source".into()),
    )
    .unwrap();
    assert_eq!(c.origin_description(), Some("unit-test-source"));
}
