// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! Layer-1 programmatic E12 conformance tests.
//! Layer-2 YAML scenario runner lives in tests/deferred_resolution_fixtures.rs.

use hocon::{empty, parse_string_with_options, ParseOptions, ResolveOptions};

fn deferred_parse(input: &str) -> hocon::Config {
    parse_string_with_options(
        input,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .expect("parse must succeed")
}

fn no_env_opts() -> ResolveOptions {
    ResolveOptions::defaults().with_use_system_environment(false)
}

// --- is_resolved state transitions ---

#[test]
fn fused_parse_is_resolved() {
    let c = hocon::parse(r#"a = 1"#).unwrap();
    assert!(c.is_resolved(), "fused parse must produce a resolved Config");
}

#[test]
fn deferred_parse_with_subst_is_not_resolved() {
    let c = deferred_parse(r#"a = ${b}"#);
    assert!(!c.is_resolved(), "deferred parse with substitution must be unresolved");
}

#[test]
fn deferred_parse_no_subst_is_resolved() {
    let c = deferred_parse(r#"a = 1"#);
    assert!(
        c.is_resolved(),
        "no substitutions -> is_resolved must be true even with deferred flag"
    );
}

// --- from_map / empty integration ---

#[cfg(feature = "serde")]
#[test]
fn from_map_is_resolved() {
    use serde_json::json;
    let map = json!({"x": 1}).as_object().unwrap().clone();
    let c = hocon::from_map(map, None).unwrap();
    assert!(c.is_resolved(), "from_map must produce a resolved Config");
}

#[test]
fn empty_is_resolved() {
    assert!(empty(None).is_resolved());
}

#[test]
fn empty_as_fallback_is_noop() {
    let c = hocon::parse(r#"a = 1"#).unwrap();
    let m = c.with_fallback(&empty(None));
    assert_eq!(m.get_i64("a").unwrap(), 1);
}

#[test]
fn empty_resolve_is_noop() {
    let r = empty(None).resolve(ResolveOptions::defaults()).unwrap();
    assert!(r.is_resolved());
    assert!(r.keys().is_empty());
}

// --- getter precondition: NotResolved ---

#[test]
fn getter_on_unresolved_path_returns_error() {
    let c = deferred_parse(r#"a = ${b}"#);
    let err = c.get_string("a").expect_err("must error on unresolved path");
    // The error is a ConfigError with "not resolved" in message.
    let msg = format!("{}", err);
    assert!(
        msg.to_lowercase().contains("not resolved") || msg.to_lowercase().contains("unresolved"),
        "error must indicate unresolved state; got: {}",
        msg
    );
}

// --- with_fallback + deferred (S13a cross-layer self-reference) ---

#[test]
fn s13a_optional_self_ref_across_fallback_dr04() {
    // receiver: a = ${?a} extra
    // fallback: a = base
    // result:   a = "base extra"
    let r = deferred_parse(r#"a = ${?a} extra"#);
    let f = deferred_parse(r#"a = base"#);
    let resolved = r
        .with_fallback(&f)
        .resolve(no_env_opts())
        .expect("Resolve must succeed");
    assert_eq!(resolved.get_string("a").unwrap(), "base extra");
}

#[test]
fn s13a_required_self_ref_with_fallback_dr05() {
    let r = deferred_parse(r#"a = ${a} extra"#);
    let f = deferred_parse(r#"a = base"#);
    let resolved = r
        .with_fallback(&f)
        .resolve(no_env_opts())
        .expect("Resolve must succeed");
    assert_eq!(resolved.get_string("a").unwrap(), "base extra");
}

#[test]
fn s13a_required_self_ref_no_fallback_dr06() {
    let r = deferred_parse(r#"a = ${a} extra"#);
    let result = r.resolve(no_env_opts());
    assert!(result.is_err(), "required self-ref with no prior must error");
}

// --- transitive cross-layer (dr21) ---

#[test]
fn transitive_cross_layer_dr21() {
    let r = deferred_parse(r#"a = ${b}"#);
    let f1 = deferred_parse(r#"b = ${c}"#);
    let f2 = deferred_parse(r#"c = 1"#);
    let resolved = r
        .with_fallback(&f1)
        .with_fallback(&f2)
        .resolve(no_env_opts())
        .unwrap();
    assert_eq!(resolved.get_i64("a").unwrap(), 1);
}

// --- hidden substitutions across layers (dr23) ---

#[test]
fn hidden_across_layers_dr23() {
    // Receiver: foo = 42 (wins). Fallback: foo = ${nonexist}.
    // Hidden substitution must not be evaluated.
    let r = deferred_parse(r#"foo = 42"#);
    let f = deferred_parse(r#"foo = ${nonexist}"#);
    let resolved = r
        .with_fallback(&f)
        .resolve(no_env_opts())
        .expect("hidden substitution must not cause error");
    assert_eq!(resolved.get_i64("foo").unwrap(), 42);
}

// --- cross-layer cycle (dr18) ---

#[test]
fn cross_layer_cycle_dr18() {
    let r = deferred_parse(r#"a = ${b}"#);
    let f = deferred_parse(r#"b = ${a}"#);
    let result = r.with_fallback(&f).resolve(no_env_opts());
    assert!(result.is_err(), "cross-layer cycle must return ResolveError");
}

// --- optional substitution materialisation (dr24-dr25) ---

#[test]
fn optional_undef_materialisation_standalone_dr24() {
    let r = deferred_parse(r#"a = ${?x}"#);
    let resolved = r.resolve(no_env_opts()).unwrap();
    assert!(
        !resolved.has("a"),
        "standalone optional undefined -> field must be omitted"
    );
}

#[test]
fn optional_undef_materialisation_concat_dr25() {
    let r = deferred_parse(r#"a = ${?x} "tail""#);
    let resolved = r.resolve(no_env_opts()).unwrap();
    // When optional subst is undefined in concat, it contributes nothing;
    // the concat produces the remaining pieces.
    let val = resolved.get_string("a").unwrap_or_default();
    assert!(
        val.contains("tail"),
        "concat with undefined optional must produce remaining pieces; got {:?}",
        val
    );
}

// --- composition barrier (dr10) ---

#[test]
fn composition_barrier_dr10() {
    // Receiver: a { x = 1 } — object at a.
    // fb1: a = "scalar" — scalar wins at a (blocks fb2).
    // fb2: a { y = 2 } — object; must NOT contribute because fb1 scalar at a
    //      forms a composition barrier against fb2's object.
    let r = deferred_parse(r#"a { x = 1 }"#);
    let fb1 = deferred_parse(r#"a = "scalar""#);
    let fb2 = deferred_parse(r#"a { y = 2 }"#);
    let m = r.with_fallback(&fb1).with_fallback(&fb2);
    let resolved = m.resolve(no_env_opts()).unwrap();
    assert_eq!(resolved.get_i64("a.x").unwrap(), 1);
    let a_cfg = resolved.get_config("a").unwrap();
    assert!(!a_cfg.has("y"), "composition barrier: fb2's y must not contribute");
}

// --- double-resolve idempotency (dr19) ---

#[test]
fn double_resolve_is_idempotent_dr19() {
    let c = deferred_parse(r#"a = 1"#);
    let r1 = c.resolve(no_env_opts()).unwrap();
    let r2 = r1.clone().resolve(no_env_opts()).unwrap();
    assert_eq!(r1.get_i64("a").unwrap(), r2.get_i64("a").unwrap());
}

// --- not_resolved error propagation ---

#[test]
fn not_resolved_error_is_config_error() {
    let c = deferred_parse(r#"a = ${b}"#);
    let result = c.get_string("a");
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.to_lowercase().contains("not resolved") || msg.to_lowercase().contains("unresolved"),
        "ConfigError message must mention resolution state; got: {}",
        msg
    );
}

// --- render_json_for_test sanity check ---

#[test]
fn render_json_for_test_basic() {
    use hocon::_render_json_for_test;
    let c = hocon::parse(
        r#"
        a = 1
        b = "hello"
        c { x = true }
    "#,
    )
    .unwrap();
    let json = _render_json_for_test(&c);
    // Keys sorted: a, b, c; c has nested x.
    assert!(
        json.contains(r#""a":1"#) || json.contains(r#""a": 1"#),
        "a must be 1; got {}",
        json
    );
    assert!(
        json.contains(r#""b":"hello""#) || json.contains(r#""b": "hello""#),
        "b must be hello; got {}",
        json
    );
    assert!(
        json.contains(r#""x":true"#) || json.contains(r#""x": true"#),
        "c.x must be true; got {}",
        json
    );
}
