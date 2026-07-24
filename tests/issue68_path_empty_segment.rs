//! Cross-impl fix for xx.hocon#68 — two spec-compliance gaps that both let
//! syntactically-invalid text through the KEY/value lexer while the
//! substitution-path lexer already rejected it.
//!
//! **S11.7 — empty path segments in key position** (HOCON.md L515-519): "If a
//! path element is an empty string, it must always be quoted. That is,
//! `a."".b` is a valid path with three elements, and the middle element is an
//! empty string. But `a..b` is invalid and should generate an error. Following
//! the same rule, a path that starts or ends with a `.` is invalid and should
//! generate an error." Pre-fix `parse_key` split the unquoted key token on `.`
//! and *filtered out* the empty pieces, so `a..b: 3` silently collapsed to
//! `{"a":{"b":3}}` and `.a: 3` to `{"a":3}`. Trailing dots already errored,
//! and every substitution-position form already errored — only the key-side
//! leading/adjacent cases leaked.
//!
//! **S8.1 — backtick in unquoted strings** (HOCON.md L245-247): the forbidden
//! set for unquoted strings is ``$ " { } [ ] : = , + # ` ^ ? ! @ * & \``.
//! Backtick was the one member missing from `is_unquoted_start` /
//! `is_unquoted_continue`, so ``a = `t` `` lexed as an ordinary unquoted
//! string. `(` and `)` are deliberately NOT in the set (xx.hocon#34) and are
//! untouched. Backtick inside a quoted string is ordinary content.
//!
//! Pinning fixtures: `testdata/hocon/path-empty-segment/pe01–pe08` and
//! `testdata/hocon/unquoted-forbidden/uf01–uf04` (run `make testdata`).

use std::path::PathBuf;

// ── S11.7: empty path segments in key position ───────────────────────────────

#[test]
fn issue68_adjacent_dots_in_key_is_rejected() {
    // pe01 — pre-fix this parsed to {"a":{"b":3}}.
    assert!(hocon::parse("a..b: 3").is_err());
}

#[test]
fn issue68_leading_dot_in_key_is_rejected() {
    // pe02 — pre-fix this parsed to {"a":3}.
    assert!(hocon::parse(".a: 3").is_err());
}

#[test]
fn issue68_triple_dots_in_key_is_rejected() {
    // pe04 — pre-fix this parsed to {"a":{"c":4}}.
    assert!(hocon::parse("a...c: 4").is_err());
}

#[test]
fn issue68_adjacent_dots_nested_in_object_is_rejected() {
    // pe05 — the same key path one level down must not escape the check.
    assert!(hocon::parse("o { a..b: 3 }").is_err());
}

#[test]
fn issue68_triple_dots_before_quoted_empty_is_rejected() {
    // pe06 — a legal quoted `""` tail does not excuse the illegal `...` head;
    // pre-fix this parsed to {"a":{"c":{"":4}}}.
    assert!(hocon::parse("a...c.\"\": 4").is_err());
}

#[test]
fn issue68_quoted_empty_segment_still_parses() {
    // pe07 — S11.6: `a."".b` is a valid three-element path whose middle
    // element is the empty string. This must NOT regress.
    // Walked via `get` rather than a dotted getter path: the getter's own path
    // syntax cannot address an empty element, so the assertion goes through
    // the parsed tree directly.
    let cfg = hocon::parse("a.\"\".b: 3").unwrap();
    let a = match cfg.get("a") {
        Some(hocon::HoconValue::Object(m)) => m,
        other => panic!("a must be an object, got {:?}", other),
    };
    let empty = match a.get("") {
        Some(hocon::HoconValue::Object(m)) => m,
        other => panic!(
            "the empty-string element must be an object, got {:?}",
            other
        ),
    };
    match empty.get("b") {
        Some(hocon::HoconValue::Scalar(sv)) => assert_eq!(sv.raw, "3"),
        other => panic!("a.\"\".b must be 3, got {:?}", other),
    }
}

#[test]
fn issue68_trailing_dot_in_key_still_rejected() {
    // pe03 — regression guard: already correct before the fix.
    assert!(hocon::parse("a.: 3").is_err());
}

#[test]
fn issue68_substitution_path_forms_still_rejected() {
    // pe08 + siblings — regression guard: the substitution-path lexer already
    // enforced S11.7 and must keep doing so.
    for input in ["y = ${?a..b}", "y = ${?.a}", "y = ${?a.}", "y = ${a..b}"] {
        assert!(
            hocon::parse(input).is_err(),
            "{} must be rejected (empty segment in substitution path)",
            input
        );
    }
}

#[test]
fn issue68_path_whitespace_forms_are_unaffected() {
    // E13 (xx.hocon#42) guard: whitespace around dots yields real (non-empty)
    // segments and stays legal — the S11.7 check must not swallow these.
    //   `a . b = 1`  → ['a ', ' b']
    //   `a. .b = 1`  → ['a', ' ', 'b']
    let cfg = hocon::parse("a . b = 1").unwrap();
    assert_eq!(cfg.get_config("a ").unwrap().get_i64(" b").unwrap(), 1);

    let cfg = hocon::parse("a. .b = 1").unwrap();
    assert_eq!(
        cfg.get_config("a")
            .unwrap()
            .get_config(" ")
            .unwrap()
            .get_i64("b")
            .unwrap(),
        1
    );
}

#[test]
fn issue68_ordinary_dotted_keys_still_parse() {
    // Plain sanity: the common case must be untouched.
    let cfg = hocon::parse("a.b: 3\nc.d.e: 4").unwrap();
    assert_eq!(cfg.get_i64("a.b").unwrap(), 3);
    assert_eq!(cfg.get_i64("c.d.e").unwrap(), 4);
}

// ── S8.1: backtick is forbidden in unquoted strings ──────────────────────────

#[test]
fn issue68_backtick_in_value_is_rejected() {
    // uf01 — pre-fix this parsed to {"a":"`t`"}.
    assert!(hocon::parse("a = `t`").is_err());
}

#[test]
fn issue68_backtick_in_key_is_rejected() {
    // uf02 — pre-fix this parsed to {"`k`":1}.
    assert!(hocon::parse("`k` = 1").is_err());
}

#[test]
fn issue68_backtick_mid_token_is_rejected() {
    // uf03 — pre-fix this parsed to {"a":"x`y"}; the backtick must terminate
    // the unquoted run and then be reported, not absorbed into it.
    assert!(hocon::parse("a = x`y").is_err());
}

#[test]
fn issue68_backtick_inside_quotes_still_parses() {
    // uf04 — inside quotes a backtick is ordinary content.
    let cfg = hocon::parse("a = \"x`y\"").unwrap();
    assert_eq!(cfg.get_string("a").unwrap(), "x`y");
}

#[test]
fn issue68_parens_in_unquoted_strings_still_allowed() {
    // xx.hocon#34 guard: `(` and `)` are NOT in the S8.1 forbidden set — the
    // backtick fix must not widen the exclusion list.
    let cfg = hocon::parse("a = foo(bar)").unwrap();
    assert_eq!(cfg.get_string("a").unwrap(), "foo(bar)");
}

// ── Fixture parity (xx.hocon shared corpus) ──────────────────────────────────

fn fixture_dir(group: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/testdata/hocon/{}", group))
}

fn expected_dir(group: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/testdata/expected/{}", group))
}

/// Every fixture in `group` must error iff it carries an `.error` sidecar and
/// succeed iff it carries a `-expected.json` sidecar. If the fixtures are
/// missing, panic with guidance to run `make testdata`.
fn assert_fixture_group_parity(group: &str) {
    let dir = fixture_dir(group);
    let expected = expected_dir(group);
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
        let path = dir.join(format!("{}.conf", stem));
        let must_error = expected.join(format!("{}.error", stem)).exists();
        let must_succeed = expected.join(format!("{}-expected.json", stem)).exists();
        assert!(
            must_error ^ must_succeed,
            "{}: expected exactly one sidecar (.error xor -expected.json)",
            stem
        );
        let result = hocon::parse_file(&path);
        if must_error {
            assert!(
                result.is_err(),
                "{}: must be rejected (has .error sidecar), got {:?}",
                stem,
                result.map(|c| c.keys().into_iter().map(str::to_string).collect::<Vec<_>>())
            );
        } else if let Err(e) = result {
            panic!(
                "{}: must parse (has -expected.json sidecar), got {}",
                stem, e
            );
        }
    }
}

#[test]
fn issue68_path_empty_segment_fixtures() {
    assert_fixture_group_parity("path-empty-segment");
}

#[test]
fn issue68_unquoted_forbidden_fixtures() {
    assert_fixture_group_parity("unquoted-forbidden");
}
