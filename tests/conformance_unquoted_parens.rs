//! Unquoted-parens conformance tests against xx.hocon fixtures (up01–up06).
//!
//! Fixture suite: tests/testdata/hocon/unquoted-parens/ — individual files
//! `up01-paren-mid-token.conf` through `up06-paren-unbalanced-close.conf`
//! (the `up01–up06` range notation is a shorthand for the suite, not a
//! single file path).
//! Expected JSON: tests/testdata/expected/unquoted-parens/up0N-<slug>-expected.json
//!
//! Background: HOCON.md L274 specifies a forbidden set for unquoted strings
//! that does NOT include `(` or `)`. External report xx.hocon#34 (@cgordon)
//! surfaced that some implementations incorrectly exclude parens. rs.hocon is
//! spec-compliant: `is_unquoted_start` (src/lexer.rs:811) and
//! `is_unquoted_continue` (src/lexer.rs:837) both omit parens from their
//! exclusion lists, so all 6 fixtures parse correctly with no impl change.
//!
//! These tests exist purely to pin regression coverage for already-correct
//! behaviour. The upstream spec PR is xx.hocon#35 (merged 2026-05-21,
//! commit 5b9c1ba). go.hocon is the only cross-impl outlier and gets a
//! separate impl-fix PR (o3co/go.hocon#100).
//!
//! Convention: all up01–up06 are success fixtures (no `.error` sidecar).
//!
//! If a fixture file is missing, this test panics with guidance to run
//! `make testdata`.

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/unquoted-parens")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/unquoted-parens")
}

fn fixture_path(stem: &str) -> PathBuf {
    fixture_dir().join(format!("{}.conf", stem))
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
            // Normalize int↔float representation across JSON serializers
            // (e.g. `1` vs `1.0`) when the value fits in f64. Numbers outside
            // the f64-representable range (i64 outside [-2^53, 2^53]) are
            // preserved as-is to avoid silently collapsing distinct numbers
            // to 0.0 — flagged by Copilot review on PR#102.
            match n.as_f64() {
                Some(f) => serde_json::json!(f),
                None => serde_json::Value::Number(n.clone()),
            }
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

fn run_up_fixture(stem: &str) {
    let fp = fixture_path(stem);
    let jp = expected_json_path(stem);

    if !fp.exists() {
        panic!(
            "unquoted-parens/{stem}.conf is missing — run `make testdata` to fetch \
             fixtures from xx.hocon (commit 5b9c1ba, PR #35)."
        );
    }
    if !jp.exists() {
        panic!(
            "unquoted-parens/{stem}-expected.json is missing — run `make testdata` \
             to fetch expected sidecars from xx.hocon (commit 5b9c1ba, PR #35)."
        );
    }

    // Use parse_file_with_env with an empty env so the test is deterministic
    // regardless of the runner's `std::env::vars()` — matches the pattern in
    // tests/conformance_empty_file.rs and other fixture-driven conformance
    // tests. Flagged by Copilot review on PR#102.
    let env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let cfg = hocon::parse_file_with_env(&fp, &env).unwrap_or_else(|e| {
        panic!(
            "unquoted-parens/{stem}.conf: unexpected parse error — {e}\n\
             rs.hocon should accept parens in unquoted values per HOCON.md L274 \
             (forbidden set does NOT include `(` or `)`)."
        )
    });

    let got = config_to_json(&cfg);

    let json_src = std::fs::read_to_string(&jp)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", jp.display(), e));
    let expected: serde_json::Value = serde_json::from_str(&json_src)
        .unwrap_or_else(|e| panic!("invalid JSON in {}: {}", jp.display(), e));
    let expected = normalize(&expected);

    assert_eq!(
        got,
        expected,
        "unquoted-parens {stem}: output mismatch\n  got:      {}\n  expected: {}",
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap(),
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// up01: `a = hello (world)` → parens mid-token are allowed in unquoted values.
#[test]
fn up01_paren_mid_token() {
    run_up_fixture("up01-paren-mid-token");
}

/// up02: `a = (internal)` → paren-leading unquoted value is allowed.
#[test]
fn up02_paren_leading() {
    run_up_fixture("up02-paren-leading");
}

/// up03: `description = Build API spec for Dependency Security service (internal)`
///       → parens in real-world prose (the motivating case from xx.hocon#34).
#[test]
fn up03_paren_real_world() {
    run_up_fixture("up03-paren-real-world");
}

/// up04: `a = ((nested))` → multiple/nested parens are allowed.
#[test]
fn up04_paren_nested() {
    run_up_fixture("up04-paren-nested");
}

/// up05: `a = (foo` → unbalanced open paren is allowed (no matching requirement).
#[test]
fn up05_paren_unbalanced_open() {
    run_up_fixture("up05-paren-unbalanced-open");
}

/// up06: `a = foo)` → unbalanced close paren is allowed.
#[test]
fn up06_paren_unbalanced_close() {
    run_up_fixture("up06-paren-unbalanced-close");
}
