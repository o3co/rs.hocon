use crate::lexer::Segment;
use crate::value::HoconValue;
use indexmap::IndexMap;

use super::types::{ResObj, ResolverValue};

/// Compare two segment slices by `text` only, ignoring source positions.
/// Used for self-reference detection in substitution resolution and for
/// path equality where positions are immaterial.
pub(crate) fn segments_text_equal(a: &[Segment], b: &[Segment]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.text == y.text)
}

pub(crate) fn lookup_path<'a>(root: &'a ResObj, segments: &[Segment]) -> Option<&'a ResolverValue> {
    if segments.is_empty() {
        return None;
    }
    let head = &segments[0].text;
    let tail = &segments[1..];
    let val = root.fields.get(head.as_str())?;
    if tail.is_empty() {
        return Some(val);
    }
    if let ResolverValue::Obj(inner) = val {
        return lookup_path(inner, tail);
    }
    None
}

pub(crate) fn deep_merge_hocon_objects(
    base: IndexMap<String, HoconValue>,
    overlay: IndexMap<String, HoconValue>,
) -> HoconValue {
    let mut merged = base;
    for (k, v) in overlay {
        // Both sides being objects is the only case that requires deep merge;
        // everything else is an overlay-wins insert (IndexMap preserves the
        // existing key position).
        //
        // Pre-fix (issue #23) used `(merged.get(&k).cloned(), &v)` which
        // deep-cloned the existing subtree AND `new_fields.clone()` on every
        // recursive call — O(N²) work for an N-deep nested merge. Peek by
        // reference, then take ownership via `mem::take` to drop both clones.
        let both_objects = matches!(merged.get(&k), Some(HoconValue::Object(_)))
            && matches!(&v, HoconValue::Object(_));
        if both_objects {
            // Take the existing inner IndexMap without cloning. The slot at
            // `k` temporarily holds Object(empty IndexMap); the insert below
            // overwrites it at the same position.
            let existing_fields = match merged.get_mut(&k).expect("just checked Some via matches!")
            {
                HoconValue::Object(f) => std::mem::take(f),
                _ => unreachable!("just matched HoconValue::Object via matches!"),
            };
            let new_fields = match v {
                HoconValue::Object(f) => f,
                _ => unreachable!("just matched HoconValue::Object via matches!"),
            };
            merged.insert(k, deep_merge_hocon_objects(existing_fields, new_fields));
        } else {
            merged.insert(k, v);
        }
    }
    HoconValue::Object(merged)
}

/// Convert a Resolved(Object) into a ResObj so we can deep-merge it.
fn resolved_obj_to_res_obj(fields: &IndexMap<String, HoconValue>) -> ResObj {
    let mut obj = ResObj::new();
    for (k, v) in fields {
        obj.fields
            .insert(k.clone(), ResolverValue::Resolved(v.clone()));
    }
    obj
}

/// Extract the inner ResObj if the value is an object-like ResolverValue.
/// Returns Some(ResObj) for Obj or Resolved(Object), None otherwise.
fn as_res_obj(val: &ResolverValue) -> Option<ResObj> {
    match val {
        ResolverValue::Obj(o) => Some(o.clone()),
        ResolverValue::Resolved(HoconValue::Object(fields)) => {
            Some(resolved_obj_to_res_obj(fields))
        }
        _ => None,
    }
}

pub(crate) fn deep_merge_res_obj_into(dst: &mut ResObj, src: ResObj, path_prefix: &[String]) {
    let ResObj {
        fields: src_fields,
        prior_values: src_priors,
        reset_keys: src_reset_keys,
    } = src;
    for (k, src_val) in src_fields {
        let dst_is_obj = dst.fields.get(&k).and_then(as_res_obj);
        let src_obj = as_res_obj(&src_val);

        // Full dotted path of this field (`path_prefix + k`). The fold uses
        // this so a self-reference `${full_key}` (e.g. `${r.x}` while merging
        // inside an `r` object) is correctly detected — the pre-fix code used
        // bare leaf `k` and missed full-key self-refs, causing chain-length-4
        // multi-segment patterns (`r.x = ${r.x} [...]` × 4) to overflow the
        // stack at resolve time. Cross-impl follow-up after rs.hocon#119
        // post-Codex review (go.hocon was unaffected because its setPath
        // writes priorValues keyed by full dotted path directly).
        let mut child_prefix = path_prefix.to_vec();
        child_prefix.push(k.clone());
        let full_key = string_segments_to_key(child_prefix.iter().map(String::as_str));

        if let (Some(mut dst_obj), Some(src_obj)) = (dst_is_obj, src_obj) {
            // #120 cross-impl: save dst's pre-merge value as the prior at the
            // OUTER level even when both sides are objects and we recurse.
            // Otherwise a `${k}` in the merged result (e.g. `o = { history =
            // ${o}, v = 2 }` included into a parent `o = { v = 1 }`) has no
            // lookback target — resolve_subst hits the "no prior" error.
            if let Some(old) = dst.fields.get(&k) {
                let prior_existing = dst.prior_values.get(&k).cloned();
                if let Some(prior) = super::fold_self_ref::fold_or_skip_prior(
                    old,
                    &full_key,
                    prior_existing.as_ref(),
                ) {
                    // fold_or_skip_prior only matches Subst nodes whose path
                    // equals `full_key` (the outer key). It does not detect
                    // self-refs nested at child paths inside `prior`, so
                    // without a recursive fold the saved prior breaks the
                    // "every saved prior is self-ref-free" invariant from
                    // fold_self_ref.rs. A later cycle-recovery descent into
                    // the prior would then trip contains_subst_by_path on a
                    // self-ref the leaf-level path already has a folded
                    // value for.
                    //
                    // Unlike structure_builder.rs's sr13 site, this fold needs
                    // no `should_fold_nested` guard. The two folds here target
                    // disjoint key sets: fold_or_skip_prior folds only the
                    // outer-key self-ref `${full_key}`, while fold_nested_self_refs
                    // folds only deeper nested-path self-refs against each
                    // sub-object's own prior_values — no node is folded twice.
                    // fold_nested_self_refs is also self-guarded by
                    // contains_self_ref per leaf, so applying it to an
                    // already-self-ref-free prior (e.g. on a subsequent merge
                    // into the same key) is a no-op. That idempotence is what
                    // sr13's guard exists to enforce structurally there; here it
                    // holds for free, so the unconditional fold is safe.
                    let prior_self_ref_free =
                        super::fold_self_ref::fold_nested_self_refs(&prior, &child_prefix);
                    dst.prior_values.insert(k.clone(), prior_self_ref_free);
                }
            }
            deep_merge_res_obj_into(&mut dst_obj, src_obj, &child_prefix);
            dst.fields.insert(k, ResolverValue::Obj(dst_obj));
            continue;
        }

        // Non-object collision: distinguish how src's value for `k` composes
        // with dst's pre-merge value (go.hocon#134, S13b.2 `+=` accumulation
        // across includes). Three cases:
        //   (1) src reset `k` (explicit non-self-ref `k = [...]`) → src replaces
        //       dst; drop dst's stale prior, let src's prior carry over.
        //   (2a) src is a within-file `+=` chain (has its own prior for `k`) →
        //       splice dst's value into the chain's `known_absent` bottom so the
        //       included chain accumulates onto dst across the include boundary.
        //   (2b) src is a bare `+=` (no in-file prior) → dst's value becomes the
        //       prior that src's field-level `${?k}` chains off.
        // Cases 2a/2b keep the same fold-or-skip / self-ref-free-prior discipline
        // (#118/#120 chain-class invariant) as the both-objects branch above.
        if dst.fields.contains_key(&k) {
            if src_reset_keys.contains(&k) {
                dst.prior_values.shift_remove(&k);
            } else {
                let dst_old = dst.fields.get(&k).cloned().unwrap();
                let dst_prior = dst.prior_values.get(&k).cloned();
                if let Some(dst_folded) = super::fold_self_ref::fold_or_skip_prior(
                    &dst_old,
                    &full_key,
                    dst_prior.as_ref(),
                ) {
                    if let Some(src_prior) = src_priors.get(&k) {
                        dst.prior_values.insert(
                            k.clone(),
                            super::fold_self_ref::fold_known_absent_self_ref(
                                src_prior,
                                &full_key,
                                &dst_folded,
                            ),
                        );
                    } else {
                        dst.prior_values.insert(k.clone(), dst_folded);
                    }
                }
                // dst_folded == None only when dst's value is a *required*
                // self-ref with no prior — unreachable through `+=` (which
                // desugars to the *optional* `${?key}`). If it were ever hit,
                // doing nothing here is the safe default: src's own prior
                // carries over via the loop below and the chain is unaffected.
            }
        }
        dst.fields.insert(k, src_val);
    }
    // Carry over prior_values from src that aren't already set in dst.
    // This preserves delayed-merge chains from included files (and, for a reset
    // key whose dst prior was dropped above, installs src's own prior).
    for (k, src_prior) in src_priors {
        if !dst.prior_values.contains_key(&k) {
            dst.prior_values.insert(k, src_prior);
        }
    }
    // go.hocon#134: propagate reset origin so a future merge that treats this
    // object as an included source composes correctly. If src reset `k`, the
    // merged value traces back to that reset; if dst had reset `k` and src
    // chained off it, the merged value still traces to dst's reset — the union
    // is the correct reset set either way.
    dst.reset_keys.extend(src_reset_keys);
}

/// Relativize all substitution paths in a ResolverValue tree by prepending the given prefix.
/// Called when including a file into a nested scope so `${y}` becomes `${prefix.y}`.
/// Prefix strings are wrapped into `Segment` values using the substitution's own line/col.
pub(crate) fn relativize_subst_paths(val: &mut ResolverValue, prefix_segments: &[String]) {
    match val {
        ResolverValue::Subst(s) => {
            let prefix: Vec<Segment> = prefix_segments
                .iter()
                .map(|text| Segment {
                    text: text.clone(),
                    line: s.line,
                    col: s.col,
                })
                .collect();
            let mut new_segments = Vec::with_capacity(prefix.len() + s.segments.len());
            new_segments.extend_from_slice(&prefix);
            new_segments.extend_from_slice(&s.segments);
            s.segments = new_segments;
            s.prefix_len += prefix_segments.len();
        }
        ResolverValue::Concat(c) => {
            for node in &mut c.nodes {
                relativize_subst_paths(node, prefix_segments);
            }
        }
        ResolverValue::Obj(o) => {
            relativize_res_obj(o, prefix_segments);
        }
        ResolverValue::UnresolvedArray(items) => {
            for item in items {
                relativize_subst_paths(item, prefix_segments);
            }
        }
        ResolverValue::Resolved(_) => {}
    }
}

pub(crate) fn relativize_res_obj(obj: &mut ResObj, prefix_segments: &[String]) {
    for val in obj.fields.values_mut() {
        relativize_subst_paths(val, prefix_segments);
    }
    for val in obj.prior_values.values_mut() {
        relativize_subst_paths(val, prefix_segments);
    }
}

pub(crate) fn segments_to_key(segments: &[Segment]) -> String {
    string_segments_to_key(segments.iter().map(|s| s.text.as_str()))
}

/// String-segment variant of [`segments_to_key`] for callers that have
/// path components as `String` (e.g. structure builder's `path_prefix +
/// head`). The escape / quoting rules are kept identical to the
/// Segment-based version so a fold-time key computed from string
/// segments compares equal to a resolver-time key computed from
/// substitution placeholder segments.
pub(crate) fn string_segments_to_key<'a, I>(segments: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    segments
        .into_iter()
        .map(|s| {
            // Same quoting rule as segments_to_key. Documented there.
            if s.is_empty()
                || s.contains('.')
                || s.contains('"')
                || s.contains('\\')
                || s != s.trim()
                || s.contains(' ')
                || s.contains('\t')
                || s.contains('[')
                || s.contains(']')
            {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::super::fold_self_ref;
    use super::super::types::{ConcatPlaceholder, SubstPlaceholder};
    use super::*;
    use std::collections::HashSet;
    use ResObj as ResObjTy;

    fn seg(text: &str) -> Segment {
        Segment {
            text: text.to_string(),
            line: 1,
            col: 1,
        }
    }

    // ── issue #135 / PR #136 defect 1 ─────────────────────────────────────
    //
    // Invariant under test (fold_self_ref.rs module comment): *every saved
    // prior is self-ref-free*. The Obj-Obj branch of `deep_merge_res_obj_into`
    // saves dst's pre-merge value as the outer prior via `fold_or_skip_prior`,
    // which only detects a `Subst` whose path EQUALS the outer key. A self-ref
    // nested at a deeper child path (`a.b.child.f1` under outer key `a`) slips
    // through unfolded unless `fold_nested_self_refs` runs. This is a white-box
    // pin of the invariant: the end-to-end false-positive needs an additional
    // in-resolution cycle to *consult* the broken prior (hard to minimise in
    // rs.hocon — see issue #135 — though go.hocon#147 reproduces the same code
    // gap end-to-end). On develop pre-#136 this FAILS; with the fix it PASSES.

    fn subst(path: &[&str]) -> ResolverValue {
        ResolverValue::Subst(SubstPlaceholder {
            segments: path.iter().map(|s| seg(s)).collect(),
            optional: false,
            known_absent: false,
            list_suffix: false,
            line: 1,
            col: 1,
            prefix_len: 0,
        })
    }

    fn obj_with(
        fields: Vec<(&str, ResolverValue)>,
        priors: Vec<(&str, ResolverValue)>,
        resets: &[&str],
    ) -> ResObjTy {
        let mut f = IndexMap::new();
        for (k, v) in fields {
            f.insert(k.to_string(), v);
        }
        let mut p = IndexMap::new();
        for (k, v) in priors {
            p.insert(k.to_string(), v);
        }
        ResObjTy {
            fields: f,
            prior_values: p,
            reset_keys: resets.iter().map(|s| s.to_string()).collect::<HashSet<_>>(),
        }
    }

    #[test]
    fn issue135_obj_obj_merge_saves_self_ref_free_outer_prior() {
        // dst = file-1 accumulated tree, rooted at top-level key `a`:
        //   a.b.child.f1 = ${a.b.parent.f1} ${a.b.child.f1}
        // with a leaf prior for f1 (the value the chain folds against — e.g.
        // an `include` two lines above).
        let f1_chain = ResolverValue::Concat(ConcatPlaceholder {
            nodes: vec![
                subst(&["a", "b", "parent", "f1"]),
                subst(&["a", "b", "child", "f1"]),
            ],
            separator_flags: vec![false, false],
            line: 1,
            col: 1,
        });
        let leaf_prior = ResolverValue::Resolved({
            let mut m = IndexMap::new();
            m.insert(
                "k".to_string(),
                HoconValue::Scalar(crate::value::ScalarValue::number("1".to_string())),
            );
            HoconValue::Object(m)
        });
        let child = obj_with(vec![("f1", f1_chain)], vec![("f1", leaf_prior)], &["f1"]);
        let b = obj_with(
            vec![("child", ResolverValue::Obj(child))],
            vec![],
            &["child"],
        );
        let a = obj_with(vec![("b", ResolverValue::Obj(b))], vec![], &["b"]);
        let mut dst = obj_with(vec![("a", ResolverValue::Obj(a))], vec![], &["a"]);

        // src = file-2 layered on top: also an object at `a` (so the Obj-Obj
        // branch fires and dst's pre-merge `a` is captured as the outer prior).
        let outer = obj_with(
            vec![(
                "values",
                ResolverValue::Resolved(HoconValue::Scalar(crate::value::ScalarValue::string(
                    "x".to_string(),
                ))),
            )],
            vec![],
            &["values"],
        );
        let inner_b = obj_with(
            vec![("outer", ResolverValue::Obj(outer))],
            vec![],
            &["outer"],
        );
        let src_a = obj_with(vec![("b", ResolverValue::Obj(inner_b))], vec![], &["b"]);
        let src = obj_with(vec![("a", ResolverValue::Obj(src_a))], vec![], &["a"]);

        deep_merge_res_obj_into(&mut dst, src, &[]);

        // The outer prior saved at `a` must be self-ref-free at every depth.
        let saved = dst
            .prior_values
            .get("a")
            .expect("outer prior for `a` should be saved during Obj-Obj merge");
        let saved_obj = match saved {
            ResolverValue::Obj(o) => o,
            other => panic!("expected Obj prior, got {other:?}"),
        };
        let descended = lookup_path(saved_obj, &[seg("b"), seg("child"), seg("f1")])
            .expect("a.b.child.f1 should exist in the saved prior");
        assert!(
            !fold_self_ref::contains_subst_by_path(
                descended,
                &[seg("a"), seg("b"), seg("child"), seg("f1")]
            ),
            "saved outer prior for `a` still contains an unfolded self-ref \
             ${{a.b.child.f1}} — breaks the self-ref-free-prior invariant \
             (issue #135 defect 1). descended value: {descended:?}"
        );
    }

    #[test]
    fn segments_to_key_simple() {
        assert_eq!(segments_to_key(&[seg("a"), seg("b"), seg("c")]), "a.b.c");
    }

    #[test]
    fn segments_to_key_quoted_dot() {
        assert_eq!(segments_to_key(&[seg("a.b"), seg("c")]), r#""a.b".c"#);
    }

    #[test]
    fn segments_to_key_empty_string() {
        assert_eq!(segments_to_key(&[seg(""), seg("foo")]), r#""".foo"#);
    }

    #[test]
    fn segments_to_key_escaped_quotes() {
        assert_eq!(segments_to_key(&[seg("a\"b"), seg("c")]), r#""a\"b".c"#);
    }

    #[test]
    fn segments_to_key_escaped_backslash() {
        assert_eq!(segments_to_key(&[seg("a\\b"), seg("c")]), r#""a\\b".c"#);
    }

    #[test]
    fn segments_to_key_quotes_whitespace() {
        assert_eq!(segments_to_key(&[seg(" a "), seg("b")]), r#"" a ".b"#);
    }

    // Issue #23 regression — deep_merge_hocon_objects refactored from
    // double-clone-per-level to peek-and-take. These tests pin the
    // observable contract: overlay wins on scalars/arrays, deep merge on
    // nested objects, and IndexMap key position preserved when overlay
    // updates an existing key.
    fn scalar(s: &str) -> HoconValue {
        HoconValue::Scalar(crate::value::ScalarValue::string(s.to_string()))
    }

    fn obj(pairs: &[(&str, HoconValue)]) -> HoconValue {
        let mut m = IndexMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        HoconValue::Object(m)
    }

    fn as_obj(v: HoconValue) -> IndexMap<String, HoconValue> {
        if let HoconValue::Object(m) = v {
            m
        } else {
            panic!("expected Object, got {:?}", v)
        }
    }

    #[test]
    fn deep_merge_overlay_wins_on_scalar() {
        let base = as_obj(obj(&[("a", scalar("base"))]));
        let overlay = as_obj(obj(&[("a", scalar("overlay"))]));
        let merged = as_obj(deep_merge_hocon_objects(base, overlay));
        assert_eq!(merged.get("a"), Some(&scalar("overlay")));
    }

    #[test]
    fn deep_merge_recurses_when_both_sides_are_objects() {
        let base = as_obj(obj(&[(
            "a",
            obj(&[("x", scalar("from-base")), ("y", scalar("base-only"))]),
        )]));
        let overlay = as_obj(obj(&[(
            "a",
            obj(&[("x", scalar("from-overlay")), ("z", scalar("overlay-only"))]),
        )]));
        let merged = as_obj(deep_merge_hocon_objects(base, overlay));
        let a = as_obj(merged.get("a").unwrap().clone());
        // Overlay wins on overlapping leaf, both-side-only leaves preserved.
        assert_eq!(a.get("x"), Some(&scalar("from-overlay")));
        assert_eq!(a.get("y"), Some(&scalar("base-only")));
        assert_eq!(a.get("z"), Some(&scalar("overlay-only")));
    }

    #[test]
    fn deep_merge_preserves_key_position_for_existing_keys() {
        // After overlay update of "a", "a" stays at position 0 — IndexMap
        // insert on existing key preserves its position. The refactor uses
        // mem::take + re-insert, which must keep the same position.
        let base = as_obj(obj(&[
            ("a", obj(&[("x", scalar("1"))])),
            ("b", scalar("2")),
        ]));
        let overlay = as_obj(obj(&[("a", obj(&[("y", scalar("3"))]))]));
        let merged = as_obj(deep_merge_hocon_objects(base, overlay));
        let keys: Vec<&String> = merged.keys().collect();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn deep_merge_nonobject_then_object_overlays() {
        // base "a" is a scalar; overlay "a" is an object → overlay wins
        // (no deep merge, since base is not an object).
        let base = as_obj(obj(&[("a", scalar("scalar-base"))]));
        let overlay = as_obj(obj(&[("a", obj(&[("nested", scalar("v"))]))]));
        let merged = as_obj(deep_merge_hocon_objects(base, overlay));
        let a = as_obj(merged.get("a").unwrap().clone());
        assert_eq!(a.get("nested"), Some(&scalar("v")));
    }

    #[test]
    fn deep_merge_empty_overlay_is_noop() {
        // Pinning the obvious-by-inspection edge case: empty overlay
        // leaves base untouched, empty base accepts overlay as-is. Cheap
        // contract guard for the refactored loop.
        let base = as_obj(obj(&[("a", scalar("x"))]));
        let overlay: IndexMap<String, HoconValue> = IndexMap::new();
        let merged = as_obj(deep_merge_hocon_objects(base, overlay));
        assert_eq!(merged.get("a"), Some(&scalar("x")));
        assert_eq!(merged.len(), 1);

        let empty_base: IndexMap<String, HoconValue> = IndexMap::new();
        let overlay = as_obj(obj(&[("a", scalar("y"))]));
        let merged = as_obj(deep_merge_hocon_objects(empty_base, overlay));
        assert_eq!(merged.get("a"), Some(&scalar("y")));
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn deep_merge_handles_deeply_nested_without_quadratic_clones() {
        // Smoke test for the refactor's primary motivation. Builds a
        // 10-level-deep nested base and overlay, then merges. Before the
        // fix this would re-clone every subtree per level (O(N²) work);
        // after the fix it's O(N) total. We don't assert timing here —
        // the value is that this exercises the deep recursion path.
        fn build(depth: usize, leaf_label: &str) -> HoconValue {
            if depth == 0 {
                return scalar(leaf_label);
            }
            obj(&[("nested", build(depth - 1, leaf_label))])
        }
        let base = as_obj(build(10, "base-leaf"));
        let overlay = as_obj(build(10, "overlay-leaf"));
        let merged = deep_merge_hocon_objects(base, overlay);

        // Walk down to the leaf, assert overlay won.
        let mut cur = merged;
        for _ in 0..10 {
            let m = as_obj(cur);
            cur = m.get("nested").cloned().unwrap();
        }
        assert_eq!(cur, scalar("overlay-leaf"));
    }
}
