// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::{ParseOptions, ResolveOptions};

#[test]
fn parse_options_defaults() {
    let opts = ParseOptions::defaults();
    assert!(opts.resolve_substitutions, "resolve_substitutions must default true");
    assert!(opts.origin_description.is_none(), "origin_description must default None");
}

#[test]
fn parse_options_with_resolve_substitutions_non_mutating() {
    let o1 = ParseOptions::defaults();
    let o2 = o1.clone().with_resolve_substitutions(false);
    assert!(o1.resolve_substitutions, "original must be unchanged");
    assert!(!o2.resolve_substitutions, "copy must have false");
}

#[test]
fn parse_options_with_origin_description() {
    let o = ParseOptions::defaults().with_origin_description("inline-config".into());
    assert_eq!(o.origin_description.as_deref(), Some("inline-config"));
}

#[test]
fn parse_options_chainable() {
    let o = ParseOptions::defaults()
        .with_resolve_substitutions(false)
        .with_origin_description("inline".into());
    assert!(!o.resolve_substitutions);
    assert_eq!(o.origin_description.as_deref(), Some("inline"));
}

#[test]
fn resolve_options_defaults() {
    let opts = ResolveOptions::defaults();
    assert!(opts.use_system_environment, "use_system_environment must default true");
    assert!(!opts.allow_unresolved, "allow_unresolved must default false");
}

#[test]
fn resolve_options_with_use_system_environment() {
    let o1 = ResolveOptions::defaults();
    let o2 = o1.clone().with_use_system_environment(false);
    assert!(o1.use_system_environment, "original unchanged");
    assert!(!o2.use_system_environment);
}

#[test]
fn resolve_options_with_allow_unresolved() {
    let o1 = ResolveOptions::defaults();
    let o2 = o1.clone().with_allow_unresolved(true);
    assert!(!o1.allow_unresolved, "original unchanged");
    assert!(o2.allow_unresolved);
}

#[test]
fn resolve_options_chainable() {
    let o = ResolveOptions::defaults()
        .with_use_system_environment(false)
        .with_allow_unresolved(true);
    assert!(!o.use_system_environment);
    assert!(o.allow_unresolved);
}
