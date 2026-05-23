//! S13a.13 — optional self-ref look-back conformance tests.
//!
//! Fixtures loaded from `tests/testdata/hocon/self-ref-lookback/` (synced from
//! xx.hocon via `make testdata`). Expected sidecars from
//! `tests/testdata/expected/self-ref-lookback/`.
//!
//! Convention:
//! - `.error` sidecar present → assert `parse_file(...).is_err()`
//! - `-expected.json` present → assert `parse_file(...).is_ok()` + compare JSON
//!
//! Closes: rs.hocon#76 (S13a.13 self-ref look-back fix)

use std::collections::HashMap;
use std::path::PathBuf;

// ── Paths ─────────────────────────────────────────────────────────────────────

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/self-ref-lookback")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/self-ref-lookback")
}

fn fixture_path(stem: &str) -> PathBuf {
    fixture_dir().join(format!("{}.conf", stem))
}

fn error_sidecar_path(stem: &str) -> PathBuf {
    expected_dir().join(format!("{}.error", stem))
}

fn expected_json_path(stem: &str) -> PathBuf {
    expected_dir().join(format!("{}-expected.json", stem))
}

// ── JSON helpers ──────────────────────────────────────────────────────────────

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

// ── Fixture runner ────────────────────────────────────────────────────────────

fn run_fixture(stem: &str) {
    let fp = fixture_path(stem);
    let ep = error_sidecar_path(stem);
    let jp = expected_json_path(stem);

    assert!(
        fp.exists(),
        "fixture missing: {} — run `make testdata` to sync fixtures from xx.hocon",
        fp.display()
    );

    let has_error = ep.exists();
    let has_json = jp.exists();

    assert!(
        has_error || has_json,
        "self-ref-lookback/{stem}.conf has no expected sidecar (.error or -expected.json).\n\
         Run `make testdata` first to fetch expected sidecars from xx.hocon."
    );

    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_file_with_env(&fp, &env);

    if has_error {
        assert!(
            result.is_err(),
            "self-ref-lookback {}: expected parse/resolve error but got Ok (fixture: {})",
            stem,
            fp.display()
        );
    } else {
        let cfg = result.unwrap_or_else(|e| {
            panic!(
                "self-ref-lookback {}: unexpected error {:?} (fixture: {})",
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

        assert_eq!(
            got,
            expected,
            "self-ref-lookback {}: output mismatch\n  got:      {}\n  expected: {}",
            stem,
            serde_json::to_string_pretty(&got).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap(),
        );
    }
}

// ── sr01–sr11 ─────────────────────────────────────────────────────────────────

/// sr01: `a = ${?a}foo` no prior → `"foo"` (core fix)
#[test]
fn sr01_optional_no_prior() {
    run_fixture("sr01-optional-no-prior");
}

/// sr02: `a = bar${?a}` no prior → `"bar"` (leading literal)
#[test]
fn sr02_optional_no_prior_leading() {
    run_fixture("sr02-optional-no-prior-leading");
}

/// sr03: `a = bar${?a}foo` no prior → `"barfoo"` (literal on both sides)
#[test]
fn sr03_optional_no_prior_both_sides() {
    run_fixture("sr03-optional-no-prior-both-sides");
}

/// sr04: `a = "x"; a = ${?a}foo` → `"xfoo"` (regression: prior value used)
#[test]
fn sr04_optional_with_prior() {
    run_fixture("sr04-optional-with-prior");
}

/// sr05: `a = ${a}foo` no prior → resolve error (required ref boundary)
#[test]
fn sr05_required_no_prior() {
    run_fixture("sr05-required-no-prior");
}

/// sr06: `a = "x"; a = ${a}foo` → `"xfoo"` (regression: required + prior)
#[test]
fn sr06_required_with_prior() {
    run_fixture("sr06-required-with-prior");
}

/// sr07: `a = ${?a} [2]` no prior → `[2]` (array variant)
#[test]
fn sr07_array_optional_no_prior() {
    run_fixture("sr07-array-optional-no-prior");
}

/// sr08: `a = [1]; a = ${?a} [2]` → `[1, 2]` (regression: array with prior)
#[test]
fn sr08_array_optional_with_prior() {
    run_fixture("sr08-array-optional-with-prior");
}

/// sr09: `foo.a = ${?foo.a}bar` no prior → `foo.a = "bar"` (nested path)
#[test]
fn sr09_nested_no_prior() {
    run_fixture("sr09-nested-no-prior");
}

/// sr10: `foo.a = "x"; foo.a = ${?foo.a}bar` → `foo.a = "xbar"` (nested regression)
#[test]
fn sr10_nested_with_prior() {
    run_fixture("sr10-nested-with-prior");
}

/// sr11: mutual forward-ref — not a self-ref; standard forward-ref resolution (regression guard)
#[test]
fn sr11_mutual_ref_forward() {
    run_fixture("sr11-mutual-ref-forward");
}

/// (cluster 3f, NOT the cluster 3h sr12): object-literal form
/// `foo { a = "x"\n a = ${?foo.a}bar }` → `foo.a = "xbar"`. Regression guard
/// for AST normalization unifying object-literal and dotted-path forms.
/// Note: HOCON field separator is LF (0x0A); semicolons are not newline
/// equivalents in this parser, so the multi-field form requires a literal
/// newline inside the object-literal block.
#[test]
fn s13a_13_nested_self_ref_object_literal_form() {
    let cfg = hocon::parse_with_env(
        "foo {\n  a = \"x\"\n  a = ${?foo.a}bar\n}",
        &std::collections::HashMap::new(),
    )
    .expect("parse failed");
    assert_eq!(cfg.get_string("foo.a").unwrap(), "xbar");
}

// ── sr12–sr16 (xx.hocon#27 cluster 3h follow-ups) ────────────────────────────
//
// 4 cross-impl resolver bugs surfaced by Round-2 multi-agent-review of the
// S13a.13 cluster 3f PRs. See xx.hocon E14 for the convention. Pre-fix rs.hocon
// status:
//   sr12 FAIL-CRASH (stack overflow) — cycle handler clones the resolving set
//   sr13 FAIL-WRONG (foo.a="xbarbar"; expected "xbar") — fold_nested_self_refs
//        overwrites prior with already-folded form on the 3rd field write
//   sr14 FAIL-WRONG (b="x" vs "xfoo") — cache pollution: is_self_ref branch
//        writes prior to cache, external lookup reads stale entry
//   sr15 FAIL-WRONG (a="2" vs "12") — fold_or_skip_prior skips when prior
//        contains self-ref AND no old prior (universal failure cross-impl)
//   sr16 FAIL-WRONG (a="foofoo" vs "foo") — cache pollution: external caller
//        traverses self-ref field's concat, caches preview value, then
//        self-ref field reads stale cache

/// sr12: `foo.a = ${?foo.a}bar; foo.b = ${foo.a}` → `{foo:{a:"bar",b:"bar"}}`
/// Pre-fix: stack overflow (cycle handler bug in resolve_subst_inner).
#[test]
fn sr12_nested_external_ref_no_prior() {
    run_fixture("sr12-nested-external-ref-no-prior");
}

/// sr13: `foo.a = "x"; foo.a = ${?foo.a}bar; foo.b = ${foo.a}` → both `"xbar"`.
/// Pre-fix: `foo.a="xbarbar", foo.b="xbar"` — prior-overwrite-with-folded.
#[test]
fn sr13_nested_external_ref_with_prior() {
    run_fixture("sr13-nested-external-ref-with-prior");
}

/// sr14: `a = "x"; a = ${?a}foo; b = ${a}` → both `"xfoo"`.
/// Pre-fix: `a="xfoo", b="x"` — cache pollution from is_self_ref branch.
#[test]
fn sr14_cache_prior_external() {
    run_fixture("sr14-cache-prior-external");
}

/// sr15: `a = ${?a}1; a = ${?a}2` → `"12"`.
/// Pre-fix: `"2"` — fold_or_skip_prior drops first concat (universal cross-impl bug).
#[test]
fn sr15_double_self_ref() {
    run_fixture("sr15-double-self-ref");
}

/// sr16: `b = ${a}; a = ${?a}foo` → `a="foo", b="foo"` (order-independent).
/// Pre-fix: `a="foofoo", b="foo"` — cache pollution from external preview.
#[test]
fn sr16_external_before_self_ref() {
    run_fixture("sr16-external-before-self-ref");
}
