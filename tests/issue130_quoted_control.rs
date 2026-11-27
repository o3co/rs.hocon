// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! xx.hocon#130 — S1.2.4: a raw C0 control character (U+0000–U+001F) inside a
//! double-quoted string is a parse error, as in JSON and Lightbend. The lexer
//! rejected only LF; tab, CR, U+0001, U+001F and NUL were accepted in value,
//! key, substitution-path and include position. Triple-quoted (S9.1) and
//! unquoted (S8.8) strings keep controls verbatim, and DEL / C1 are legal.
//!
//! The emitter half (E18 decision 5) is in `render_hocon_test.rs`.
//!
//! Pinning fixtures: `testdata/hocon/quoted-control/qc01–qc12` (run
//! `make testdata`).

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/quoted-control")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/quoted-control")
}

/// qc01–qc12: every fixture must error iff it carries an `.error` sidecar;
/// one with a `-expected.json` sidecar must parse to exactly that value.
#[test]
fn issue130_quoted_control_fixtures() {
    let dir = fixture_dir();
    let expected = expected_dir();
    let mut stems: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} missing ({}); run `make testdata`", dir.display(), e))
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "conf").then(|| p.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    stems.sort();
    assert!(!stems.is_empty(), "no fixtures in {}", dir.display());

    for stem in stems {
        let path = dir.join(format!("{stem}.conf"));
        let error_sidecar = expected.join(format!("{stem}.error"));
        let json_sidecar = expected.join(format!("{stem}-expected.json"));
        let must_error = error_sidecar.exists();
        assert!(
            must_error ^ json_sidecar.exists(),
            "{stem}: expected exactly one sidecar (.error xor -expected.json)"
        );
        let result = hocon::parse_file(&path);
        if must_error {
            assert!(
                result.is_err(),
                "{stem}: must be rejected (has .error sidecar), got success"
            );
            continue;
        }
        let cfg = result.unwrap_or_else(|e| panic!("{stem}: must parse, got {e}"));
        let want: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json_sidecar).unwrap()).unwrap();
        let want = want.as_object().expect("expected JSON is an object");
        for (key, value) in want {
            let want = value.as_str().map(str::to_string);
            let got = match cfg.get(key) {
                Some(hocon::HoconValue::Scalar(sv)) => Some(sv.raw.clone()),
                other => panic!("{stem}: key {key:?} is not a scalar: {other:?}"),
            };
            match want {
                Some(w) => assert_eq!(got.as_deref(), Some(w.as_str()), "{stem}: key {key:?}"),
                None => assert_eq!(
                    got.as_deref(),
                    Some(value.to_string().as_str()),
                    "{stem}: key {key:?}"
                ),
            }
        }
        assert_eq!(cfg.keys().len(), want.len(), "{stem}: key count");
    }
}

/// The whole C0 range in value position, except LF (reported as an
/// unterminated string, pinned elsewhere).
#[test]
fn issue130_every_c0_control_in_quoted_value_is_rejected() {
    for cp in 0u32..0x20 {
        if cp == 0x0A {
            continue;
        }
        let c = char::from_u32(cp).unwrap();
        let src = format!("a = \"x{c}y\"\n");
        let err = hocon::parse(&src)
            .err()
            .unwrap_or_else(|| panic!("U+{cp:04X} in a quoted value: expected a parse error"));
        let want = format!("unescaped control character U+{cp:04X} in quoted string");
        assert!(
            err.to_string().contains(&want),
            "U+{cp:04X}: error {err} does not name the control character"
        );
    }
}

/// The other positions that share the quoted-string reader.
#[test]
fn issue130_raw_control_rejected_in_every_quoted_position() {
    // Mirrors go.hocon's 18 positions. Every case must name the control
    // character, not merely fail: a lenient parser can otherwise fail for an
    // unrelated reason (a misleading key or include error, an undefined
    // substitution), which hides whether the lexer rejected the character.
    let cases = [
        ("key", "\"a\tb\" = 1\n"),
        ("dotted-key-segment", "a.\"b\u{1}\" = 1\n"),
        ("quoted-dotted-key", "\"a\".\"b\tc\" = 1\n"),
        ("space-joined-key", "a \"b\tc\" = 1\n"),
        ("adjacent-key", "a\"b\tc\" = 1\n"),
        ("numeric-tail-key", "123abc.\"x\t= 1\n"),
        ("nested-key", "o { \"a\tb\" = 1 }\n"),
        ("array-element", "a = [\"x\u{0}\"]\n"),
        // The key exists (written with an escape), so only the lexer can fail.
        ("subst-path", "\"x\\t\" = 1\na = ${\"x\t\"}\n"),
        ("optional-subst", "a = ${?\"x\u{1f}\"}\n"),
        ("concat", "a = foo \"x\u{1}\" bar\n"),
        ("append", "a = [1]\na += \"x\ty\"\n"),
        ("include", "include \"x\ty\"\n"),
        ("include-required", "include required(\"x\ty\")\n"),
        ("include-file", "include file(\"x\ty\")\n"),
        (
            "include-package-id",
            "include package(\"p\tq\", \"f.conf\")\n",
        ),
        (
            "include-package-file",
            "include package(\"p\", \"f\td.conf\")\n",
        ),
        ("include-nested", "a { include \"x\ty\" }\n"),
    ];
    for (name, src) in cases {
        let err = hocon::parse(src)
            .err()
            .unwrap_or_else(|| panic!("{name}: parse({src:?}): expected a parse error"));
        assert!(
            err.to_string().contains("unescaped control character U+"),
            "{name}: error {err} does not name the control character"
        );
    }
}

/// The error points at the control character, not at the opening quote.
#[test]
fn issue130_error_points_at_the_control_character() {
    let err = hocon::parse("k = 1\na = \"xy\tz\"\n").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("at 2:8:"),
        "error {msg} does not point at the tab (line 2, col 8)"
    );
}

/// The boundary the rejection must not cross.
#[test]
fn issue130_controls_legal_outside_single_quotes() {
    let cases = [
        (
            "escaped",
            "a = \"x\\u0000\\u0001\\t\\u001fy\"\n",
            "x\u{0}\u{1}\t\u{1f}y",
        ),
        (
            "del-c1",
            "a = \"x\u{7f}\u{85}\u{9f}y\"\n",
            "x\u{7f}\u{85}\u{9f}y",
        ),
        (
            "triple-quoted",
            "a = \"\"\"x\u{0}\u{1}\t\r\u{1f}y\"\"\"\n",
            "x\u{0}\u{1}\t\r\u{1f}y",
        ),
        ("unquoted", "a = x\u{1}y\u{0}z\n", "x\u{1}y\u{0}z"),
    ];
    for (name, src, want) in cases {
        let cfg = hocon::parse(src).unwrap_or_else(|e| panic!("{name}: parse({src:?}): {e}"));
        assert_eq!(cfg.get_string("a").unwrap(), want, "{name}");
    }
}
