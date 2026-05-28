// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! S10.11 — numbers stringify "as written in the source file" (HOCON.md
//! §String value concatenation, L366). Cross-impl regression for
//! go.hocon#133.
//!
//! Pre-fix, `parse_scalar_value` canonicalized integer lexemes via
//! `n.to_string()` (`05` → `"5"`, `01` → `"1"`), so a `${minor}`
//! substitution into a string concat lost the leading zero
//! (`version = ${major}.${minor}` → `"26.5"` instead of `"26.05"`).
//! Lightbend keeps the source lexeme for stringification while still
//! reading the standalone value semantically (`getInt` / serde re-parse,
//! dropping leading zeros). The fix stores the raw token text in the
//! `Number` scalar's `raw` field; numeric accessors already parse it.

#[test]
fn s10_11_leading_zero_preserved_in_concat() {
    let cfg = hocon::parse("major = 26\nminor = 05\nversion = ${major}.${minor}\n").expect("parse");
    assert_eq!(cfg.get_string("version").unwrap(), "26.05");
}

#[test]
fn s10_11_standalone_numeric_still_canonical() {
    // The standalone value reads/serializes semantically: 05 → 5.
    let cfg = hocon::parse("minor = 05\n").expect("parse");
    assert_eq!(cfg.get_i64("minor").unwrap(), 5);
}

#[test]
fn s10_11_leading_zero_string_getter_returns_lexeme() {
    // Lenient get_string on a numeric scalar returns the source lexeme
    // (Lightbend's getString throws WrongType; rs.hocon is lenient and
    // must echo the preserved lexeme, not a canonicalized form).
    let cfg = hocon::parse("minor = 05\n").expect("parse");
    assert_eq!(cfg.get_string("minor").unwrap(), "05");
}

#[test]
fn s10_11_negative_zero_lexeme_in_concat() {
    let cfg = hocon::parse("z = -0\ns = v${z}\n").expect("parse");
    assert_eq!(cfg.get_string("s").unwrap(), "v-0");
    assert_eq!(cfg.get_i64("z").unwrap(), 0);
}

#[test]
fn s10_11_unquoted_numeric_prefix_name_unaffected() {
    // `00_example` is not a pure number (parse::<i64> fails), so it stays a
    // string verbatim — this path was already correct, pinned as a guard.
    let cfg = hocon::parse("name = 00_example\n").expect("parse");
    assert_eq!(cfg.get_string("name").unwrap(), "00_example");
}

#[test]
fn s10_11_fractional_lexeme_preserved() {
    // Fractional forms were already preserved (f64 path kept raw); guard it.
    let cfg = hocon::parse("pi = 3.140\ns = v${pi}\n").expect("parse");
    assert_eq!(cfg.get_string("s").unwrap(), "v3.140");
}
