//! S3.5 — an array-root document (`[1,2]`) is syntactically valid HOCON
//! (HOCON.md L989-991: "both JSON and HOCON allow arrays as root values in a
//! document"), but the object-rooted Config API rejects it with a TYPE error
//! after a successful syntax parse. Reference: Lightbend parses the document,
//! then `Parseable.forceParsedToObject` throws `ConfigException.WrongType`
//! "has type LIST rather than object at file root". The former behavior — a
//! `ParseError` "expected key, got LBracket" — was the right net outcome
//! (reject) as the wrong kind of error at the wrong layer.
//!
//! S14b.1 (HOCON.md L993-994): an INCLUDED file with an array root is
//! invalid; the error names the *innermost* included source.
//!
//! Fixtures: xx.hocon array-root/ar01-ar03 with `.error` sidecars (see the
//! conformance loop at the bottom).

use hocon::HoconError;

/// Assert the error is the Config (type-mismatch) variant naming the
/// array-at-file-root condition, and NOT a Parse (syntax) error.
fn assert_array_root_config_error(err: HoconError, label: &str) -> String {
    match err {
        HoconError::Config(ce) => {
            assert!(
                ce.message.contains("array rather than object at file root"),
                "{}: message must name the array-at-file-root condition, got: {}",
                label,
                ce.message
            );
            assert!(
                ce.path.is_empty(),
                "{}: file-root errors carry no access path, got: {}",
                label,
                ce.path
            );
            ce.message
        }
        HoconError::Parse(pe) => panic!(
            "{}: got ParseError {:?}; array-root documents are valid syntax — expected HoconError::Config",
            label, pe.message
        ),
        other => panic!("{}: expected HoconError::Config, got {:?}", label, other),
    }
}

#[test]
fn s3_5_top_level_is_type_error() {
    for (label, src) in [
        ("basic", "[1,2]"),
        ("multiline-objects", "[\n  { a : 1 },\n  { b : 2 }\n]\n"),
        ("empty-array", "[]"),
    ] {
        let err = hocon::parse(src).expect_err(label);
        assert_array_root_config_error(err, label);
    }
}

#[test]
fn s3_5_error_carries_position() {
    let err = hocon::parse("\n  [1,2]").expect_err("position");
    let msg = assert_array_root_config_error(err, "position");
    assert!(
        msg.contains("2:3"),
        "message must carry the opening bracket position 2:3, got: {}",
        msg
    );
}

#[test]
fn s3_5_deferred_lifecycle() {
    let opts = hocon::ParseOptions::defaults().with_resolve_substitutions(false);
    let err = hocon::parse_string_with_options("[1,2]", opts).expect_err("deferred");
    assert_array_root_config_error(err, "deferred");
}

#[test]
fn s3_5_malformed_arrays_stay_syntax_errors() {
    for (label, src) in [
        ("unterminated", "[1,2"),
        ("trailing-content", "[1,2]\na = 1"),
    ] {
        match hocon::parse(src) {
            Err(HoconError::Parse(_)) => {}
            other => panic!(
                "{}: expected HoconError::Parse (syntax), got {:?}",
                label, other
            ),
        }
    }
}

#[test]
fn s3_5_include_of_array_root_names_included_file() {
    let dir = tempfile::tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("arr.conf"), "[1,2]\n").unwrap();
    let input = format!("include \"{}/arr.conf\"\na = 1\n", dir_str);
    match hocon::parse(&input) {
        Err(HoconError::Resolve(re)) => {
            assert!(
                re.message.contains("array at file root"),
                "message must name the array-at-file-root condition, got: {}",
                re.message
            );
            assert!(
                re.message.contains("arr.conf") || re.path.contains("arr.conf"),
                "error must name the included file, got: {:?}",
                re
            );
        }
        other => panic!("expected HoconError::Resolve, got {:?}", other),
    }
}

#[test]
fn s3_5_nested_include_names_innermost_file() {
    // parent -> mid -> arr: the error must accuse arr.conf (the file that
    // actually has the array root), never the intermediate mid.conf.
    let dir = tempfile::tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("arr.conf"), "[1,2]\n").unwrap();
    std::fs::write(dir.path().join("mid.conf"), "include \"arr.conf\"\nb = 2\n").unwrap();
    let input = format!("include \"{}/mid.conf\"\na = 1\n", dir_str);
    match hocon::parse(&input) {
        Err(HoconError::Resolve(re)) => {
            assert!(
                re.message.contains("array at file root"),
                "message must name the array-at-file-root condition, got: {}",
                re.message
            );
            let all = format!("{} {}", re.message, re.path);
            assert!(
                all.contains("arr.conf"),
                "error must name the innermost file arr.conf, got: {:?}",
                re
            );
            assert!(
                !re.message.contains("mid.conf"),
                "error must not accuse the intermediate file mid.conf, got: {}",
                re.message
            );
        }
        other => panic!("expected HoconError::Resolve, got {:?}", other),
    }
}

#[cfg(feature = "include-package")]
#[test]
fn s3_5_package_include_of_array_root_is_error() {
    let input = r#"
        a = 1
        include package("test/s3-5-array-root", "ref.conf")
    "#;
    match hocon::Parser::new()
        .register_package("test/s3-5-array-root", "ref.conf", "[1,2]\n")
        .parse(input)
    {
        Err(HoconError::Resolve(re)) => {
            assert!(
                re.message.contains("array at file root"),
                "message must name the array-at-file-root condition, got: {}",
                re.message
            );
        }
        other => panic!("expected HoconError::Resolve, got {:?}", other),
    }
}

#[test]
fn s3_5_non_root_arrays_unaffected() {
    let cfg = hocon::parse("a = [1,2]").expect("field-value array");
    assert_eq!(cfg.get_i64("a.0").unwrap_or(1), 1);
    assert!(
        hocon::parse("{ a = [1,2] }").is_ok(),
        "braced root with array field"
    );
}

/// Conformance loop over the xx.hocon array-root fixtures (ar01-ar03,
/// `.error` sidecars — Lightbend WrongType ground truth). ar03-inner.conf is
/// a sibling include-target, not a standalone fixture.
#[test]
fn s3_5_conformance_fixture_loop() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/hocon/array-root");
    if !dir.is_dir() {
        eprintln!("skip: array-root fixtures missing; run `make testdata` (requires xx.hocon#64)");
        return;
    }
    let env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut ran = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".conf") || name.ends_with("-inner.conf") {
            continue;
        }
        let err = hocon::parse_file_with_env(&path, &env)
            .err()
            .unwrap_or_else(|| panic!("{}: expected array-at-file-root error, got Ok", name));
        let msg = format!("{}", err);
        assert!(
            msg.contains("array") && msg.contains("file root"),
            "{}: error must name the array-at-file-root condition, got: {}",
            name,
            msg
        );
        ran += 1;
    }
    assert!(
        ran >= 3,
        "expected at least 3 array-root fixtures, ran {}",
        ran
    );
}
