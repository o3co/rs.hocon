use std::fs;
use std::path::PathBuf;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon")
}

/// Normalize a serde_json::Value so all numbers are f64
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

/// Convert a HoconValue to serde_json::Value for comparison
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
        hocon::HoconValue::Scalar(s) => match s {
            hocon::ScalarValue::String(s) => serde_json::Value::String(s.clone()),
            hocon::ScalarValue::Int(n) => serde_json::json!(*n as f64),
            hocon::ScalarValue::Float(f) => serde_json::json!(*f),
            hocon::ScalarValue::Bool(b) => serde_json::Value::Bool(*b),
            hocon::ScalarValue::Null => serde_json::Value::Null,
        },
    }
}

fn config_to_json(config: &hocon::Config) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for key in config.keys() {
        if let Some(val) = config.get(key) {
            m.insert(key.to_string(), hocon_to_json(val));
        }
    }
    normalize(&serde_json::Value::Object(m))
}

fn parse_and_compare(conf_path: &std::path::Path, json_path: &std::path::Path) {
    let config = hocon::parse_file(conf_path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", conf_path.display(), e));

    let got = config_to_json(&config);

    let expected_str = fs::read_to_string(json_path)
        .unwrap_or_else(|e| panic!("read {}: {}", json_path.display(), e));
    let expected: serde_json::Value = serde_json::from_str(&expected_str)
        .unwrap_or_else(|e| panic!("parse {}: {}", json_path.display(), e));
    let expected = normalize(&expected);

    assert_eq!(
        got, expected,
        "mismatch for {}\ngot:\n{}\nwant:\n{}",
        conf_path.display(),
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap()
    );
}

#[test]
fn lightbend_equiv01() {
    let dir = testdata_dir().join("equiv01");
    let json_path = dir.join("original.json");
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        let name = path.file_name().unwrap().to_str().unwrap();
        // Test .conf files and non-original .json files
        if ext == Some("conf") || (ext == Some("json") && name != "original.json") {
            parse_and_compare(&path, &json_path);
        }
    }
}

#[test]
fn lightbend_equiv02() {
    let dir = testdata_dir().join("equiv02");
    let json_path = dir.join("original.json");
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("conf") {
            parse_and_compare(&path, &json_path);
        }
    }
}

#[test]
fn lightbend_equiv03() {
    // This test requires .properties support and include resolution
    let dir = testdata_dir().join("equiv03");
    let json_path = dir.join("original.json");
    let conf_path = dir.join("includes.conf");
    parse_and_compare(&conf_path, &json_path);
}

#[test]
fn lightbend_equiv04() {
    let dir = testdata_dir().join("equiv04");
    let json_path = dir.join("original.json");
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("conf") {
            parse_and_compare(&path, &json_path);
        }
    }
}

#[test]
fn lightbend_equiv05() {
    let dir = testdata_dir().join("equiv05");
    let json_path = dir.join("original.json");
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("conf") {
            parse_and_compare(&path, &json_path);
        }
    }
}
