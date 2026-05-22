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

pub(crate) fn deep_merge_res_obj_into(dst: &mut ResObj, src: ResObj) {
    for (k, src_val) in src.fields {
        let dst_is_obj = dst.fields.get(&k).and_then(as_res_obj);
        let src_obj = as_res_obj(&src_val);

        if let (Some(mut dst_obj), Some(src_obj)) = (dst_is_obj, src_obj) {
            // #120 cross-impl: save dst's pre-merge value as the prior at the
            // OUTER level even when both sides are objects and we recurse.
            // Otherwise a `${k}` in the merged result (e.g. `o = { history =
            // ${o}, v = 2 }` included into a parent `o = { v = 1 }`) has no
            // lookback target — resolve_subst hits the "no prior" error.
            //
            // Key is bare-leaf: deep_merge operates per nesting level; for
            // top-level include-merge it matches the substitution path
            // directly (e.g. ${o} → key "o"). For nested include-merge the
            // included substitutions have already been relativized by
            // structure_builder, so they target the absolute outer path —
            // never the bare-leaf inner key. The bare-leaf save is correct
            // for the surface that needs it.
            if let Some(old) = dst.fields.get(&k) {
                let prior_existing = dst.prior_values.get(&k).cloned();
                if let Some(prior) =
                    super::fold_self_ref::fold_or_skip_prior(old, &k, prior_existing.as_ref())
                {
                    dst.prior_values.insert(k.clone(), prior);
                }
            }
            deep_merge_res_obj_into(&mut dst_obj, src_obj);
            dst.fields.insert(k, ResolverValue::Obj(dst_obj));
            continue;
        }

        if let Some(old) = dst.fields.get(&k) {
            // Same fold-or-skip discipline as the both-objects branch above.
            // The non-merge case was the pre-#120 save site; with #120 we
            // also apply the fold so the saved prior is self-ref-free
            // (chain-class invariant — applies when the include-merged value
            // is itself a self-referential concat from a sibling include).
            let prior_existing = dst.prior_values.get(&k).cloned();
            if let Some(prior) =
                super::fold_self_ref::fold_or_skip_prior(old, &k, prior_existing.as_ref())
            {
                dst.prior_values.insert(k.clone(), prior);
            }
        }
        dst.fields.insert(k, src_val);
    }
    // Carry over prior_values from src that aren't already set in dst.
    // This preserves delayed-merge chains from included files.
    for (k, src_prior) in src.prior_values {
        if !dst.prior_values.contains_key(&k) {
            dst.prior_values.insert(k, src_prior);
        }
    }
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
        ResolverValue::Append(a) => {
            relativize_subst_paths(&mut a.existing, prefix_segments);
            relativize_subst_paths(&mut a.elem, prefix_segments);
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
    use super::*;

    fn seg(text: &str) -> Segment {
        Segment {
            text: text.to_string(),
            line: 1,
            col: 1,
        }
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
