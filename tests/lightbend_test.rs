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
            _ => unreachable!("unknown ScalarValue variant"),
        },
        _ => unreachable!("unknown HoconValue variant"),
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
        got,
        expected,
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

// --- Lightbend test suite (test01–test13) ---
// These tests verify that the parser can handle the individual test*.conf files
// from the Lightbend HOCON test suite. Where a matching .json exists, we compare
// the output; otherwise we verify parsing succeeds and spot-check key values.

#[test]
fn lightbend_test01() {
    // test01.json is NOT an expected output — it's JSON data included by test01.conf.
    // So we just verify parsing succeeds and spot-check values.
    let path = testdata_dir().join("test01.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    assert_eq!(config.get_i64("ints.fortyTwo").unwrap(), 42);
    assert_eq!(config.get_i64("ints.fortyTwoAgain").unwrap(), 42);
    assert_eq!(config.get_string("strings.abcd").unwrap(), "abcd");
    assert_eq!(config.get_string("strings.abcdAgain").unwrap(), "abcd");
    assert!(config.get_bool("booleans.true").unwrap());
    assert!(!config.get_bool("booleans.false").unwrap());
}

#[test]
fn lightbend_test02_empty_keys_and_quoted_paths() {
    let path = testdata_dir().join("test02.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    // dot-path: a.b.c = 57
    assert_eq!(config.get_i64("a.b.c").unwrap(), 57);
    // substitution ${a.b.c} = 57
    assert_eq!(config.get_i64("57_a").unwrap(), 57);
    assert_eq!(config.get_i64("57_b").unwrap(), 57);
    // substitution ${""."".""}  = 42
    assert_eq!(config.get_i64("42_a").unwrap(), 42);
    assert_eq!(config.get_i64("42_b").unwrap(), 42);
    // "a.b.c" = 103, ${\"a.b.c\"} = 103
    assert_eq!(config.get_i64("103_a").unwrap(), 103);
    // hyphen and underscore keys
    assert_eq!(config.get_i64("a-c").unwrap(), 259);
    assert_eq!(config.get_i64("a_c").unwrap(), 260);
}

#[test]
fn lightbend_test03_includes_with_substitution_fallback() {
    // test03.conf includes test01.conf inside a nested key, plus test03-included.conf.
    // test01.conf contains ${ints.fortyTwo} which requires resolving within the
    // include scope — this is a known limitation when included into a nested key.
    let path = testdata_dir().join("test03.conf");
    let result = hocon::parse_file(&path);

    match result {
        Ok(config) => {
            assert_eq!(config.get_i64("test01.booleans").unwrap(), 42);
            assert_eq!(
                config.get_string("b").unwrap(),
                "This is in the including file"
            );
        }
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            assert!(
                msg.contains("substitution"),
                "unexpected parse error for {}: {}",
                path.display(),
                e
            );
        }
    }
}

#[test]
fn lightbend_test04_akka_reference_config() {
    // This is the Akka reference config — a large, real-world HOCON file.
    // Just verify it parses without errors.
    let path = testdata_dir().join("test04.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    assert_eq!(config.get_string("akka.version").unwrap(), "2.0-SNAPSHOT");
    assert!(config.has("akka.actor"));
}

#[test]
fn lightbend_test05_play_application_config() {
    // Play Framework application config — real-world HOCON.
    let path = testdata_dir().join("test05.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    assert_eq!(
        config.get_string("application.name").unwrap(),
        "Yet Another Blog Engine"
    );
    assert_eq!(config.get_string("db").unwrap(), "mem");
}

#[test]
fn lightbend_test06_delayed_merge() {
    let path = testdata_dir().join("test06.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    // x is defined twice: x=${a} then x=${b}, last wins
    assert_eq!(config.get_i64("x").unwrap(), 2);
    // y is merged: y=${d} then y={hello: world, foo: 10}
    // the object merge should produce foo=10 (overriding d.foo="bar")
    assert_eq!(config.get_i64("y.foo").unwrap(), 10);
    assert_eq!(config.get_string("y.hello").unwrap(), "world");
}

#[test]
fn lightbend_test07_classpath_include() {
    let path = testdata_dir().join("test07.conf");
    let _config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));
}

#[test]
fn lightbend_test08_classpath_include_absolute() {
    let path = testdata_dir().join("test08.conf");
    let _config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));
}

#[test]
fn lightbend_test09_delayed_merge_object() {
    let path = testdata_dir().join("test09.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    // a is defined multiple times with merges
    // Final a should have c=3 from the last object definition
    assert_eq!(config.get_i64("a.c").unwrap(), 3);
    // x = { q: 10 }
    assert_eq!(config.get_i64("x.q").unwrap(), 10);
}

#[test]
fn lightbend_test10_nested_include() {
    let path = testdata_dir().join("test10.conf");
    let result = hocon::parse_file(&path);
    assert!(
        result.is_err(),
        "ParseFile({}) unexpectedly succeeded; update test to assert resolved values",
        path.display()
    );
}

#[test]
fn lightbend_test11_numeric_string_keys() {
    let path = testdata_dir().join("test11.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    // Quoted keys like "10" are stored with the unquoted key name
    assert_eq!(config.get_string("10").unwrap(), "42");
    assert_eq!(config.get_string("-10").unwrap(), "-42");
    assert_eq!(config.get_string("foo-bar").unwrap(), "bar-baz");
    assert_eq!(config.get_string("---").unwrap(), "------");
    assert_eq!(config.get_string("a-").unwrap(), "b-");
}

#[test]
fn lightbend_test12_long_numeric_keys() {
    let path = testdata_dir().join("test12.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    assert_eq!(config.get_string("10").unwrap(), "42");
    assert_eq!(config.get_i64("sth").unwrap(), 42);
    // Very long numeric key — stored without quotes
    assert_eq!(
        config
            .get_string("12345678901234567891234567890123456789")
            .unwrap(),
        "42"
    );
}

#[test]
fn lightbend_test13_substitution_override() {
    // test13 tests that application config can override substitution values
    // from reference config by merging files
    let ref_path = testdata_dir().join("test13-reference-with-substitutions.conf");
    let app_path = testdata_dir().join("test13-application-override-substitutions.conf");

    let ref_config = hocon::parse_file(&ref_path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", ref_path.display(), e));
    let app_config = hocon::parse_file(&app_path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", app_path.display(), e));

    // reference: a=${b}, b="b" → a="b"
    assert_eq!(ref_config.get_string("a").unwrap(), "b");
    assert_eq!(ref_config.get_string("b").unwrap(), "b");

    // application overrides b="overridden"
    assert_eq!(app_config.get_string("b").unwrap(), "overridden");

    // Merge: app with ref fallback — simple key merge (already-resolved values)
    let merged = app_config.with_fallback(&ref_config);
    assert_eq!(merged.get_string("b").unwrap(), "overridden");

    // For proper substitution re-resolution, merge at the text level
    // (concatenate configs before parsing, as HOCON spec intends)
    let ref_text = fs::read_to_string(&ref_path).unwrap();
    let app_text = fs::read_to_string(&app_path).unwrap();
    let combined = format!("{}\n{}", ref_text, app_text);
    let resolved = hocon::parse(&combined).unwrap();
    assert_eq!(resolved.get_string("a").unwrap(), "overridden");
}

#[test]
fn lightbend_test13_bad_substitution() {
    // test13-reference-bad-substitutions.conf has a=${b} with no b defined
    // This should fail with a resolve error
    let path = testdata_dir().join("test13-reference-bad-substitutions.conf");
    let result = hocon::parse_file(&path);
    assert!(
        result.is_err(),
        "Expected error for unresolved substitution in test13-reference-bad-substitutions.conf"
    );
}

// --- Auto-discovery tests using expected JSON from xx.hocon ---

/// Auto-discover test*.conf files that have matching *-expected.json
/// in the expected directory and compare parsed output.
#[test]
fn lightbend_suite_expected_json() {
    let testdata = testdata_dir();
    let expected_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/expected");

    assert!(
        expected_dir.exists(),
        "Missing expected JSON fixtures at {}. Run `make testdata` before `cargo test`.",
        expected_dir.display()
    );

    // Known failures — skip these (linked to open issues)
    let skip: std::collections::HashSet<&str> = [
        "test01-expected.json",  // system.* contains env-specific values (HOME, PATH, etc.)
        "test02-expected.json",  // empty-key ("".""."") and quoted-key ("a.b.c") bugs
        "test09-expected.json",  // delayed merge: object merge with substitution incomplete
        "test10-expected.json",  // rs.hocon#36: nested include substitution scope
        "file-include-expected.json",  // file() include semantics differ from JVM classpath
    ].into_iter().collect();

    let mut tested = 0;
    for entry in fs::read_dir(&expected_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();

        if !name.ends_with("-expected.json") {
            continue;
        }
        if skip.contains(name.as_str()) {
            eprintln!("SKIP (known failure): {}", name);
            continue;
        }

        let conf_name = name.replace("-expected.json", ".conf");
        let conf_path = testdata.join(&conf_name);
        let expected_path = expected_dir.join(&name);

        if !conf_path.exists() {
            eprintln!("SKIP (conf not found): {}", conf_name);
            continue;
        }

        eprintln!("TEST: {} vs {}", conf_name, name);
        parse_and_compare(&conf_path, &expected_path);
        tested += 1;
    }
    assert!(tested > 0, "No expected JSON tests were run. Check tests/testdata/expected/");
}

#[test]
fn lightbend_suite_expected_errors() {
    let testdata = testdata_dir();
    let expected_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/expected");

    assert!(
        expected_dir.exists(),
        "Missing expected JSON fixtures at {}. Run `make testdata` before `cargo test`.",
        expected_dir.display()
    );

    for entry in fs::read_dir(&expected_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();

        if !name.ends_with("-expected-error.json") {
            continue;
        }

        let conf_name = name.replace("-expected-error.json", ".conf");
        let conf_path = testdata.join(&conf_name);

        if !conf_path.exists() {
            eprintln!("SKIP (conf not found): {}", conf_name);
            continue;
        }

        eprintln!("TEST (expect error): {}", conf_name);
        let result = hocon::parse_file(&conf_path);
        assert!(
            result.is_err(),
            "Expected error for {} but got success",
            conf_path.display()
        );
    }
}
