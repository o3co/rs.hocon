// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! rs.hocon#199 — a substitution cycle written twice (`a = ${?b}` /
//! `b = ${?a}` twice) overflowed the stack and aborted the process: the
//! in-progress branch of the resolver re-entered a key's saved prior forever.
//! The interim guard (`PriorReentry` in `substitution_resolver.rs`) treats a
//! re-entry in an unchanged state as an unbreakable cycle; the full fix is the
//! resolver redesign in o3co/xx.hocon#67 / #125. Expected values are
//! Lightbend 1.4.9.
//!
//! Fixtures: `testdata/hocon/optional-cycle-repeat/oc01–oc06`
//! (o3co/xx.hocon#136; the fixture test skips until xx main carries them).

use std::path::PathBuf;

fn json(cfg: &hocon::Config) -> serde_json::Value {
    serde_json::from_str(&hocon::_render_json_for_test(cfg)).expect("valid JSON")
}

fn check(src: &str, want: &str) {
    let cfg = hocon::parse(src).unwrap_or_else(|e| panic!("{src:?}: must parse, got {e}"));
    let want: serde_json::Value = serde_json::from_str(want).unwrap();
    assert_eq!(json(&cfg), want, "{src:?}");
}

#[test]
fn issue199_unbreakable_cycle_through_priors() {
    check("a = ${?b}\nb = ${?a}\na = ${?b}\nb = ${?a}\n", "{}");
    check(
        "a = ${?b}\nb = ${?a}\na = ${?b}\nb = ${?a}\na = ${?b}\nb = ${?a}\n",
        "{}",
    );
    check(
        "a = ${?b}\nb = ${?c}\nc = ${?a}\na = ${?b}\nb = ${?c}\nc = ${?a}\n",
        "{}",
    );
    check(
        "o.a = ${?o.b}\no.b = ${?o.a}\no.a = ${?o.b}\no.b = ${?o.a}\n",
        r#"{"o":{}}"#,
    );
    check(
        "a = ${?b}\nb = ${?a}\na = ${?b}\nb = ${?a}\nc = ${?a}x\n",
        r#"{"c":"x"}"#,
    );
    check(
        "a = ${?b}\nb = ${?a}\na = ${?b}\nb = ${?a}\na = 1\n",
        r#"{"a":1,"b":1}"#,
    );
}

/// Re-entries the guard must let through: the second pass through a prior
/// sees new keys in progress and stops on its own.
#[test]
fn issue199_legal_reentry_unchanged() {
    check(
        "o = { y = ${?a} }\na = ${o.y}\no.x = 5\no.y += [3]\n",
        r#"{"a":[[3]],"o":{"x":5,"y":[[3]]}}"#,
    );
    check(
        "a = 1\na = ${a}x\na = ${a}y\nb = ${a}\n",
        r#"{"a":"1xy","b":"1xy"}"#,
    );
    check("a = ${?b}\nb = ${?a}\na = ${?b}\n", "{}");
    // The second pass through k's prior runs with `o` in the S13a.12
    // prefix-prior guard set, so it stops on its own; these load today and
    // must keep their value (review counter-examples).
    check(
        "o = ${?k}z\no.y = ${?o.y}\nk = ${?o.x}\nk = ${k}z\n",
        r#"{"k":"z","o":{}}"#,
    );
    check(
        "o = ${?k}z\no.y.z = ${?o.y.z}\nk = ${?o.x}\nk = ${k}z\n",
        r#"{"k":"z","o":{"y":{}}}"#,
    );
    check(
        "o = { y = ${?k} }\nk = [${?o.y}]\nk = ${?m}\nm = ${?k}\no = ${?k}\n",
        r#"{"k":[],"m":[],"o":[]}"#,
    );
}

/// The required form of an unbreakable cycle is an error, never an abort.
#[test]
fn issue199_required_cycle_is_an_error() {
    for src in [
        "a = ${b}\nb = ${a}\na = ${b}\nb = ${a}\n",
        "a = ${?b}\nb = ${a}\na = ${?b}\nb = ${a}\n",
    ] {
        assert!(hocon::parse(src).is_err(), "{src:?}: expected an error");
    }
}

/// Shapes that also aborted before and now terminate. The values still differ
/// from Lightbend's (a cycle through concatenation or nested objects); getting
/// them right is the resolver redesign, so only termination is asserted.
#[test]
fn issue199_other_cycle_shapes_terminate() {
    for src in [
        "a = ${?b}x\nb = ${?a}y\na = ${?b}x\nb = ${?a}y\n",
        "c = 4\no.x = 3\no.y = { x = ${?a} }\no.x = ${?o.y}\na = ${?o}\na += 0\n",
    ] {
        let _ = hocon::parse(src);
    }
}

/// A cycle through nested objects (it aborted before the guard) built an
/// exponentially large value and took tens of seconds; the value budget
/// turns it into an error promptly.
#[test]
fn issue199_runaway_nested_cycle_is_an_error() {
    let src = "o.x = 2\no.x = ${?o}\no.y = { z = ${?o.y.z} }\no.y.z = ${?o}\no.x = ${?o.x}\n\
               o.y = { x = ${?o.y} }\no.y = ${?o.y.z}\no.y.q = ${?o.x}\n";
    let err = hocon::parse(src).expect_err("expected the value budget to stop it");
    assert!(
        err.to_string().contains("resolved value too large"),
        "unexpected error: {err}"
    );
}

/// rs.hocon#203 review: a ring of array or string substitutions written twice
/// doubles its value on every lap. The budget is charged as each substitution
/// result is produced, so it stops while the value is being built (a model of
/// the 16-key array ring reaches 17 billion nodes) instead of after the field.
#[test]
fn issue199_doubling_rings_are_an_error() {
    for (name, element) in [("array", "[${?a%}, ${?a%}]"), ("string", "${?a%}${?a%}x")] {
        let n = 16;
        let ring: String = (0..n)
            .map(|i| {
                format!(
                    "a{i} = {}\n",
                    element.replace('%', &((i + 1) % n).to_string())
                )
            })
            .collect();
        let src = format!("{ring}{ring}");
        let err = hocon::parse(&src).expect_err(name);
        assert!(
            err.to_string().contains("resolved value too large"),
            "{name}: unexpected error: {err}"
        );
    }
}

/// rs.hocon#203 review (resource exhaustion after a cut): a cut substitution
/// resolves to nothing, which is never cached, so sibling references
/// re-explore whole subtrees (11 s of CPU for 4 lines, days for longer ones);
/// a dense cycle nests ×2 per key; padding inflated the value budget into
/// gigabytes. The post-cut work budget, nesting cap and value-budget cap turn
/// each into a prompt error. All of these aborted with a stack overflow
/// before the guard.
#[test]
fn issue199_post_cut_resource_bounds() {
    let refs = |key: &str, n: usize| format!("${{?{key}}}").repeat(n);
    let w8 = format!(
        "a = {a}\nb = {b}\na = {a}\nb = {b}\n",
        a = refs("b", 8),
        b = refs("a", 8)
    );
    let keys: Vec<String> = (0..10).map(|i| format!("k{i}")).collect();
    let dense: String = keys
        .iter()
        .map(|k| {
            format!(
                "{k} = {}\n",
                keys.iter()
                    .map(|o| format!("${{?{o}}}"))
                    .collect::<String>()
            )
        })
        .collect();
    let pad = format!("pad = [{}]\n", vec!["1"; 10_000].join(","));
    let ring: String = (0..16)
        .map(|i| format!("a{i} = ${{?a{n}}}${{?a{n}}}x\n", n = (i + 1) % 16))
        .collect();
    let cases = [
        ("wide refs", w8, "too complex"),
        ("dense cycle", format!("{dense}{dense}"), "too complex"),
        (
            "padded string ring",
            format!("{pad}{ring}{ring}"),
            "too large",
        ),
    ];
    // The nesting cap allows 1,000 nested substitutions; a debug build's
    // frames need more than the 2 MiB a test thread gets by default.
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            for (name, src, want) in cases {
                let err = hocon::parse(&src).expect_err(name);
                assert!(
                    err.to_string().contains(want),
                    "{name}: unexpected error: {err}"
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// The value budget only applies after the guard has cut a cycle, so large
/// acyclic configs that load today are never charged: an exponential chain of
/// references and a deeply nested object.
#[test]
fn issue199_value_budget_spares_acyclic_configs() {
    let mut src = String::from("a0 = { v = 1 }\n");
    for i in 1..=12 {
        src += &format!("a{i} = {{ x = ${{a{}}}, y = ${{a{}}} }}\n", i - 1, i - 1);
    }
    hocon::parse(&src).expect("an exponential but acyclic chain must still load");
    let deep = format!("{}x = 1{}\n", "a { ".repeat(120), " }".repeat(120));
    hocon::parse(&deep).expect("a deeply nested object must still load");
}

/// oc01–oc06: must error iff the fixture carries an `.error` sidecar; one with
/// a `-expected.json` sidecar must parse to exactly that value. `-inner.conf`
/// files are include targets, not fixtures.
#[test]
fn issue199_optional_cycle_repeat_fixtures() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata");
    let dir = root.join("hocon/optional-cycle-repeat");
    let expected = root.join("expected/optional-cycle-repeat");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!(
            "skip: {} missing (lands with o3co/xx.hocon#136)",
            dir.display()
        );
        return;
    };
    let mut stems: Vec<String> = entries
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "conf").then(|| p.file_stem()?.to_str().map(str::to_string))?
        })
        .filter(|s| !s.ends_with("-inner"))
        .collect();
    stems.sort();
    assert!(stems.len() >= 6, "expected oc01–oc06 in {}", dir.display());
    for stem in stems {
        let result = hocon::parse_file(dir.join(format!("{stem}.conf")));
        if expected.join(format!("{stem}.error")).exists() {
            assert!(result.is_err(), "{stem}: must be rejected, got success");
            continue;
        }
        let sidecar = expected.join(format!("{stem}-expected.json"));
        let want: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        let cfg = result.unwrap_or_else(|e| panic!("{stem}: must parse, got {e}"));
        assert_eq!(json(&cfg), want, "{stem}");
    }
}
