//! Path-expression whitespace preservation (xx.hocon#42, E13) — pw01-pw07.
//!
//! Fixture suite: tests/testdata/hocon/path-expr-whitespace/pw01..pw07.conf
//! Expected JSON: tests/testdata/expected/path-expr-whitespace/pw*-expected.json
//! Error sidecar: pw06-trailing-dot-before-separator.error (BadPath, boundary guard)
//!
//! Lightbend preserves literal whitespace adjacent to dots in path expressions:
//!   a b. c = 1   →  {"a b":{" c":1}}     // leading space on " c" preserved
//!   a b.\tc = 1  →  {"a b":{"\tc":1}}    // tab preserved (HOCON_WS includes tab)
//! rs.hocon previously stripped leading whitespace from post-dot segments.
//! See xx.hocon docs/extra-spec-conventions.md E13.
//!
//! 6 success fixtures + 1 error fixture (pw06: trailing-dot still BadPath —
//! loosening does NOT cascade into empty path segments).

use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/path-expr-whitespace")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/path-expr-whitespace")
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
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) => serde_json::json!(f),
            None => serde_json::Value::Number(n.clone()),
        },
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

fn run_pw_success(stem: &str) {
    let fp = fixture_dir().join(format!("{}.conf", stem));
    let jp = expected_dir().join(format!("{}-expected.json", stem));

    if !fp.exists() {
        panic!("path-expr-whitespace/{stem}.conf is missing — run `make testdata`.");
    }
    if !jp.exists() {
        panic!("path-expr-whitespace/{stem}-expected.json is missing — run `make testdata`.");
    }

    let env: HashMap<String, String> = HashMap::new();
    let cfg = hocon::parse_file_with_env(&fp, &env).unwrap_or_else(|e| {
        panic!(
            "path-expr-whitespace/{stem}.conf: unexpected parse error — {e}\n\
             rs.hocon should preserve literal whitespace adjacent to dots in \
             path expressions per E13 (xx.hocon#42)."
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
        "path-expr-whitespace {stem}: output mismatch\n  got:      {}\n  expected: {}",
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap(),
    );
}

fn run_pw_error(stem: &str) {
    let fp = fixture_dir().join(format!("{}.conf", stem));
    let ep = expected_dir().join(format!("{}.error", stem));

    if !fp.exists() {
        panic!("path-expr-whitespace/{stem}.conf is missing — run `make testdata`.");
    }
    // Sidecar existence is the signal (per xx.hocon docs/fixture-conventions.md);
    // message content is not asserted across impls.
    assert!(
        ep.exists(),
        "path-expr-whitespace/{stem}.error sidecar is missing — run `make testdata`."
    );

    let env: HashMap<String, String> = HashMap::new();
    let result = hocon::parse_file_with_env(&fp, &env);
    assert!(
        result.is_err(),
        "path-expr-whitespace {stem}: expected parse error per .error sidecar, got Ok({:?})",
        result.ok().map(|c| config_to_json(&c))
    );
}

#[test]
fn pw01_space_after_dot() {
    run_pw_success("pw01-space-after-dot")
}
#[test]
fn pw02_space_both_sides_of_dot() {
    run_pw_success("pw02-space-both-sides-of-dot")
}
#[test]
fn pw03_space_before_dot() {
    run_pw_success("pw03-space-before-dot")
}
#[test]
fn pw04_space_concat_both_segments() {
    run_pw_success("pw04-space-concat-both-segments")
}
#[test]
fn pw05_multi_whitespace_both_sides() {
    run_pw_success("pw05-multi-whitespace-both-sides")
}
#[test]
fn pw07_tab_after_dot() {
    run_pw_success("pw07-tab-after-dot")
}

#[test]
fn pw06_trailing_dot_before_separator_errors() {
    run_pw_error("pw06-trailing-dot-before-separator")
}
