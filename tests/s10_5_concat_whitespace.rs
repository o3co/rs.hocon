// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! S10.5 — inner whitespace between simple values in a string value
//! concatenation is preserved verbatim (HOCON.md §String value
//! concatenation, L332). Cross-impl regression for go.hocon#132.
//!
//! Pre-fix, rs.hocon's `parse_value` inserted a single hardcoded `" "`
//! separator between concat pieces regardless of how many whitespace
//! characters the source had, collapsing every multi-space run to one
//! space. Lightbend keeps the literal run. The fix threads
//! `peek_preceding_whitespace()` (the same lexer field E13 added for
//! key-position whitespace) into the value-position separator.
//!
//! The undefined-optional case (`"left"  ${?UNSET}  "right"`) is the
//! shape reported in go.hocon#132: both surrounding whitespace runs must
//! survive even though the substitution between them resolves to nothing,
//! yielding `"left    right"` (2 + 2 = 4 spaces).

use std::collections::HashMap;

#[test]
fn s10_5_unquoted_multi_space_preserved() {
    let cfg = hocon::parse("a = foo   bar\n").expect("parse");
    assert_eq!(cfg.get_string("a").unwrap(), "foo   bar");
}

#[test]
fn s10_5_quoted_multi_space_preserved() {
    let cfg = hocon::parse("a = \"foo\"   \"bar\"\n").expect("parse");
    assert_eq!(cfg.get_string("a").unwrap(), "foo   bar");
}

#[test]
fn s10_5_single_space_unchanged() {
    let cfg = hocon::parse("a = foo bar\n").expect("parse");
    assert_eq!(cfg.get_string("a").unwrap(), "foo bar");
}

#[test]
fn s10_5_defined_subst_multi_space_preserved() {
    let cfg = hocon::parse("x = mid\na = \"left\"  ${x}  \"right\"\n").expect("parse");
    assert_eq!(cfg.get_string("a").unwrap(), "left  mid  right");
}

#[test]
fn s10_5_undefined_optional_keeps_both_runs() {
    // go.hocon#132 canonical repro: env unset → substitution contributes
    // nothing, but the 2 + 2 whitespace runs around it must remain.
    let env: HashMap<String, String> = HashMap::new();
    let cfg = hocon::parse_with_env("a = \"left\"  ${?GO132_UNSET}  \"right\"\n", &env)
        .expect("parse");
    assert_eq!(cfg.get_string("a").unwrap(), "left    right");
}

#[test]
fn s10_5_tab_run_preserved() {
    // HOCON_WS includes tab; the literal run (space + tab + space) is kept.
    let cfg = hocon::parse("a = foo \t bar\n").expect("parse");
    assert_eq!(cfg.get_string("a").unwrap(), "foo \t bar");
}
