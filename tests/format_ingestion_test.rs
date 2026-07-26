//! Conformance against the shared format-ingestion fixtures from xx.hocon.
//!
//! These expectations are not oracle-generated — Lightbend has no equivalent of
//! these adapters — so they encode the project's own F-item decisions. Their
//! value is cross-implementation: all four must agree with them, and with each
//! other. See tests/testdata/format-ingestion/manifest.json.
#![cfg(feature = "adapters")]

use std::collections::HashMap;
use std::path::PathBuf;

use hocon::adapters::{env, jsonc, properties, toml, yaml, AdapterError};
use hocon::{Config, HoconValue};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/format-ingestion")
}

fn hocon_to_json(v: &HoconValue) -> serde_json::Value {
    match v {
        HoconValue::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, e)| (k.clone(), hocon_to_json(e)))
                .collect(),
        ),
        HoconValue::Array(items) => {
            serde_json::Value::Array(items.iter().map(hocon_to_json).collect())
        }
        HoconValue::Scalar(sv) => match sv.value_type {
            hocon::ScalarType::Null => serde_json::Value::Null,
            hocon::ScalarType::Boolean => serde_json::Value::Bool(sv.raw == "true"),
            hocon::ScalarType::Number => sv
                .raw
                .parse::<i64>()
                .map(serde_json::Value::from)
                .or_else(|_| sv.raw.parse::<f64>().map(serde_json::Value::from))
                .unwrap_or_else(|_| serde_json::Value::String(sv.raw.clone())),
            _ => serde_json::Value::String(sv.raw.clone()),
        },
        other => panic!("unexpected variant: {other:?}"),
    }
}

fn config_to_json(cfg: &Config) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for key in cfg.keys() {
        let quoted = format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\""));
        if let Some(v) = cfg.get(&quoted) {
            m.insert(key.to_string(), hocon_to_json(v));
        }
    }
    serde_json::Value::Object(m)
}

#[derive(serde::Deserialize)]
struct EnvFixture {
    prefix: String,
    vars: HashMap<String, String>,
}

fn ingest(format: &str, kind: Option<&str>, text: &str, id: &str) -> Result<Config, AdapterError> {
    match format {
        "jsonc" => jsonc::parse(text, Some(id)),
        "properties" => properties::parse(text, Some(id)),
        "toml" => toml::parse(text, Some(id)),
        "yaml" => yaml::parse(text, Some(id)),
        "env" => {
            if kind == Some("dotenv") {
                return env::parse_dotenv(
                    text,
                    env::Options {
                        origin: Some(id.to_string()),
                        ..Default::default()
                    },
                );
            }
            let f: EnvFixture = serde_json::from_str(text).expect("env fixture");
            env::load_from(
                &f.vars,
                env::Options {
                    prefix: f.prefix,
                    origin: Some(id.to_string()),
                },
            )
        }
        other => panic!("unknown format {other}"),
    }
}

#[test]
fn format_ingestion_fixtures() {
    let manifest_path = root().join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("{}: {e} — run `make testdata`", manifest_path.display()));
    let manifest: serde_json::Value = serde_json::from_str(&raw).expect("manifest json");
    let cases = manifest["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "manifest lists no cases");

    for c in cases {
        let id = c["id"].as_str().unwrap();
        let format = c["format"].as_str().unwrap();
        let kind = c["kind"].as_str();
        let note = c["note"].as_str().unwrap_or("");
        let text = std::fs::read_to_string(root().join(c["input"].as_str().unwrap()))
            .unwrap_or_else(|e| panic!("{id}: input: {e}"));

        let result = ingest(format, kind, &text, id);

        if c["expect"] == "error" {
            let err = result
                .err()
                .unwrap_or_else(|| panic!("{id}: succeeded, want error ({note})"));
            if let Some(cites) = c["cites"].as_str() {
                assert!(
                    err.message.contains(cites),
                    "{id}: error {:?} does not mention {cites:?}",
                    err.message
                );
            }
            continue;
        }

        let cfg = result.unwrap_or_else(|e| panic!("{id}: {e} ({note})"));
        let expected_raw =
            std::fs::read_to_string(root().join(c["expected"].as_str().unwrap())).unwrap();
        let expected: serde_json::Value = serde_json::from_str(&expected_raw).unwrap();
        let got = config_to_json(&cfg);
        assert_eq!(
            got, expected,
            "{id} mismatch ({note})\n  got:      {got}\n  expected: {expected}"
        );
    }
}
