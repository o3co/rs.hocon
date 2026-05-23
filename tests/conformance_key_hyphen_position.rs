//! Key-position S8.6 conformance (xx.hocon#42, E13) — kh01-kh08 fixtures.
//!
//! Fixture suite: tests/testdata/hocon/key-hyphen-position/kh01..kh08.conf
//! Expected JSON: tests/testdata/expected/key-hyphen-position/kh*-expected.json
//!
//! HOCON.md L270-276 (S8.6) forbids unquoted strings from BEGINNING with `-`
//! (unless followed by a digit). That rule is value-position only: Lightbend's
//! path parser accepts hyphen-start segments verbatim in field-key position.
//! rs.hocon previously over-enforced S8.6 on every dot-split key segment,
//! rejecting all 8 cases. See xx.hocon docs/extra-spec-conventions.md E13.
//!
//! All 8 fixtures are success fixtures (no .error sidecars). If a fixture is
//! missing, the test panics with guidance to run `make testdata`.

use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/key-hyphen-position")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/key-hyphen-position")
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

fn run_kh_fixture(stem: &str) {
    let fp = fixture_dir().join(format!("{}.conf", stem));
    let jp = expected_dir().join(format!("{}-expected.json", stem));

    if !fp.exists() {
        panic!(
            "key-hyphen-position/{stem}.conf is missing — run `make testdata` to fetch fixtures \
             from xx.hocon (E13 cluster, xx.hocon#42)."
        );
    }
    if !jp.exists() {
        panic!(
            "key-hyphen-position/{stem}-expected.json is missing — run `make testdata` \
             to fetch expected sidecars from xx.hocon (E13 cluster, xx.hocon#42)."
        );
    }

    let env: HashMap<String, String> = HashMap::new();
    let cfg = hocon::parse_file_with_env(&fp, &env).unwrap_or_else(|e| {
        panic!(
            "key-hyphen-position/{stem}.conf: unexpected parse error — {e}\n\
             rs.hocon should accept hyphen-start segments in key position per E13 \
             (xx.hocon#42). HOCON.md L270-276 S8.6 is value-position only."
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
        "key-hyphen-position {stem}: output mismatch\n  got:      {}\n  expected: {}",
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap(),
    );
}

#[test]
fn kh01_space_concat_hyphen_tail() {
    run_kh_fixture("kh01-space-concat-hyphen-tail")
}
#[test]
fn kh02_dotted_then_space_hyphen_tail() {
    run_kh_fixture("kh02-dotted-then-space-hyphen-tail")
}
#[test]
fn kh03_quoted_then_space_hyphen_tail() {
    run_kh_fixture("kh03-quoted-then-space-hyphen-tail")
}
#[test]
fn kh04_space_concat_dot_hyphen_start() {
    run_kh_fixture("kh04-space-concat-dot-hyphen-start")
}
#[test]
fn kh05_first_token_hyphen_start() {
    run_kh_fixture("kh05-first-token-hyphen-start")
}
#[test]
fn kh06_trailing_hyphen_only() {
    run_kh_fixture("kh06-trailing-hyphen-only")
}
#[test]
fn kh07_dot_hyphen_start_segment() {
    run_kh_fixture("kh07-dot-hyphen-start-segment")
}
#[test]
fn kh08_space_concat_hyphen_digit_tail() {
    run_kh_fixture("kh08-space-concat-hyphen-digit-tail")
}
