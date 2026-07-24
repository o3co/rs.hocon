//! S23.5 / S23.6 — full `java.util.Properties` syntax, against xx.hocon fixtures.
//!
//! Fixtures: tests/testdata/hocon/properties-syntax/ps01-ps05.properties
//! Expected: tests/testdata/expected/properties-syntax/*-expected.json,
//! generated from Lightbend, which reads an included .properties file with
//! java.util.Properties.
//!
//! Both items were globally out-of-scope until 2026-07-24. The fixtures cover
//! backslash continuations, the escape set, the three separator forms, escaped
//! separators belonging to the key, a value keeping its trailing whitespace,
//! and astral characters written both as a surrogate pair and directly.
//!
//! Loaded through a temporary `include` wrapper so the real resolver path is
//! what gets checked, matching conformance_properties_conflict.rs.

use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/properties-syntax")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/properties-syntax")
}

fn hocon_to_json(v: &hocon::HoconValue) -> serde_json::Value {
    match v {
        hocon::HoconValue::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, val)| (k.clone(), hocon_to_json(val)))
                .collect(),
        ),
        hocon::HoconValue::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(hocon_to_json).collect())
        }
        // Every value from a .properties file is a string (S23.3), so no
        // numeric coercion is wanted here — it would mask a wrong raw value.
        hocon::HoconValue::Scalar(sv) => serde_json::Value::String(sv.raw.clone()),
        other => panic!("unexpected variant: {other:?}"),
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
        format!("\"{escaped}\"")
    } else {
        key.to_string()
    }
}

fn config_to_json(config: &hocon::Config) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for key in config.keys() {
        if let Some(val) = config.get(&key_to_lookup_path(key)) {
            m.insert(key.to_string(), hocon_to_json(val));
        }
    }
    serde_json::Value::Object(m)
}

fn run_ps_fixture(stem: &str) {
    let props_path = fixture_dir().join(format!("{stem}.properties"));
    let expected_path = expected_dir().join(format!("{stem}-expected.json"));

    let path_str = props_path.to_string_lossy().replace('\\', "/");
    let wrapper = format!(r#"include file("{path_str}")"#);

    let env: HashMap<String, String> = HashMap::new();
    let cfg = hocon::parse_with_env(&wrapper, &env)
        .unwrap_or_else(|e| panic!("properties-syntax/{stem}.properties: unexpected error {e:?}"));

    let got = config_to_json(&cfg);

    let json_src = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", expected_path.display(), e));
    let expected: serde_json::Value = serde_json::from_str(&json_src)
        .unwrap_or_else(|e| panic!("invalid JSON in {}: {}", expected_path.display(), e));

    assert_eq!(
        got,
        expected,
        "properties-syntax {stem}: mismatch against the Lightbend oracle\n  got:      {}\n  expected: {}",
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap(),
    );
}

/// ps01: backslash continuations, and the runs that are not continuations.
#[test]
fn ps01_continuation() {
    run_ps_fixture("ps01-continuation");
}

/// ps02: the escape set, an unknown escape, and a literal backslash.
#[test]
fn ps02_escapes() {
    run_ps_fixture("ps02-escapes");
}

/// ps03: `=`, `:` and whitespace separators; escaped separators stay in the key.
#[test]
fn ps03_separators() {
    run_ps_fixture("ps03-separators");
}

/// ps04: a value keeps its trailing whitespace.
#[test]
fn ps04_value_whitespace() {
    run_ps_fixture("ps04-value-whitespace");
}

/// ps05: a surrogate pair and a directly written astral character.
#[test]
fn ps05_astral() {
    run_ps_fixture("ps05-astral");
}
