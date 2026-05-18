//! S10.4/S10.13/S10.19 — concat type-check conformance tests.
//!
//! Fixtures loaded from `tests/testdata/hocon/concat-errors/` (synced from
//! xx.hocon via `make testdata`). Expected-error marker from
//! `tests/testdata/expected/concat-errors/<name>.error` (sidecar presence means
//! "must produce an error"). Expected-success JSON from
//! `tests/testdata/expected/concat-errors/<name>-expected.json`.
//!
//! Convention:
//! - `.error` sidecar present → assert `parse_file(...).is_err()`
//! - `-expected.json` present → assert `parse_file(...).is_ok()` + compare JSON
//! - neither present → test is skipped (prints a note)
//!
//! Closes: rs.hocon#65 (S10.4), rs.hocon#67 (S10.13), rs.hocon#68 (S10.19).

use std::collections::HashMap;
use std::path::PathBuf;

/// Fixtures that are intentionally missing a sidecar because the Lightbend reference
/// implementation silently accepts them (quirk, not a conformance gap).
/// Source: xx.hocon GenerateExpected.java:150-155 for ce05.
const KNOWN_LIGHTBEND_QUIRKS: &[&str] = &["ce05-object-plus-scalar"];

// ── Paths ─────────────────────────────────────────────────────────────────────

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/concat-errors")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/concat-errors")
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

// ── JSON helpers (mirrors env_var_list_test.rs) ───────────────────────────────

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

    if !has_error && !has_json {
        if KNOWN_LIGHTBEND_QUIRKS.contains(&stem) {
            eprintln!(
                "note: concat-errors/{}.conf is a known Lightbend quirk — skipping validation",
                stem
            );
            return;
        }
        panic!(
            "concat-errors/{stem}.conf has no expected sidecar (.error or -expected.json).\n\
             Run `make testdata` first to fetch expected sidecars from xx.hocon.\n\
             If this fixture is intentionally unsupported by Lightbend, add \"{stem}\" to \
             KNOWN_LIGHTBEND_QUIRKS in tests/concat_errors_test.rs."
        );
    }

    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_file_with_env(&fp, &env);

    if has_error {
        assert!(
            result.is_err(),
            "concat-errors {}: expected parse/resolve error but got Ok (fixture: {})",
            stem,
            fp.display()
        );
    } else {
        // has_json
        let cfg = result.unwrap_or_else(|e| {
            panic!(
                "concat-errors {}: unexpected error {:?} (fixture: {})",
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
            "concat-errors {}: output mismatch\n  got:      {}\n  expected: {}",
            stem,
            serde_json::to_string_pretty(&got).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap(),
        );
    }
}

// ── ce01-ce15 ─────────────────────────────────────────────────────────────────

/// ce01: `a = [1] { b: 2 }` — array+object → error (S10.4)
#[test]
fn ce01_array_plus_object() {
    run_fixture("ce01-array-plus-object");
}

/// ce02: `a = { b: 2 } [1]` — object+array → error (S10.4)
#[test]
fn ce02_object_plus_array() {
    run_fixture("ce02-object-plus-array");
}

/// ce03: `a = [1, 2] 3` — array+scalar → error (S10.13)
#[test]
fn ce03_array_plus_scalar() {
    run_fixture("ce03-array-plus-scalar");
}

/// ce04: `a = 3 [1, 2]` — scalar+array → error (S10.13)
#[test]
fn ce04_scalar_plus_array() {
    run_fixture("ce04-scalar-plus-array");
}

/// ce05: `a = { b: 1 } x` — object+scalar → error (S10.13)
/// Note: no expected sidecar in xx.hocon at time of fixture sync; test skips.
#[test]
fn ce05_object_plus_scalar() {
    run_fixture("ce05-object-plus-scalar");
}

/// ce06: `a = x { b: 1 }` — scalar+object → error (S10.13)
#[test]
fn ce06_scalar_plus_object() {
    run_fixture("ce06-scalar-plus-object");
}

/// ce07: `obj = { b: 2 }\na = [1] ${obj}` — subst-resolved object+array → error (S10.19)
#[test]
fn ce07_subst_obj_plus_array() {
    run_fixture("ce07-subst-obj-plus-array");
}

/// ce08: `arr = [1]\na = ${arr} { b: 2 }` — subst-resolved array+object → error (S10.19)
#[test]
fn ce08_subst_array_plus_obj() {
    run_fixture("ce08-subst-array-plus-obj");
}

/// ce09: numeric-keyed object still converts via S15 — regression guard
#[test]
fn ce09_numeric_obj_still_works() {
    run_fixture("ce09-numeric-obj-still-works");
}

/// ce10: `a = [] {b:1}` — empty-array+object → error (S10.4 + S15.4)
#[test]
fn ce10_empty_array_plus_object() {
    run_fixture("ce10-empty-array-plus-object");
}

/// ce11: `a = [1] {}` — array+empty-object → error (S10.4 + S15.4)
#[test]
fn ce11_array_plus_empty_object() {
    run_fixture("ce11-array-plus-empty-object");
}

/// ce12: `arr = [1]\na = x ${arr}` — scalar+subst-resolved-array → error (S10.13/S10.19)
#[test]
fn ce12_string_concat_resolved_array() {
    run_fixture("ce12-string-concat-resolved-array");
}

/// ce13: `obj = { b: 1 }\na = x ${obj}` — scalar+subst-resolved-object → error (S10.13/S10.19)
#[test]
fn ce13_string_concat_resolved_object() {
    run_fixture("ce13-string-concat-resolved-object");
}

/// ce14: optional-missing mid-concat — omission leaves [1]+{b:2} → error (S10.4)
#[test]
fn ce14_optional_missing_mid_concat() {
    run_fixture("ce14-optional-missing-mid-concat");
}

/// ce15: optional-missing trailing — fold reduces to [1] → ok (positive case)
#[test]
fn ce15_optional_missing_suppresses_pair() {
    run_fixture("ce15-optional-missing-suppresses-pair");
}
