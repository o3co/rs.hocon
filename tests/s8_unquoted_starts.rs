//! S8.6 — Unquoted strings MUST NOT begin with `-` (unless followed by a digit
//! forming a number prefix) or any digit `0-9` (per HOCON.md L270-276).
//! Issue #63: <https://github.com/o3co/rs.hocon/issues/63>
//!
//! Fixture-driven conformance tests against xx.hocon ground truth at
//! `tests/testdata/hocon/unquoted-starts/`.
//!
//! rs.hocon implements S8.6 via three lex/parse-time checks rather than via a
//! separate `Number` token kind (which the lexer does not have):
//!   1. Main `tokenize` loop, unquoted-start branch (`src/lexer.rs`, runs after
//!      the `is_unquoted_start` predicate dispatches the branch).
//!   2. `parse_subst_body`, unquoted-segment start (same file).
//!   3. `parse_key`, post-`.`-split (`src/parser.rs`), so dotted keys like
//!      `a.-foo` are policed at the segment level.
//! See `docs/spec-compliance.md` §S8.6 for the architectural rationale and the
//! Lightbend-quirk gaps (us13, us15) that remain out of scope for this PR.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/hocon/unquoted-starts")
        .join(format!("{}.conf", name))
}

fn parse_fixture(name: &str) -> Result<hocon::Config, hocon::HoconError> {
    let content = fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", name, e));
    hocon::parse_with_env(&content, &HashMap::new())
}

// Success fixtures: parse must succeed. Per-impl resolved-value assertions are
// intentionally skipped here — value-coercion equivalence with Lightbend is
// covered by the rs.hocon Phase-4 test suite, and the cross-impl JSON ground
// truth in `expected/unquoted-starts/` is the lingua franca; this file's job
// is only to assert the S8.6 *strict-rejection* posture (or its absence on
// success fixtures).
const SUCCESS_FIXTURES: &[&str] = &[
    "us01-digit-prefix-with-tail",
    "us04-hyphen-with-digit",
    "us05-number-then-comment",
    "us06-embedded-digits",
    "us07-embedded-hyphen",
    "us08-numeric-key-positive",
    "us09-dotted-number-key",
    "us10-greedy-backtrack-exp",
    "us11-greedy-backtrack-frac",
    "us12-hex-prefix",
    "us14-multi-dot-version",
    "us16-negative-with-tail",
];

// Error fixtures: parse must throw a ParseError. us02 / us03 are the rule this
// PR enforces (`-` not followed by a digit at the lex layer).
const ERROR_FIXTURES: &[&str] = &["us02-hyphen-no-digit", "us03-hyphen-alone"];

#[test]
fn s8_6_success_fixtures_parse() {
    let mut failures: Vec<(&str, String)> = vec![];
    for name in SUCCESS_FIXTURES {
        if let Err(e) = parse_fixture(name) {
            failures.push((*name, format!("{:?}", e)));
        }
    }
    assert!(
        failures.is_empty(),
        "S8.6 success fixtures failed to parse: {:#?}",
        failures
    );
}

#[test]
fn s8_6_error_fixtures_throw() {
    let mut accepts: Vec<&str> = vec![];
    for name in ERROR_FIXTURES {
        if parse_fixture(name).is_ok() {
            accepts.push(*name);
        }
    }
    assert!(
        accepts.is_empty(),
        "S8.6 error fixtures unexpectedly parsed OK (must throw ParseError): {:?}",
        accepts
    );
}

// ── Known gap tripwires ──────────────────────────────────────────────────────
// us13 (`01`) and us15 (`1e+x`) require introducing a `Number` token kind
// (architectural change deferred under #63). These tests use #[should_panic]
// to fire automatically when the gap closes: the assertion currently panics
// (parse is_err returns false → assert! fails → panic), satisfying the attribute;
// if rs.hocon ever starts rejecting these, assert! succeeds → no panic →
// #[should_panic] sees no panic → test FAILS, surfacing the change without any
// source edit.

#[test]
#[should_panic(expected = "us13-leading-zero")]
fn s8_6_us13_known_gap_tripwire() {
    assert!(
        parse_fixture("us13-leading-zero").is_err(),
        "us13-leading-zero: strict spec requires reject, but rs.hocon currently parses (gap)"
    );
}

#[test]
#[should_panic(expected = "us15-incomplete-exp")]
fn s8_6_us15_known_gap_tripwire() {
    assert!(
        parse_fixture("us15-incomplete-exp").is_err(),
        "us15-incomplete-exp: strict spec / Lightbend require reject, but rs.hocon currently parses (gap)"
    );
}

// ── Path-rule regressions ────────────────────────────────────────────────────

#[test]
fn s8_6_subst_path_hyphen_no_digit_rejected() {
    // Tightened to assert specifically a *parse-time* error, not a resolve-time
    // one: a generic `is_err()` would also pass via an unresolved-substitution
    // ResolveError, masking removal of the parse_subst_body check itself.
    let result = hocon::parse_with_env("x = ${-foo}", &HashMap::new());
    match result {
        Err(hocon::HoconError::Parse(_)) => {} // ok — lex-time rejection
        Err(other) => panic!(
            "S8.6: ${{-foo}} must throw ParseError, got non-Parse variant: {:?}",
            other
        ),
        Ok(_) => panic!("S8.6: ${{-foo}} substitution path must throw ParseError, parsed OK"),
    }
}

#[test]
fn s8_6_key_path_hyphen_segment_rejected() {
    // `a.-foo = 1` — the lexer sees `a.-foo` as one unquoted token; parse_key
    // splits on `.` and must validate the `-foo` segment against the S8.6 rule.
    let result = hocon::parse_with_env("a.-foo = 1", &HashMap::new());
    match result {
        Err(hocon::HoconError::Parse(_)) => {} // ok — parse-time rejection
        Err(other) => panic!(
            "S8.6: a.-foo = 1 must throw ParseError, got non-Parse variant: {:?}",
            other
        ),
        Ok(_) => panic!("S8.6: a.-foo = 1 key path must throw ParseError, parsed OK"),
    }
}
