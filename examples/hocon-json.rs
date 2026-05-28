//! rs.hocon adapter for the cross-impl differential harness
//! (xx.hocon/generate). Parses+resolves a HOCON file and prints the resolved
//! tree as JSON to stdout. On any parse/resolve error it prints a single-line
//! `{"__error__":{"type":..,"message":..}}` record to stdout and exits 3, so
//! the differential driver can compare error-vs-success behaviour uniformly
//! across go/rs/ts and the Lightbend oracle.
//!
//! Usage: `cargo run --example hocon-json -- <conf-file>`
//!
//! Output is built through `serde_json`, which guarantees valid, fully-escaped
//! JSON for object keys and string scalars (the crate's `_render_json_for_test`
//! renderer does not escape object keys). The differential driver sorts keys
//! during normalization, so key order emitted here is irrelevant.
//!
//! Environment substitutions resolve against the process environment, so the
//! driver controls hermeticity by clearing/setting the subprocess env.

use std::process::exit;

use hocon::{Config, HoconValue, ScalarType};

const EXIT_ERROR: i32 = 3;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: hocon-json <conf-file>");
        exit(2);
    }
    match hocon::parse_file(&args[1]) {
        Ok(cfg) => println!("{}", serde_json::to_string(&config_to_json(&cfg)).unwrap()),
        Err(e) => {
            // Enum variant name (first token of Debug) as a coarse error type;
            // the driver mainly distinguishes success-vs-error.
            let dbg = format!("{e:?}");
            let ty = dbg
                .split(|c| c == '(' || c == '{' || c == ' ')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("HoconError");
            let rec = serde_json::json!({ "__error__": { "type": ty, "message": e.to_string() } });
            println!("{rec}");
            exit(EXIT_ERROR);
        }
    }
}

fn config_to_json(config: &Config) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for key in config.keys() {
        let path = key_to_lookup_path(key);
        if let Some(val) = config.get(&path) {
            m.insert(key.to_string(), hocon_to_json(val));
        }
    }
    serde_json::Value::Object(m)
}

fn hocon_to_json(v: &HoconValue) -> serde_json::Value {
    match v {
        HoconValue::Object(map) => {
            let mut m = serde_json::Map::new();
            for (k, val) in map {
                m.insert(k.clone(), hocon_to_json(val));
            }
            serde_json::Value::Object(m)
        }
        HoconValue::Array(arr) => serde_json::Value::Array(arr.iter().map(hocon_to_json).collect()),
        HoconValue::Scalar(sv) => match sv.value_type {
            ScalarType::Null => serde_json::Value::Null,
            ScalarType::Boolean => serde_json::Value::Bool(sv.raw == "true"),
            ScalarType::Number => {
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
            ScalarType::String => serde_json::Value::String(sv.raw.clone()),
            _ => serde_json::Value::String(sv.raw.clone()),
        },
        // Defensive: never panic in the adapter — a stray variant becomes a
        // string so the success path always emits valid JSON.
        _ => serde_json::Value::String(format!("{v:?}")),
    }
}

/// Build a Config lookup path from a raw top-level key, quoting keys that need
/// it so `config.get` treats them as a single segment. Mirrors the convention
/// in tests/lightbend_test.rs.
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
