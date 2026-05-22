// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! Regression tests for rs.hocon#119 (cross-impl with go.hocon#118 — chained
//! self-referential append) and rs.hocon-equivalent of go.hocon#120
//! (value-interior self-references inside ArrayVal / ObjectVal). Both bug
//! classes are fixed by the same change set (foldSelfRef walker + is_self_ref
//! widening + fold_nested_self_refs pre-pass at structure_builder), so a
//! single regression file covers them.
//!
//! Each test runs in isolation when failing — a stack overflow on rs.hocon's
//! recursive resolver terminates the entire test binary. Use
//! `cargo test --test issue119_120_chained_self_ref <name>` to debug one
//! scenario at a time.

use std::fs;
use tempfile::tempdir;

// ---- helpers ----

fn get_string_array(cfg: &hocon::Config, path: &str) -> Vec<String> {
    cfg.get_list(path)
        .unwrap_or_else(|e| panic!("{path}: {e:?}"))
        .into_iter()
        .map(|hv| match hv {
            hocon::HoconValue::Scalar(s) => s.raw,
            other => panic!("{path}: unexpected element {:?}", other),
        })
        .collect()
}

// ---- #119 — chained self-referential append (cross-impl with go.hocon#118) ----

#[test]
fn issue119_flat_array_chain_3() {
    let src = r#"
branches = ["main"]
branches = ${branches} ["dev"]
branches = ${branches} ["release"]
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(
        get_string_array(&cfg, "branches"),
        vec!["main", "dev", "release"]
    );
}

#[test]
fn issue119_flat_array_chain_4() {
    let src = r#"
a = ["a"]
a = ${a} ["b"]
a = ${a} ["c"]
a = ${a} ["d"]
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(get_string_array(&cfg, "a"), vec!["a", "b", "c", "d"]);
}

#[test]
fn issue119_chained_include() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    fs::write(dir.path().join("common.conf"), r#"branches = ["main"]"#).unwrap();
    fs::write(
        dir.path().join("child.conf"),
        r#"branches = ${branches} ["dev"]"#,
    )
    .unwrap();
    let input = format!(
        r#"include "{}/common.conf"
include "{}/child.conf"
branches = ${{branches}} ["release"]"#,
        dir_str, dir_str
    );
    let cfg = hocon::parse(&input).expect("parse/resolve");
    assert_eq!(
        get_string_array(&cfg, "branches"),
        vec!["main", "dev", "release"]
    );
}

#[test]
fn issue119_object_concat_chain_3() {
    // obj = ${obj} {b=2} chain — concat-pattern with object payload.
    let src = r#"
obj = { a = 1 }
obj = ${obj} { b = 2 }
obj = ${obj} { c = 3 }
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(cfg.get_i64("obj.a").unwrap(), 1);
    assert_eq!(cfg.get_i64("obj.b").unwrap(), 2);
    assert_eq!(cfg.get_i64("obj.c").unwrap(), 3);
}

#[test]
fn issue119_multi_segment_chain() {
    let src = r#"
r.x = ["a"]
r.x = ${r.x} ["b"]
r.x = ${r.x} ["c"]
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(get_string_array(&cfg, "r.x"), vec!["a", "b", "c"]);
}

#[test]
fn issue119_nested_object_scoped_chain() {
    // Nested-object form where the inner field uses an explicit parent-qualified
    // self-reference.
    let src = r#"
r {
  x = ["a"]
  x = ${r.x} ["b"]
  x = ${r.x} ["c"]
}
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(get_string_array(&cfg, "r.x"), vec!["a", "b", "c"]);
}

// ---- #120 equivalent — value-interior self-reference ----

#[test]
fn issue120_array_element_chain() {
    // `a = [${a}, "x"]` repeated — substitution as an array element.
    let src = r#"
a = ["init"]
a = [${a}, "x"]
a = [${a}, "y"]
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    // Expected nesting:
    //   step 1: a = ["init"]
    //   step 2: a = [["init"], "x"]
    //   step 3: a = [[["init"], "x"], "y"]
    let v = cfg.get_list("a").unwrap();
    assert_eq!(v.len(), 2, "top-level length");
    match (&v[0], &v[1]) {
        (hocon::HoconValue::Array(inner), hocon::HoconValue::Scalar(s)) => {
            assert_eq!(s.raw, "y");
            assert_eq!(inner.len(), 2, "inner length");
            match (&inner[0], &inner[1]) {
                (hocon::HoconValue::Array(innermost), hocon::HoconValue::Scalar(s)) => {
                    assert_eq!(s.raw, "x");
                    assert_eq!(innermost.len(), 1);
                    if let hocon::HoconValue::Scalar(s) = &innermost[0] {
                        assert_eq!(s.raw, "init");
                    } else {
                        panic!("a[0][0][0]: expected scalar 'init'");
                    }
                }
                _ => panic!("a[0]: unexpected shape"),
            }
        }
        _ => panic!("a: unexpected top-level shape"),
    }
}

#[test]
fn issue120_object_field_chain_2() {
    // `o = { history = ${o}, v = 2 }` over `o = { v = 1 }`. Chain length 2.
    let src = r#"
o = { v = 1 }
o = { history = ${o}, v = 2 }
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(cfg.get_i64("o.v").unwrap(), 2);
    assert_eq!(cfg.get_i64("o.history.v").unwrap(), 1);
}

#[test]
fn issue120_object_field_chain_3() {
    let src = r#"
o = { v = 1 }
o = { history = ${o}, v = 2 }
o = { history = ${o}, v = 3 }
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(cfg.get_i64("o.v").unwrap(), 3);
    assert_eq!(cfg.get_i64("o.history.v").unwrap(), 2);
    assert_eq!(cfg.get_i64("o.history.history.v").unwrap(), 1);
}

#[test]
fn issue120_object_field_chain_2_retained_key() {
    let src = r#"
o = { a = 1, v = 1 }
o = { history = ${o}, v = 2 }
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(cfg.get_i64("o.a").unwrap(), 1);
    assert_eq!(cfg.get_i64("o.v").unwrap(), 2);
    assert_eq!(cfg.get_i64("o.history.a").unwrap(), 1);
    assert_eq!(cfg.get_i64("o.history.v").unwrap(), 1);
}

#[test]
fn issue120_nested_path_object_merge() {
    let src = r#"
r.s = { v = 1 }
r.s = { history = ${r.s}, v = 2 }
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    assert_eq!(cfg.get_i64("r.s.v").unwrap(), 2);
    assert_eq!(cfg.get_i64("r.s.history.v").unwrap(), 1);
}

#[test]
fn issue120_include_merge_object_form() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    fs::write(
        dir.path().join("inc.conf"),
        r#"o = { history = ${o}, v = 2 }"#,
    )
    .unwrap();
    let input = format!(
        r#"o = {{ v = 1 }}
include "{}/inc.conf""#,
        dir_str
    );
    let cfg = hocon::parse(&input).expect("parse/resolve");
    assert_eq!(cfg.get_i64("o.v").unwrap(), 2);
    assert_eq!(cfg.get_i64("o.history.v").unwrap(), 1);
}

#[test]
fn issue120_mixed_concat_array_chain() {
    // step 2 uses concat-substitution (#119 path); step 3 uses array-element
    // substitution (#120 path).
    let src = r#"
a = ["init"]
a = ${a} ["x"]
a = [${a}, "y"]
"#;
    let cfg = hocon::parse(src).expect("parse/resolve");
    let v = cfg.get_list("a").unwrap();
    assert_eq!(v.len(), 2);
    match (&v[0], &v[1]) {
        (hocon::HoconValue::Array(inner), hocon::HoconValue::Scalar(s)) => {
            assert_eq!(s.raw, "y");
            assert_eq!(inner.len(), 2);
            if let (hocon::HoconValue::Scalar(s0), hocon::HoconValue::Scalar(s1)) =
                (&inner[0], &inner[1])
            {
                assert_eq!(s0.raw, "init");
                assert_eq!(s1.raw, "x");
            } else {
                panic!("inner shape mismatch");
            }
        }
        _ => panic!("top-level shape mismatch"),
    }
}
