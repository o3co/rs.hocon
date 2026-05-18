//! S12.5 — `include` reserved at start of key path — conformance tests.
//! Fixtures: tests/testdata/hocon/include-reservation/ (ir01–ir14).
//! Sidecars:  tests/testdata/expected/include-reservation/.
//!
//! Convention:
//! - `.error` sidecar present → assert `parse_file_with_env(...).is_err()`
//! - `-expected.json` present → assert `parse_file_with_env(...).is_ok()` + compare JSON
//! - neither present → checked against KNOWN_LIGHTBEND_QUIRKS; error if not listed
//!
//! ir03/ir04 have no xx.hocon sidecar (Lightbend silently accepts them
//! due to tokenizer quirk — see spec §Strict-spec posture). These are
//! listed in KNOWN_LIGHTBEND_QUIRKS and tested via per-impl override
//! functions that assert is_err() directly (strict HOCON.md compliance).

use std::collections::HashMap;
use std::path::PathBuf;

/// Fixtures that are intentionally missing a sidecar because the Lightbend reference
/// implementation silently accepts them (quirk, not a conformance gap).
/// rs.hocon enforces strict HOCON.md compliance; see ir03_per_impl / ir04_per_impl below.
const KNOWN_LIGHTBEND_QUIRKS: &[&str] = &[
    "ir03-include-dot-foo-equals",
    "ir04-include-nested-object",
];

// ── Paths ─────────────────────────────────────────────────────────────────────

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/hocon/include-reservation")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/expected/include-reservation")
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

// ── JSON helpers (mirrors concat_errors_test.rs) ──────────────────────────────

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
                "note: include-reservation/{}.conf is a known Lightbend quirk — \
                 skipping Lightbend-sidecar validation (per-impl override test applies instead)",
                stem
            );
            return;
        }
        panic!(
            "include-reservation/{stem}.conf has no expected sidecar (.error or -expected.json).\n\
             Run `make testdata` first to fetch expected sidecars from xx.hocon.\n\
             If this fixture is intentionally unsupported by Lightbend, add \"{stem}\" to \
             KNOWN_LIGHTBEND_QUIRKS in tests/include_reservation_test.rs."
        );
    }

    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_file_with_env(&fp, &env);

    if has_error {
        assert!(
            result.is_err(),
            "include-reservation {}: expected parse/resolve error but got Ok (fixture: {})",
            stem,
            fp.display()
        );
    } else {
        // has_json
        let cfg = result.unwrap_or_else(|e| {
            panic!(
                "include-reservation {}: unexpected error {:?} (fixture: {})",
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
            "include-reservation {}: output mismatch\n  got:      {}\n  expected: {}",
            stem,
            serde_json::to_string_pretty(&got).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap(),
        );
    }
}

// ── ir01–ir14 ─────────────────────────────────────────────────────────────────

/// ir01: `include = 1` — parse error (S12.5)
#[test]
fn ir01_include_equals() {
    run_fixture("ir01-include-equals");
}

/// ir02: `include : 1` — parse error (S12.5)
#[test]
fn ir02_include_colon() {
    run_fixture("ir02-include-colon");
}

/// ir03: `include.foo = 1` — Lightbend quirk (silently accepts); rs.hocon enforces S12.5
/// Lightbend-sidecar validation skipped via KNOWN_LIGHTBEND_QUIRKS.
/// Per-impl override: ir03_include_dot_foo_per_impl below asserts is_err().
#[test]
fn ir03_include_dot_foo() {
    run_fixture("ir03-include-dot-foo-equals");
}

/// ir04: `a = { include.bar = 1 }` — Lightbend quirk; rs.hocon enforces S12.5
#[test]
fn ir04_include_nested_object() {
    run_fixture("ir04-include-nested-object");
}

/// ir05: `include "ir05-inner.conf"` — include statement still works (regression guard)
#[test]
fn ir05_include_statement() {
    run_fixture("ir05-include-statement");
}

/// ir06: `"include" = 1` — quoted bypasses reservation, resolves to { include: 1 }
#[test]
fn ir06_quoted_include() {
    run_fixture("ir06-quoted-include");
}

/// ir07: `foo.include = 1` — non-initial include is allowed
#[test]
fn ir07_include_non_initial() {
    run_fixture("ir07-include-non-initial");
}

/// ir08: `a = include` — value-position include is an unquoted string, not a key
#[test]
fn ir08_include_as_value() {
    run_fixture("ir08-include-as-value");
}

/// ir09: `include "ir09-inner.conf"` via file() form (regression guard)
#[test]
fn ir09_include_file_form() {
    run_fixture("ir09-include-file-form");
}

/// ir10: `include += [1]` — += separator form, parse error (S12.5)
#[test]
fn ir10_include_plus_equals() {
    run_fixture("ir10-include-plus-equals");
}

/// ir11: `"include".foo = 1` — quoted-dotted bypass; resolves to { include: { foo: 1 } }
#[test]
fn ir11_quoted_include_dotted() {
    run_fixture("ir11-quoted-include-dotted");
}

/// ir12: `include\nfoo.conf` — existing include-statement parse error wins (not S12.5)
#[test]
fn ir12_include_newline_arg() {
    run_fixture("ir12-include-newline-arg");
}

/// ir13: `include { x = 1 }` — object-body form, parse error (S12.5)
#[test]
fn ir13_include_object_body() {
    run_fixture("ir13-include-object-body");
}

/// ir14: `a = ${include}` — substitution paths are NOT subject to the reservation
#[test]
fn ir14_substitution_include_path() {
    run_fixture("ir14-substitution-include-path");
}

// ── Per-impl overrides for Lightbend quirks ───────────────────────────────────

/// ir03 per-impl: rs.hocon strictly enforces S12.5 — `include.foo = 1` must fail.
#[test]
fn ir03_include_dot_foo_per_impl() {
    let fp = fixture_path("ir03-include-dot-foo-equals");
    let env: HashMap<String, String> = HashMap::new();
    assert!(
        hocon::parse_file_with_env(&fp, &env).is_err(),
        "ir03: include.foo = 1 must be a parse error (S12.5, HOCON.md L570)"
    );
}

/// ir04 per-impl: rs.hocon strictly enforces S12.5 — `a = {{ include.bar = 1 }}` must fail.
#[test]
fn ir04_include_nested_object_per_impl() {
    let fp = fixture_path("ir04-include-nested-object");
    let env: HashMap<String, String> = HashMap::new();
    assert!(
        hocon::parse_file_with_env(&fp, &env).is_err(),
        "ir04: a = {{ include.bar = 1 }} must be a parse error (S12.5, HOCON.md L570)"
    );
}
