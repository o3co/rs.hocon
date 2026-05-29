// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! `+=` array-append accumulation across include boundaries. Cross-impl
//! regression for go.hocon#134.
//!
//! `a += b` is spec sugar for `a = ${?a} [b]` (S13b.2, HOCON.md L732).
//! When repeated across included files that are inlined in document order,
//! each append must see the accumulated prior, so
//!
//!   include "first.conf"   # items += "first"
//!   include "second.conf"  # items += "second"
//!   items += "main"
//!
//! yields ["first", "second", "main"]. Pre-fix rs.hocon dropped the first
//! include's element because `+=` was an eager-snapshot AppendPlaceholder whose
//! `existing` was captured in the included file's isolated scope, so the
//! cross-include merge overwrote it. The fix desugars `+=` to the `${?key}
//! [elem]` concat and, in `deep_merge_res_obj_into`, splices the destination's
//! pre-merge value into the included chain's `known_absent` bottom.
//!
//! The explicit-reset case (an included file that assigns `items = []` before
//! its `+=`) must NOT accumulate onto the parent — the assignment breaks the
//! chain per duplicate-key semantics. Reset origin is tracked via
//! `ResObj.reset_keys` rather than mere presence of a src prior, so a
//! *within-file* `+=` chain inside an included file (which also records a src
//! prior) is correctly distinguished from a reset and still accumulates. Both
//! halves are pinned here so a fix for the accumulation bug cannot silently
//! regress reset, and vice versa.

use std::fs;
use tempfile::tempdir;

fn hv_strings(list: &[hocon::HoconValue]) -> Vec<String> {
    list.iter()
        .map(|v| match v {
            hocon::HoconValue::Scalar(s) => s.raw.clone(),
            other => format!("{:?}", other),
        })
        .collect()
}

#[test]
fn s13b2_plus_equals_accumulates_across_includes() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("first.conf"), "items += \"first\"\n").unwrap();
    fs::write(dir.path().join("second.conf"), "items += \"second\"\n").unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src =
        format!("include \"{d}/first.conf\"\ninclude \"{d}/second.conf\"\nitems += \"main\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["first", "second", "main"]);
}

#[test]
fn s13b2_explicit_reset_in_include_breaks_chain() {
    // second.conf assigns items = [] before appending → the assignment
    // overrides the prior (first.conf's "first"), so the result is
    // ["second", "main"], NOT ["first", "second", "main"].
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("first.conf"), "items += \"first\"\n").unwrap();
    fs::write(
        dir.path().join("second.conf"),
        "items = []\nitems += \"second\"\n",
    )
    .unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src =
        format!("include \"{d}/first.conf\"\ninclude \"{d}/second.conf\"\nitems += \"main\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["second", "main"]);
}

#[test]
fn s13b2_within_file_chain_unchanged() {
    let cfg = hocon::parse("items += \"a\"\nitems += \"b\"\nitems += \"c\"\n").expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["a", "b", "c"]);
}

#[test]
fn s13b2_plus_equals_with_prior_array_seed() {
    // A normal assignment seed followed by appends in the same file.
    let cfg = hocon::parse("items = [\"seed\"]\nitems += \"a\"\nitems += \"b\"\n").expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["seed", "a", "b"]);
}

#[test]
fn s13b2_plus_equals_on_non_array_prior_errors() {
    // S13b.2: `+=` on a non-array prior must error (not silently wrap).
    let result = hocon::parse("a = 42\na += 1\n");
    assert!(
        result.is_err(),
        "+= on non-array prior must error, got {:?}",
        result
    );
}

#[test]
fn s13b2_nested_key_plus_equals_accumulates_across_includes() {
    // The desugared `${?srv.items}` self-ref must reference the full nested
    // path, so nested `+=` accumulates across includes too.
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.conf"), "srv { items += \"a\" }\n").unwrap();
    fs::write(dir.path().join("b.conf"), "srv { items += \"b\" }\n").unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src = format!("include \"{d}/a.conf\"\ninclude \"{d}/b.conf\"\nsrv.items += \"main\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("srv.items").expect("srv.items");
    assert_eq!(hv_strings(&items), vec!["a", "b", "main"]);
}

#[test]
fn s13b2_prefix_mounted_include_relativizes_self_ref() {
    // An included file with bare `+=` mounted under a prefix: the desugared
    // `${?items}` must be relativized to `${?mount.items}` so the within-file
    // chain and the parent `+=` all accumulate under the mount point.
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("inner.conf"),
        "items += \"i1\"\nitems += \"i2\"\n",
    )
    .unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src = format!("mount {{ include \"{d}/inner.conf\" }}\nmount.items += \"outer\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("mount.items").expect("mount.items");
    assert_eq!(hv_strings(&items), vec!["i1", "i2", "outer"]);
}

#[test]
fn s13b2_parent_reset_after_include_breaks_chain() {
    // Symmetric to the include-side reset: the PARENT assigns the key after an
    // include's `+=`, then appends. The assignment breaks the chain.
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("c.conf"), "items += \"c\"\n").unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src = format!("include \"{d}/c.conf\"\nitems = [\"reset\"]\nitems += \"after\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["reset", "after"]);
}

// ─── Regression cases the original WIP test matrix did NOT cover ─────────────
// These exercise the exact path where the rejected `src.prior_values.contains_key`
// discriminator was Critical-wrong: a *within-file `+=` chain inside a later
// include* also records a src prior, so that discriminator misread it as a reset
// and dropped the earlier include's contribution. `reset_keys` distinguishes
// them; `fold_known_absent_self_ref` splices dst's value into the chain bottom.

#[test]
fn s13b2_within_file_chain_in_later_include_accumulates() {
    // second.conf has a *within-file* `+=` chain (records a src prior) AND is
    // merged onto a non-empty dst (first.conf already put "first"). The src
    // prior must NOT be misread as a reset: all four elements survive.
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("first.conf"), "items += \"first\"\n").unwrap();
    fs::write(
        dir.path().join("second.conf"),
        "items += \"s1\"\nitems += \"s2\"\n",
    )
    .unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src =
        format!("include \"{d}/first.conf\"\ninclude \"{d}/second.conf\"\nitems += \"main\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["first", "s1", "s2", "main"]);
}

#[test]
fn s13b2_two_multi_write_includes_accumulate() {
    // Both includes carry within-file chains and both merge onto an
    // accumulating dst. Verifies the splice composes across two boundaries.
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("first.conf"),
        "items += \"a1\"\nitems += \"a2\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("second.conf"),
        "items += \"b1\"\nitems += \"b2\"\n",
    )
    .unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src =
        format!("include \"{d}/first.conf\"\ninclude \"{d}/second.conf\"\nitems += \"main\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["a1", "a2", "b1", "b2", "main"]);
}

#[test]
fn s13b2_reset_in_multi_write_later_include_breaks_chain() {
    // A later include that FIRST resets then runs a within-file chain. The
    // reset must still break accumulation even though the include also records
    // a src prior from its internal chain → ["r1", "r2", "main"], dropping
    // first.conf's "first".
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("first.conf"), "items += \"first\"\n").unwrap();
    fs::write(
        dir.path().join("second.conf"),
        "items = [\"r1\"]\nitems += \"r2\"\n",
    )
    .unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src =
        format!("include \"{d}/first.conf\"\ninclude \"{d}/second.conf\"\nitems += \"main\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["r1", "r2", "main"]);
}

#[test]
fn s13b2_deferred_with_fallback_plus_equals_accumulates() {
    // Multi-agent-review regression (cross-impl with ts.hocon): after the `+=`
    // desugar, a fallback's `+=` value is a `${?items} [...]` self-ref concat.
    // merge_unresolved (with_fallback) must fold it self-ref-free before recording
    // it as the receiver's prior — otherwise the receiver's `${?items}` follows a
    // prior that still contains `${?items}`, dropping the fallback's element
    // (`["r"]` instead of `["f","r"]`; ts.hocon stack-overflows on the same input).
    // Fallback fills, receiver appends → ["f", "r"].
    let recv = hocon::parse_string_with_options(
        "items += \"r\"",
        hocon::ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .expect("recv");
    let fb = hocon::parse_string_with_options(
        "items += \"f\"",
        hocon::ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .expect("fb");
    let cfg = recv
        .with_fallback(&fb)
        .resolve(hocon::ResolveOptions::defaults())
        .expect("resolve");
    assert_eq!(
        hv_strings(&cfg.get_list("items").expect("items")),
        vec!["f", "r"]
    );
}

#[test]
fn s13b2_three_level_within_file_chain_in_include() {
    // A 3-deep within-file chain inside the later include exercises the
    // recursive (nested-Concat) arm of fold_known_absent_self_ref.
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("first.conf"), "items += \"first\"\n").unwrap();
    fs::write(
        dir.path().join("second.conf"),
        "items += \"s1\"\nitems += \"s2\"\nitems += \"s3\"\n",
    )
    .unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src = format!("include \"{d}/first.conf\"\ninclude \"{d}/second.conf\"\n");
    let cfg = hocon::parse(&src).expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(hv_strings(&items), vec!["first", "s1", "s2", "s3"]);
}

#[test]
fn s13b2_degenerate_self_ref_element_in_chain() {
    // Pin the degenerate expansion `items += ${?items}` ≡ `items = ${?items}
    // [${?items}]`: after `items += "a"` (→ ["a"]), the optional self-ref
    // *element* resolves to the current array, nesting it. This is the
    // literal-correct expansion; the test guards against a future change
    // silently altering it.
    let cfg = hocon::parse("items += \"a\"\nitems += ${?items}\n").expect("parse");
    let items = cfg.get_list("items").expect("items");
    assert_eq!(items.len(), 2);
    match &items[0] {
        hocon::HoconValue::Scalar(s) => assert_eq!(s.raw, "a"),
        other => panic!("items[0] expected scalar \"a\", got {:?}", other),
    }
    match &items[1] {
        hocon::HoconValue::Array(inner) => assert_eq!(hv_strings(inner), vec!["a"]),
        other => panic!("items[1] expected nested array [\"a\"], got {:?}", other),
    }
}

#[test]
fn s13b2_allow_unresolved_cross_include_plus_equals_defers() {
    // The new resolve_concat structured-branch allow-unresolved deferral:
    // a cross-include `+=` whose prior is still an unresolved mandatory
    // substitution must defer (not error with an S10.13 Scalar/Array type
    // check) under allow_unresolved. `items = ${missing}` (deferred) followed
    // by `items += "x"` (≡ `items = ${?items} ["x"]`) resolves `${?items}` to
    // the unresolved `${missing}` placeholder, so the structured concat
    // `<placeholder> ["x"]` must defer rather than throw.
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("seed.conf"), "items = ${missing}\n").unwrap();
    let d = dir.path().display().to_string().replace('\\', "/");
    let src = format!("include \"{d}/seed.conf\"\nitems += \"x\"\n");
    let c = hocon::parse_string_with_options(
        &src,
        hocon::ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .expect("parse (deferred)");
    let r = c.resolve(
        hocon::ResolveOptions::defaults()
            .with_allow_unresolved(true)
            .with_use_system_environment(false),
    );
    assert!(
        r.is_ok(),
        "allow_unresolved cross-include += must defer, got {:?}",
        r.err()
    );
}
