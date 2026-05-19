//! S23.4 — properties object-wins conformance tests against xx.hocon fixtures.
//!
//! Fixtures: tests/testdata/hocon/properties-conflict/pc01-pc04.properties
//! Expected: tests/testdata/expected/properties-conflict/*-expected.json
//!
//! Strategy: each test creates a temporary HOCON wrapper file that includes
//! the properties fixture (using `include`), then compares parse output against
//! the expected JSON sidecar.
//!
//! RED: fails until S23.4 fix is applied to `set_nested` in `src/properties.rs`.

use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/properties-conflict")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/properties-conflict")
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
        _ => panic!("hocon_to_json: unexpected variant: {:?}", v),
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

fn run_pc_fixture(stem: &str) {
    let props_path = fixture_dir().join(format!("{}.properties", stem));
    let expected_path = expected_dir().join(format!("{}-expected.json", stem));

    // Create a temporary HOCON wrapper file that includes the properties file.
    // The include path must be absolute so the loader finds it.
    let wrapper_content = format!(r#"include file("{}")"#, props_path.display());

    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_with_env(&wrapper_content, &env);

    let cfg = result.unwrap_or_else(|e| {
        panic!(
            "properties-conflict/{}.properties: unexpected error {:?}",
            stem, e
        )
    });

    let got = config_to_json(&cfg);

    let json_src = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", expected_path.display(), e));
    let expected: serde_json::Value = serde_json::from_str(&json_src)
        .unwrap_or_else(|e| panic!("invalid JSON in {}: {}", expected_path.display(), e));
    let expected = normalize(&expected);

    assert_eq!(
        got,
        expected,
        "properties-conflict {}: output mismatch\n  got:      {}\n  expected: {}",
        stem,
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap(),
    );
}

/// pc01: `a=hello\na.b=world` (forward order) → `{a: {b: "world"}}` (object wins).
#[test]
fn pc01_forward_object_wins() {
    run_pc_fixture("pc01-forward");
}

/// pc02: `a.b=world\na=hello` (reverse order) → same result as pc01 (sort eliminates order).
#[test]
fn pc02_reverse_same_result() {
    run_pc_fixture("pc02-reverse");
}

/// pc03: `a.b.c=v1\na.b=v2` (deep forward) → `{a: {b: {c: "v1"}}}` (object wins at deep level).
#[test]
fn pc03_deep_forward_object_wins() {
    run_pc_fixture("pc03-deep-forward");
}

/// pc04: `a.b=v1\na.b.c=v2` (deep reverse) → `{a: {b: {c: "v2"}}}` (scalar at a.b discarded).
#[test]
fn pc04_deep_reverse_object_wins() {
    run_pc_fixture("pc04-deep-reverse");
}
