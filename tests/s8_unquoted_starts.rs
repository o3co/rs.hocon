//! S8.6 — Unquoted strings at value-position MAY begin with `-` (treated as
//! unquoted text when not followed by a digit) or with digits (greedy Java
//! numeric semantics, fall back to unquoted on parse failure). Concat-
//! continuation positions (after `${...}`, `"..."`, a prior unquoted run,
//! etc.) accept any unquoted-permissible character except `+` as a
//! continuation of the existing unquoted run.
//!
//! This reading was established by the E8 amendment in
//! `xx.hocon/docs/extra-spec-conventions.md` (rewritten 2026-05-20 as
//! xx.hocon#32 / commit `dd102e8`, driven by external issue xx.hocon#31). It
//! adopts Lightbend's pragmatic reading of HOCON.md L270-276 — "begin" =
//! value-position begin (first component of a concatenation), not
//! token-position begin at any lexer offset.
//!
//! Subst-body path expressions (`${-foo}`) and key-path segments
//! (`a.-foo = 1`) keep their existing strict checks — those rules are about
//! path-element composition, not value-position unquoted strings, and remain
//! out of E8 scope.

use std::collections::HashMap;
use std::path::PathBuf;

// ── Sidecar helpers (mirrors tests/concat_errors_test.rs pattern) ────────────

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/unquoted-starts")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/unquoted-starts")
}

fn fixture_path(stem: &str) -> PathBuf {
    fixture_dir().join(format!("{}.conf", stem))
}

fn expected_json_path(stem: &str) -> PathBuf {
    expected_dir().join(format!("{}-expected.json", stem))
}

fn normalize(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut m = serde_json::Map::new();
            for (k, val) in map {
                m.insert(k.clone(), normalize(val));
            }
            serde_json::Value::Object(m)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize).collect())
        }
        serde_json::Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            serde_json::json!(f)
        }
        other => other.clone(),
    }
}

fn hocon_to_json(v: &hocon::HoconValue) -> serde_json::Value {
    match v {
        hocon::HoconValue::Object(map) => {
            let mut m = serde_json::Map::new();
            for (k, val) in map {
                m.insert(k.clone(), hocon_to_json(val));
            }
            serde_json::Value::Object(m)
        }
        hocon::HoconValue::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(hocon_to_json).collect())
        }
        hocon::HoconValue::Scalar(sv) => match sv.value_type {
            hocon::ScalarType::Null => serde_json::Value::Null,
            hocon::ScalarType::Boolean => serde_json::Value::Bool(sv.raw == "true"),
            hocon::ScalarType::Number => {
                if !sv.raw.contains('.') && !sv.raw.contains('e') && !sv.raw.contains('E') {
                    if let Ok(n) = sv.raw.parse::<i64>() {
                        return serde_json::json!(n);
                    }
                }
                if let Ok(f) = sv.raw.parse::<f64>() {
                    return serde_json::json!(f);
                }
                serde_json::Value::String(sv.raw.clone())
            }
            hocon::ScalarType::String => serde_json::Value::String(sv.raw.clone()),
            _ => serde_json::Value::String(sv.raw.clone()),
        },
        _ => panic!("hocon_to_json: unknown HoconValue variant: {:?}", v),
    }
}

fn key_to_lookup_path(key: &str) -> String {
    if key.is_empty()
        || key.contains('.')
        || key.contains('"')
        || key.contains('\\')
        || key.contains(' ')
        || key.contains('\t')
    {
        let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    } else {
        key.to_string()
    }
}

fn config_to_json(config: &hocon::Config) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for key in config.keys() {
        let path = key_to_lookup_path(key);
        if let Some(val) = config.get(&path) {
            m.insert(key.to_string(), hocon_to_json(val));
        }
    }
    normalize(&serde_json::Value::Object(m))
}

fn run_fixture(stem: &str) {
    let fp = fixture_path(stem);
    let jp = expected_json_path(stem);

    assert!(
        fp.exists(),
        "fixture missing: {} — run `make testdata` to sync fixtures from xx.hocon",
        fp.display()
    );
    assert!(
        jp.exists(),
        "expected JSON missing: {} — run `make testdata` to sync expected sidecars from xx.hocon",
        jp.display()
    );

    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_file_with_env(&fp, &env);
    let cfg = result.unwrap_or_else(|e| {
        panic!(
            "unquoted-starts {}: unexpected error {:?} (fixture: {})",
            stem,
            e,
            fp.display()
        )
    });
    let got = config_to_json(&cfg);
    let json_src = std::fs::read_to_string(&jp)
        .unwrap_or_else(|e| panic!("failed to read expected JSON {}: {}", jp.display(), e));
    let expected: serde_json::Value = serde_json::from_str(&json_src)
        .unwrap_or_else(|e| panic!("invalid JSON in {}: {}", jp.display(), e));
    let expected = normalize(&expected);
    assert_eq!(got, expected, "unquoted-starts {}: JSON mismatch", stem);
}

// ── Success fixtures (post-E8 amendment) ─────────────────────────────────────
//
// Parse, resolve, and compare to xx.hocon expected JSON. us02/us03/us13
// joined this list as part of the E8 amendment (previously in ERROR_FIXTURES
// / known-gap tripwire under the strict reading). us17-us30 are new
// concat-continuation fixtures from probe groups A/B/D/E.

const SUCCESS_FIXTURES: &[&str] = &[
    "us01-digit-prefix-with-tail",
    "us02-hyphen-no-digit",
    "us03-hyphen-alone",
    "us04-hyphen-with-digit",
    "us05-number-then-comment",
    "us06-embedded-digits",
    "us07-embedded-hyphen",
    "us08-numeric-key-positive",
    "us09-dotted-number-key",
    "us10-greedy-backtrack-exp",
    "us11-greedy-backtrack-frac",
    "us12-hex-prefix",
    "us13-leading-zero",
    "us14-multi-dot-version",
    "us16-negative-with-tail",
    "us17-concat-subst-dash-text",
    "us18-concat-subst-dash-only",
    "us19-concat-subst-double-dash",
    "us20-concat-subst-dash-digit",
    "us21-concat-subst-digit-text",
    "us22-concat-subst-dot-text",
    "us23-concat-subst-underscore",
    "us24-concat-quoted-dash-text",
    "us25-concat-quoted-dot-text",
    "us26-concat-quoted-digit-text",
    "us27-concat-subst-dash-subst",
    "us28-concat-subst-dash-subst-other",
    "us29-concat-unquoted-dash-subst",
    "us30-concat-quoted-dash-subst",
];

#[test]
fn s8_6_success_fixtures_parse_and_resolve() {
    let mut failures: Vec<(&str, String)> = vec![];
    for name in SUCCESS_FIXTURES {
        let result = std::panic::catch_unwind(|| run_fixture(name));
        if let Err(e) = result {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                String::from("<unknown panic>")
            };
            failures.push((*name, msg));
        }
    }
    assert!(
        failures.is_empty(),
        "unquoted-starts: {} fixture failures: {:#?}",
        failures.len(),
        failures
    );
}

// ── Known gap tripwire ───────────────────────────────────────────────────────
//
// us15 (`a = 1e+x`) carries an `.error` sidecar from Lightbend (Reserved
// character `+` outside quotes). Lightbend's error fires at its value-parser
// layer; the `+` reservation is enforced in both value-start and concat-
// continuation positions per E8. rs.hocon currently does NOT reject `+` when
// it appears mid-unquoted-run with a non-`=` follower (e.g. `1e+x`). The
// #[should_panic] tripwire surfaces this gap automatically when it closes:
// the assertion currently panics (parse is Ok → assert! fails) and the
// attribute consumes the panic. If rs.hocon ever starts rejecting `1e+x`,
// the assertion passes → no panic → test FAILS.

#[test]
#[should_panic(expected = "us15-incomplete-exp")]
fn s8_6_us15_known_gap_tripwire() {
    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_file_with_env(fixture_path("us15-incomplete-exp"), &env);
    assert!(
        result.is_err(),
        "us15-incomplete-exp: Lightbend requires reject ('+' reservation mid-token), but rs.hocon currently parses (gap)"
    );
}

// ── E8 amendment explicit value-position tests ───────────────────────────────

fn parse(input: &str) -> hocon::Config {
    hocon::parse_with_env(input, &HashMap::new())
        .unwrap_or_else(|e| panic!("parse failed: {:?}", e))
}

fn get_scalar(cfg: &hocon::Config, key: &str) -> hocon::ScalarValue {
    match cfg.get(key) {
        Some(hocon::HoconValue::Scalar(sv)) => sv.clone(),
        Some(other) => panic!("expected scalar for {}, got {:?}", key, other),
        None => panic!("key {} not found", key),
    }
}

#[test]
fn e8_value_start_hyphen_no_digit_lexes_as_unquoted() {
    // RFC 8259 JSON-number requires a digit after `-`; bare `-foo` therefore
    // falls outside L270's disallow scope. Lightbend produces `{"a":"-foo"}`.
    let cfg = parse("a = -foo");
    let sv = get_scalar(&cfg, "a");
    assert_eq!(sv.value_type, hocon::ScalarType::String);
    assert_eq!(sv.raw, "-foo");
}

#[test]
fn e8_value_start_hyphen_alone_lexes_as_unquoted() {
    let cfg = parse("a = -");
    let sv = get_scalar(&cfg, "a");
    assert_eq!(sv.value_type, hocon::ScalarType::String);
    assert_eq!(sv.raw, "-");
}

#[test]
fn e8_value_start_leading_zero_preserves_lexeme_but_reads_as_number() {
    // `a = 01` is a digit-leading run coerced via the E8 greedy numeric path
    // (value_type = Number). S10.11 (go.hocon#133) refines the earlier F3
    // decision: a numeric value stringifies "as written in the source file",
    // so the stored `raw` must KEEP the original lexeme `"01"` for string
    // concatenation, while the typed/semantic accessors re-parse it. Lightbend
    // does the same — `${a}` in a string concat yields "01", but the standalone
    // value reads as 1 and renders as `1` in JSON.
    //
    // (Earlier this asserted `raw == "1"`; that over-canonicalized the lexeme
    // and was the go.hocon#133 bug — corrected here cross-impl.)
    let cfg = parse("a = 01");
    let sv = get_scalar(&cfg, "a");
    assert_eq!(sv.value_type, hocon::ScalarType::Number);
    assert_eq!(sv.raw, "01", "S10.11: numeric lexeme preserved as written");
    // Semantic value still canonical:
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
}

#[test]
fn e8_value_start_negative_zero_preserves_lexeme_but_reads_as_zero() {
    // `-0` reads as i64 = 0 (no negative zero in integer arithmetic), but its
    // source lexeme `"-0"` is preserved in `raw` for S10.11 stringification.
    let cfg = parse("a = -0");
    let sv = get_scalar(&cfg, "a");
    assert_eq!(sv.value_type, hocon::ScalarType::Number);
    assert_eq!(sv.raw, "-0", "S10.11: numeric lexeme preserved as written");
    assert_eq!(cfg.get_i64("a").unwrap(), 0);
}

#[test]
fn e8_value_start_negative_inf_is_string_not_number() {
    // Codex review (PR #98): Rust's `f64::parse("-inf")` succeeds and returns
    // f64::NEG_INFINITY, but Lightbend's parseDouble rejects `-inf` (Java
    // requires `-Infinity` or `Infinity`). rs.hocon must not classify `-inf`
    // as a number — the `-`-followed-by-digit guard at parse_scalar_value
    // keeps it on the string path.
    let cfg = parse("a = -inf");
    let sv = get_scalar(&cfg, "a");
    assert_eq!(
        sv.value_type,
        hocon::ScalarType::String,
        "E8: `-inf` must be unquoted string, not number (Lightbend parity)"
    );
    assert_eq!(sv.raw, "-inf");
}

#[test]
fn e8_value_start_negative_nan_is_string_not_number() {
    let cfg = parse("a = -nan");
    let sv = get_scalar(&cfg, "a");
    assert_eq!(sv.value_type, hocon::ScalarType::String);
    assert_eq!(sv.raw, "-nan");
}

#[test]
fn e8_value_start_inf_alone_is_string_not_number() {
    // `inf` (no minus) is not a JSON-number-shaped run (first char is `i`,
    // not a digit), so it already takes the string path. Regression guard.
    let cfg = parse("a = inf");
    let sv = get_scalar(&cfg, "a");
    assert_eq!(sv.value_type, hocon::ScalarType::String);
    assert_eq!(sv.raw, "inf");
}

#[test]
fn e8_plus_still_rejected_value_start() {
    // HOCON `+=` operator reservation: `+` is not in is_unquoted_start, so
    // `+foo` cannot start an unquoted run; lexer hits the catch-all
    // "unexpected character" branch.
    let result = hocon::parse_with_env("a = +foo", &HashMap::new());
    assert!(
        result.is_err(),
        "E8: '+' reservation must still reject value-start +foo, got {:?}",
        result.ok()
    );
}

// ── E8 concat-continuation explicit tests ────────────────────────────────────

#[test]
fn e8_concat_continuation_subst_dash_text() {
    let cfg = parse("a = foo\nb = ${a}-bar");
    let sv = get_scalar(&cfg, "b");
    assert_eq!(sv.raw, "foo-bar");
}

#[test]
fn e8_concat_continuation_quoted_dash_text() {
    let cfg = parse(r#"b = "foo"-bar"#);
    let sv = get_scalar(&cfg, "b");
    assert_eq!(sv.raw, "foo-bar");
}

#[test]
fn e8_concat_continuation_subst_digit_text() {
    let cfg = parse("a = foo\nb = ${a}1bar");
    let sv = get_scalar(&cfg, "b");
    assert_eq!(sv.raw, "foo1bar");
}

#[test]
fn e8_concat_continuation_subst_dot_text() {
    let cfg = parse("a = foo\nb = ${a}.bar");
    let sv = get_scalar(&cfg, "b");
    assert_eq!(sv.raw, "foo.bar");
}

#[test]
fn e8_plus_still_rejected_concat_continuation() {
    let result = hocon::parse_with_env("a = foo\nb = ${a}+bar", &HashMap::new());
    assert!(
        result.is_err(),
        "E8: '+' reservation must still reject concat-continuation, got {:?}",
        result.ok()
    );
}

// ── Out-of-E8-scope strict checks (unchanged) ────────────────────────────────
//
// The following rules apply to path-element composition (substitution body
// paths and dotted key segments), not to value-position unquoted strings.
// E8 amendment did not touch these — the strict rule is preserved.

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

// Regression: the parse_subst_body S8.6 check must fire only at **segment
// start** (gated on `!cur_started`). Quoted+unquoted concat within a segment
// — e.g. `${"a"-foo}` building key `"a-foo"` — must remain accepted.
#[test]
fn s8_6_subst_mid_segment_hyphen_after_quoted_allowed() {
    let input = r#"
"a-foo" = 1
x = ${"a"-foo}
"#;
    let result = hocon::parse_with_env(input, &HashMap::new());
    assert!(
        result.is_ok(),
        r#"${{"a"-foo}} must lex+resolve (quoted+unquoted concat → "a-foo"), got {:?}"#,
        result.err()
    );
}

// E13 (xx.hocon#42): S8.6 is NOT enforced on key path segments. The rule is
// value-position lexer-disambiguation; key paths are governed by path-element
// parsing rules. Lightbend accepts `a.-foo = 1` verbatim — pinned by the
// xx.hocon kh07 fixture. The prior strict-reject tests (which asserted the
// removed key-segment-S8.6 check) are inverted to assert success.
#[test]
fn e13_key_path_hyphen_segment_accepted_was_rejected_pre_e13() {
    // `a.-foo = 1` — the lexer sees `a.-foo` as one unquoted token; parse_key
    // splits on `.` and now treats `-foo` as a verbatim segment (Lightbend-aligned).
    let cfg = hocon::parse_with_env("a.-foo = 1", &HashMap::new())
        .expect("E13: a.-foo = 1 must parse (S8.6 not enforced on key paths)");
    assert_eq!(
        cfg.get_i64("a.\"-foo\"").unwrap(),
        1,
        "expected value at path a.-foo"
    );
}
