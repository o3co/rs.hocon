// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::ParseOptions;

fn unresolved_config() -> hocon::Config {
    hocon::parse_string_with_options(
        r#"a = ${b}
           lit = "hello""#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap()
}

fn unresolved_typed_config() -> hocon::Config {
    hocon::parse_string_with_options(
        r#"dur = ${b}
           bytes = ${b}
           period = ${b}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap()
}

#[test]
fn get_string_on_unresolved_path_errors() {
    let c = unresolved_config();
    let err = c.get_string("a").unwrap_err();
    assert!(
        err.message.contains("not resolved") || err.message.contains("unresolved"),
        "error message must mention resolution state; got: {}",
        err.message
    );
    assert_eq!(err.path, "a");
}

#[test]
fn get_string_on_literal_within_unresolved_config_succeeds() {
    let c = unresolved_config();
    assert_eq!(c.get_string("lit").unwrap(), "hello");
}

#[test]
fn get_i64_on_unresolved_path_errors() {
    let c = unresolved_config();
    assert!(c.get_i64("a").is_err());
}

#[test]
fn get_bool_on_unresolved_path_errors() {
    let c = unresolved_config();
    assert!(c.get_bool("a").is_err());
}

#[test]
fn get_string_option_on_unresolved_returns_none() {
    let c = unresolved_config();
    assert!(c.get_string_option("a").is_none());
}

#[test]
fn get_config_on_unresolved_path_errors() {
    let c = hocon::parse_string_with_options(
        r#"sub = ${?missing}"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    assert!(c.get_config("sub").is_err());
}

#[test]
fn get_duration_on_unresolved_path_errors_as_not_resolved() {
    let c = unresolved_typed_config();
    let err = c.get_duration("dur").unwrap_err();
    assert!(
        err.is_not_resolved(),
        "get_duration on unresolved path must return is_not_resolved(); got: {}",
        err.message
    );
}

#[test]
fn get_bytes_on_unresolved_path_errors_as_not_resolved() {
    let c = unresolved_typed_config();
    let err = c.get_bytes("bytes").unwrap_err();
    assert!(
        err.is_not_resolved(),
        "get_bytes on unresolved path must return is_not_resolved(); got: {}",
        err.message
    );
}

#[test]
fn get_period_on_unresolved_path_errors_as_not_resolved() {
    let c = unresolved_typed_config();
    let err = c.get_period("period").unwrap_err();
    assert!(
        err.is_not_resolved(),
        "get_period on unresolved path must return is_not_resolved(); got: {}",
        err.message
    );
}

#[test]
fn config_error_is_not_resolved_detects_unresolved_string_access() {
    let c = unresolved_config();
    let err = c.get_string("a").unwrap_err();
    assert!(
        err.is_not_resolved(),
        "ConfigError::is_not_resolved() must return true for placeholder access; got: {}",
        err.message
    );
}

#[test]
fn config_error_is_not_resolved_false_for_missing_key() {
    let c = unresolved_config();
    let err = c.get_string("no_such_key").unwrap_err();
    assert!(
        !err.is_not_resolved(),
        "ConfigError::is_not_resolved() must return false for missing-key error"
    );
}
