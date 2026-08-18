// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! E18 — `Config::render_hocon()` (HOCON emitter).
//!
//! Port of go.hocon's `render_hocon_test.go`. The emitter's correctness
//! contract is the round trip: rendering a resolved config and parsing the
//! text back must yield the same value tree, compared via the canonical JSON
//! form (`hocon::_render_json_for_test`), never as text. See xx.hocon
//! docs/extra-spec-conventions.md §E18.
//!
//! The `from_map`-based cases live in the serde-gated module below (`from_map`
//! requires the `serde` feature); the parse-based cases run featureless too.

use hocon::{parse, parse_string_with_options, ParseOptions};

/// An unresolved config has no textual round trip through a value tree, so
/// `render_hocon` must refuse it rather than emit a broken document. The
/// placeholders sit at several depths so the error propagates through the
/// nested object and array paths, not only the top level.
#[test]
fn render_hocon_rejects_unresolved() {
    let cases = [
        ("top-level", "a = 1\nb = ${a}\n"),
        ("nested", "a = 1\nb { c = ${a} }\n"),
        ("in-array", "a = 1\nb = [1, ${a}, 3]\n"),
        ("obj-in-arr", "a = 1\nb = [{ c = ${a} }]\n"),
    ];
    for (name, src) in cases {
        let cfg = parse_string_with_options(
            src,
            ParseOptions::defaults().with_resolve_substitutions(false),
        )
        .unwrap_or_else(|e| panic!("{name}: parse_string_with_options: {e}"));
        let err = cfg
            .render_hocon()
            .expect_err("render_hocon on unresolved config succeeded, want error");
        assert!(
            err.is_not_resolved() && err.message.contains("unrenderable substitution"),
            "{name}: unexpected error message: {}",
            err.message
        );
    }
}

/// A parsed HOCON document (not from `from_map`) also round-trips once
/// resolved.
#[test]
fn render_hocon_from_parsed_document() {
    let src = r#"
a = 1
b { c = "x", d = [1, 2, "three"] }
e = ${a}
"#;
    let cfg = parse(src).expect("parse");
    let text = cfg.render_hocon().expect("render_hocon");
    let reparsed = parse(&text).unwrap_or_else(|e| panic!("re-parse: {e}\n{text}"));
    let before = hocon::_render_json_for_test(&cfg);
    let after = hocon::_render_json_for_test(&reparsed);
    assert_eq!(
        before, after,
        "parsed-doc round trip diverged\n--- emitted ---\n{text}"
    );
}

#[cfg(feature = "serde")]
mod from_map_cases {
    use hocon::{from_map, parse, Config};
    use serde_json::{json, Value};

    fn config_from(name: &str, values: Value) -> Config {
        let Value::Object(map) = values else {
            panic!("{name}: fixture must be a JSON object");
        };
        from_map(map, Some("test")).unwrap_or_else(|e| panic!("{name}: from_map: {e}"))
    }

    /// Render a config to HOCON, parse it back, and compare the re-parsed
    /// config's canonical JSON with the original's. Equal JSON means the
    /// emit → parse round trip preserved the value tree, which is the
    /// emitter's correctness contract.
    fn assert_round_trip(name: &str, values: Value) {
        let cfg = config_from(name, values);
        let before = hocon::_render_json_for_test(&cfg);
        let text = cfg
            .render_hocon()
            .unwrap_or_else(|e| panic!("{name}: render_hocon: {e}"));
        let reparsed = parse(&text).unwrap_or_else(|e| {
            panic!("{name}: parse of emitted HOCON failed: {e}\n--- emitted ---\n{text}")
        });
        let after = hocon::_render_json_for_test(&reparsed);
        assert_eq!(
            before, after,
            "{name}: round trip changed the tree\n--- emitted ---\n{text}"
        );
    }

    #[test]
    fn round_trip_scalars() {
        assert_round_trip(
            "scalars",
            json!({"s": "hello", "n": 8080, "f": 1.5, "b": true, "z": false, "nul": null}),
        );
    }

    #[test]
    fn round_trip_nested_objects() {
        assert_round_trip(
            "nested-objects",
            json!({"db": {"host": "localhost", "port": 5432, "opts": {"ssl": true}}}),
        );
    }

    #[test]
    fn round_trip_arrays() {
        assert_round_trip(
            "arrays",
            json!({
                "tags": ["a", "b", "c"],
                "nums": [1, 2, 3],
                "objs": [{"id": 1}, {"id": 2}],
                "nested": [[1, 2], [3, 4]],
                "empty-a": [],
            }),
        );
    }

    /// Strings that would re-parse as another type MUST stay strings.
    #[test]
    fn round_trip_ambiguous_strings() {
        assert_round_trip(
            "ambiguous-strings",
            json!({
                "looks-num": "8080",
                "looks-float": "1.5",
                "looks-bool": "true",
                "looks-null": "null",
                "norway": "no",
                "neg": "-5",
            }),
        );
    }

    /// Strings needing quoting for their content.
    #[test]
    fn round_trip_special_strings() {
        assert_round_trip(
            "special-strings",
            json!({
                "spaces": "hello world",
                "empty": "",
                "reserved": "a:b=c,d",
                "leading": "  padded  ",
                "url": "https://example.com/a?b=1",
                "substish": "${foo.bar}",
            }),
        );
    }

    #[test]
    fn round_trip_multiline() {
        assert_round_trip(
            "multiline",
            json!({
                "block": "line1\nline2\nline3",
                "crlf": "a\r\nb",
                "tab": "a\tb",
            }),
        );
    }

    /// Keys that cannot be bare.
    #[test]
    fn round_trip_awkward_keys() {
        assert_round_trip(
            "awkward-keys",
            json!({
                "a.b": "dotted key",
                "include": "reserved word must be quoted to round-trip",
                "has space": 1,
                "": "empty key",
                "a=b": true,
                "123": "numeric key ok",
            }),
        );
    }

    #[test]
    fn round_trip_empty_object() {
        assert_round_trip("empty-object", json!({"outer": {"inner": {}}}));
    }

    #[test]
    fn round_trip_unicode() {
        assert_round_trip(
            "unicode",
            json!({"jp": "こんにちは", "emoji": "😀", "mixed": "a😀b"}),
        );
    }

    /// Strings that defeat triple-quoting (embedded `"""`, a trailing `"`)
    /// must fall through to escaped double quotes and still round-trip.
    #[test]
    fn round_trip_quote_heavy() {
        assert_round_trip(
            "quote-heavy",
            json!({
                "embedded-triple": "a\"\"\"b\nc",
                "trailing-quote": "ends\"",
                "lone-quote": "a\"b",
                "multiline-quote": "x\ny\"",
                "backslash": "a\\b\\\\c",
            }),
        );
    }

    /// Empty object as an array element and as a direct value exercise the
    /// value-position empty-object branch (distinct from a nested key).
    #[test]
    fn round_trip_empty_object_positions() {
        assert_round_trip(
            "empty-object-positions",
            json!({
                "in-array": [{}, {"a": 1}],
                "direct": {},
            }),
        );
    }

    /// The emitted text should be idiomatic where it is safe: a plain
    /// identifier value and key stay bare, a number is not quoted.
    #[test]
    fn render_hocon_idiomatic() {
        let cfg = config_from(
            "idiomatic",
            json!({"name": "svc", "port": 8080, "enabled": true}),
        );
        let got = cfg.render_hocon().expect("render_hocon");
        for want in ["name = svc\n", "port = 8080\n", "enabled = true\n"] {
            assert!(
                got.contains(want),
                "emitted HOCON missing {want:?}\n--- got ---\n{got}"
            );
        }
    }
}
