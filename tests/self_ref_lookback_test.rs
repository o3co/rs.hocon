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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/hocon/self-ref-lookback")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/expected/self-ref-lookback")
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
