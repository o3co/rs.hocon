// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::ParseOptions;

#[test]
fn fused_parse_is_resolved() {
    let c = hocon::parse(r#"a = 1"#).unwrap();
    assert!(c.is_resolved(), "fused parse must produce a resolved Config");
}

#[test]
fn unresolved_parse_is_not_resolved() {
    let c = hocon::parse_string_with_options(
        r#"a = ${b}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(
        !c.is_resolved(),
        "parse with resolve_substitutions=false + substitution present must be unresolved"
    );
}

#[test]
fn concrete_parse_without_substitutions_is_resolved_even_with_deferred() {
    let c = hocon::parse_string_with_options(
        r#"a = 1"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(
        c.is_resolved(),
        "no substitutions present => is_resolved() must return true even with deferred flag"
    );
}
