// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! Helpers for chained-self-referential-substitution support.
//!
//! Port of go.hocon's `internal/resolver/foldselfref.go` (PRs #121 and #123,
//! covering issues #118 and #120). Cross-impl with go.hocon v1.5.2.
//!
//! The chain bug: when a key is self-referentially appended N≥3 times
//! (`a = ${a} [...]` repeated, or `a = [${a}, ...]` repeated, or
//! `o = { history = ${o}, ... }` repeated) — directly, via includes, or
//! across nested paths — the resolver's `prior_values` map (one-deep per key)
//! gets overwritten with a self-referentially-malformed value, and
//! `resolve_subst`'s prior-resolution branch loops forever.
//!
//! The fix folds occurrences of `${key}` inside the value about to be saved
//! as `prior_values[key]` against the OLD prior, so by induction every saved
//! prior is self-ref-free.
//!
//! Scope: walks `Subst` / `Concat` / `UnresolvedArray` / `Obj` recursively.
//! This is the union of #118 (Subst/Concat patterns) and #120
//! (UnresolvedArray/Obj interior patterns).

use super::types::{ConcatPlaceholder, ResObj, ResolverValue, SubstPlaceholder};
use super::utils::segments_to_key;
use crate::lexer::Segment;

/// Returns true if `v` contains at least one `Subst` whose dotted-path key
/// equals `full_key`. Walks Subst / Concat / UnresolvedArray / Obj (all
/// wrapping shapes covered by the #118/#120 union).
pub(crate) fn contains_self_ref(v: &ResolverValue, full_key: &str) -> bool {
    match v {
        ResolverValue::Subst(sp) => !sp.known_absent && subst_full_key(sp) == full_key,
        ResolverValue::Concat(c) => c.nodes.iter().any(|n| contains_self_ref(n, full_key)),
        ResolverValue::UnresolvedArray(elems) => {
            elems.iter().any(|e| contains_self_ref(e, full_key))
        }
        ResolverValue::Obj(o) => o.fields.values().any(|f| contains_self_ref(f, full_key)),
        _ => false,
    }
}

/// Returns a copy of `v` with every `Subst` whose dotted-path key equals
/// `full_key` replaced by `replacement`. If `v` contains no such reference,
/// returns `v.clone()` (caller-side cheap because we re-borrow patterns).
///
/// Scope matches `contains_self_ref`.
pub(crate) fn fold_self_ref(
    v: &ResolverValue,
    full_key: &str,
    replacement: &ResolverValue,
) -> ResolverValue {
    match v {
        ResolverValue::Subst(sp) if subst_full_key(sp) == full_key => replacement.clone(),
        ResolverValue::Concat(c) => ResolverValue::Concat(ConcatPlaceholder {
            nodes: c
                .nodes
                .iter()
                .map(|n| fold_self_ref(n, full_key, replacement))
                .collect(),
            separator_flags: c.separator_flags.clone(),
            line: c.line,
            col: c.col,
        }),
        ResolverValue::UnresolvedArray(elems) => ResolverValue::UnresolvedArray(
            elems
                .iter()
                .map(|e| fold_self_ref(e, full_key, replacement))
                .collect(),
        ),
        ResolverValue::Obj(o) => {
            let mut new_fields = indexmap::IndexMap::new();
            for (k, val) in &o.fields {
                new_fields.insert(k.clone(), fold_self_ref(val, full_key, replacement));
            }
            ResolverValue::Obj(ResObj {
                fields: new_fields,
                // Preserve prior_values from the original so per-object look-back
                // continues to find them post-fold.
                prior_values: o.prior_values.clone(),
                reset_keys: o.reset_keys.clone(),
            })
        }
        _ => v.clone(),
    }
}

/// Replace every `known_absent` self-reference to `full_key` with `replacement`.
///
/// Counterpart to [`fold_self_ref`], which targets the NON-absent self-refs. An
/// included file's bare `+=` chain folds its bottom-most `${?key}` to
/// `known_absent` while building in isolation (no in-file prior). When that file
/// is merged into a destination that already has a value for `key`, this
/// re-opens the absent bottom so the included chain accumulates onto the
/// destination's value across the include boundary (go.hocon#134). Walks the
/// same Subst / Concat / UnresolvedArray / Obj shapes as [`fold_self_ref`].
pub(crate) fn fold_known_absent_self_ref(
    v: &ResolverValue,
    full_key: &str,
    replacement: &ResolverValue,
) -> ResolverValue {
    match v {
        ResolverValue::Subst(sp) if sp.known_absent && subst_full_key(sp) == full_key => {
            replacement.clone()
        }
        ResolverValue::Concat(c) => ResolverValue::Concat(ConcatPlaceholder {
            nodes: c
                .nodes
                .iter()
                .map(|n| fold_known_absent_self_ref(n, full_key, replacement))
                .collect(),
            separator_flags: c.separator_flags.clone(),
            line: c.line,
            col: c.col,
        }),
        ResolverValue::UnresolvedArray(elems) => ResolverValue::UnresolvedArray(
            elems
                .iter()
                .map(|e| fold_known_absent_self_ref(e, full_key, replacement))
                .collect(),
        ),
        ResolverValue::Obj(o) => {
            let mut new_fields = indexmap::IndexMap::new();
            for (k, val) in &o.fields {
                new_fields.insert(
                    k.clone(),
                    fold_known_absent_self_ref(val, full_key, replacement),
                );
            }
            ResolverValue::Obj(ResObj {
                fields: new_fields,
                prior_values: o.prior_values.clone(),
                reset_keys: o.reset_keys.clone(),
            })
        }
        _ => v.clone(),
    }
}

/// Three-way decision at a prior-save site:
///
///   * `prior` has no self-ref to `full_key`         → save as-is        → `Some(prior.clone())`
///   * `prior` has self-ref AND `old` is `Some(_)`   → fold against old  → `Some(folded)`
///   * optional self-ref AND `old` is `None`         → fold to absent    → `Some(folded)`
///   * required self-ref AND `old` is `None`         → skip save         → `None`
///
/// The no-prior optional case preserves S13a.13's "optional self-ref with no
/// prior resolves to undefined" rule while still saving the literal parts of
/// a concat for the next overwrite (sr15: `${?a}1; ${?a}2` → `12`).
pub(crate) fn fold_or_skip_prior(
    prior: &ResolverValue,
    full_key: &str,
    old: Option<&ResolverValue>,
) -> Option<ResolverValue> {
    if !contains_self_ref(prior, full_key) {
        return Some(prior.clone());
    }
    if let Some(o) = old {
        return Some(fold_self_ref(prior, full_key, o));
    }
    fold_optional_self_ref_absent(prior, full_key)
}

/// Mixed-concat contract (review #124 item a):
///
/// This function walks a value tree and decides, node by node, whether a node
/// containing a self-reference can be included in the saved prior when there is
/// no previously-assigned value for the key (`old == None`).
///
/// The `?`-propagation rule applies ONLY to nodes that are the self-referencing
/// substitution itself:
///
///   * Node IS the self-ref AND optional  → mark known_absent → `Some(...)`.
///     At resolve time the concat-omission rule (Phase 6 §3b) drops it.
///   * Node IS the self-ref AND required  → return `None` → `?` short-circuits
///     the entire enclosing `Concat` → save is skipped entirely → at resolve
///     time the required-self-ref-no-prior error fires (sr05-like).
///   * Node is NOT the self-ref (e.g. a literal, `${b}`, or a different key's
///     substitution) → `_ => Some(v.clone())` → preserved in the saved prior
///     → evaluated normally at resolve time.
///
/// Consequence: mixed concats like `a = ${?a}foo${b}` or `a = ${?a}foo${?b}`
/// are handled correctly without special-casing:
///   - `${b}` (required, external) is saved and errors at resolve time if b
///     has no definition.
///   - `${?b}` (optional, external) is saved and drops silently at resolve
///     time if b is absent.
///   - `a = ${?a}foo${a}` (required self-ref in concat) short-circuits → save
///     skipped → required-self-ref error at resolve time.
///
/// Tests: sr17 (pure-optional), sr18 (required external), sr19 (required
/// self-ref mixed) in tests/self_ref_lookback_test.rs.
fn fold_optional_self_ref_absent(v: &ResolverValue, full_key: &str) -> Option<ResolverValue> {
    match v {
        ResolverValue::Subst(sp) if subst_full_key(sp) == full_key => {
            if !sp.optional {
                return None;
            }
            let mut absent = sp.clone();
            absent.known_absent = true;
            Some(ResolverValue::Subst(absent))
        }
        ResolverValue::Concat(c) => {
            let mut nodes = Vec::with_capacity(c.nodes.len());
            for n in &c.nodes {
                nodes.push(fold_optional_self_ref_absent(n, full_key)?);
            }
            Some(ResolverValue::Concat(ConcatPlaceholder {
                nodes,
                separator_flags: c.separator_flags.clone(),
                line: c.line,
                col: c.col,
            }))
        }
        ResolverValue::UnresolvedArray(elems) => {
            let mut folded = Vec::with_capacity(elems.len());
            for e in elems {
                folded.push(fold_optional_self_ref_absent(e, full_key)?);
            }
            Some(ResolverValue::UnresolvedArray(folded))
        }
        ResolverValue::Obj(o) => {
            let mut new_fields = indexmap::IndexMap::new();
            for (k, val) in &o.fields {
                new_fields.insert(k.clone(), fold_optional_self_ref_absent(val, full_key)?);
            }
            Some(ResolverValue::Obj(ResObj {
                fields: new_fields,
                prior_values: o.prior_values.clone(),
                reset_keys: o.reset_keys.clone(),
            }))
        }
        _ => Some(v.clone()),
    }
}

/// Dotted-path key of a substitution placeholder's segments. Segments are
/// already relativized at this point if the placeholder lives inside an
/// included file under a nested path prefix.
pub(crate) fn subst_full_key(sp: &SubstPlaceholder) -> String {
    segments_to_key(&sp.segments)
}

#[cfg(test)]
mod tests {
    //! Unit tests for `fold_optional_self_ref_absent` branch coverage.
    //!
    //! `fold_optional_self_ref_absent` is private; exercised via the
    //! `fold_or_skip_prior(prior, key, None)` path, which calls
    //! `fold_optional_self_ref_absent` when `contains_self_ref` is true.
    //!
    //! For the fallback branch (`_ => Some(v.clone())`), we call
    //! `fold_optional_self_ref_absent` directly since it is in the same module.
    //!
    //! Covers: UnresolvedArray, Obj, and fallback branches that have no
    //! fixture coverage from sr01–sr21 (sr15 only exercises Subst + Concat).

    use super::*;
    use crate::lexer::Segment;
    use crate::value::{HoconValue, ScalarValue};
    use indexmap::IndexMap;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn seg(text: &str) -> Segment {
        Segment {
            text: text.to_string(),
            line: 1,
            col: 1,
        }
    }

    fn make_subst(key: &str, optional: bool) -> ResolverValue {
        ResolverValue::Subst(SubstPlaceholder {
            segments: vec![seg(key)],
            optional,
            known_absent: false,
            list_suffix: false,
            line: 1,
            col: 1,
            prefix_len: 0,
        })
    }

    fn subst_known_absent(v: &ResolverValue) -> bool {
        match v {
            ResolverValue::Subst(sp) => sp.known_absent,
            _ => panic!("expected Subst, got {:?}", v),
        }
    }

    // ── UnresolvedArray branch ───────────────────────────────────────────────

    /// UnresolvedArray containing an optional self-ref item → item folded to
    /// known_absent=true. Tests the `ResolverValue::UnresolvedArray` arm of
    /// `fold_optional_self_ref_absent`.
    #[test]
    fn unresolved_array_optional_self_ref_folded() {
        let array = ResolverValue::UnresolvedArray(vec![make_subst("a", true)]);
        let result = fold_or_skip_prior(&array, "a", None);
        assert!(
            result.is_some(),
            "expected Some for optional self-ref array"
        );
        match result.unwrap() {
            ResolverValue::UnresolvedArray(elems) => {
                assert_eq!(elems.len(), 1);
                assert!(
                    subst_known_absent(&elems[0]),
                    "array item should be known_absent after fold"
                );
            }
            other => panic!("expected UnresolvedArray, got {:?}", other),
        }
    }

    /// UnresolvedArray containing a required self-ref item → returns None
    /// (short-circuit: save is skipped).
    #[test]
    fn unresolved_array_required_self_ref_returns_none() {
        let array = ResolverValue::UnresolvedArray(vec![make_subst("a", false)]);
        let result = fold_or_skip_prior(&array, "a", None);
        assert!(
            result.is_none(),
            "expected None for required self-ref in array"
        );
    }

    /// UnresolvedArray with no self-ref to key "a" → cloned and returned as-is.
    #[test]
    fn unresolved_array_no_self_ref_passes_through() {
        // contains_self_ref("a") is false for ${?b} → fold_or_skip_prior
        // takes the `Some(prior.clone())` fast path. Tests the no-self-ref
        // clone path through the array.
        let array = ResolverValue::UnresolvedArray(vec![make_subst("b", true)]);
        let result = fold_or_skip_prior(&array, "a", None);
        assert!(result.is_some());
        match result.unwrap() {
            ResolverValue::UnresolvedArray(elems) => {
                assert_eq!(elems.len(), 1);
                // "b" is not "a" so known_absent must remain false
                assert!(!subst_known_absent(&elems[0]));
            }
            other => panic!("expected UnresolvedArray, got {:?}", other),
        }
    }

    /// UnresolvedArray with mixed items: optional self-ref + non-self-ref.
    /// Self-ref item folded; other item preserved unchanged.
    #[test]
    fn unresolved_array_mixed_items_only_self_ref_folded() {
        let array = ResolverValue::UnresolvedArray(vec![
            make_subst("a", true), // self-ref → fold
            make_subst("b", true), // not self-ref → preserve
        ]);
        let result = fold_or_skip_prior(&array, "a", None);
        assert!(result.is_some());
        match result.unwrap() {
            ResolverValue::UnresolvedArray(elems) => {
                assert_eq!(elems.len(), 2);
                assert!(
                    subst_known_absent(&elems[0]),
                    "first item should be known_absent"
                );
                assert!(
                    !subst_known_absent(&elems[1]),
                    "second item should not be known_absent"
                );
            }
            other => panic!("expected UnresolvedArray, got {:?}", other),
        }
    }

    // ── Obj branch ───────────────────────────────────────────────────────────

    /// ResObj with a field containing an optional self-ref → field folded to
    /// known_absent=true. Tests the `ResolverValue::Obj` arm of
    /// `fold_optional_self_ref_absent`.
    #[test]
    fn obj_optional_self_ref_field_folded() {
        let mut fields = IndexMap::new();
        fields.insert("history".to_string(), make_subst("a", true));
        let obj = ResolverValue::Obj(ResObj {
            fields,
            prior_values: IndexMap::new(),
            reset_keys: std::collections::HashSet::new(),
        });
        let result = fold_or_skip_prior(&obj, "a", None);
        assert!(
            result.is_some(),
            "expected Some for optional self-ref in obj field"
        );
        match result.unwrap() {
            ResolverValue::Obj(o) => {
                let field = o.fields.get("history").expect("history field missing");
                assert!(
                    subst_known_absent(field),
                    "obj field should be known_absent after fold"
                );
            }
            other => panic!("expected Obj, got {:?}", other),
        }
    }

    /// ResObj with a field containing a required self-ref → returns None.
    #[test]
    fn obj_required_self_ref_field_returns_none() {
        let mut fields = IndexMap::new();
        fields.insert("history".to_string(), make_subst("a", false));
        let obj = ResolverValue::Obj(ResObj {
            fields,
            prior_values: IndexMap::new(),
            reset_keys: std::collections::HashSet::new(),
        });
        let result = fold_or_skip_prior(&obj, "a", None);
        assert!(
            result.is_none(),
            "expected None for required self-ref in obj field"
        );
    }

    /// ResObj with multiple fields: only the self-ref field is folded.
    #[test]
    fn obj_multiple_fields_only_self_ref_folded() {
        let mut fields = IndexMap::new();
        fields.insert("history".to_string(), make_subst("a", true));
        fields.insert("other".to_string(), make_subst("b", true));
        let obj = ResolverValue::Obj(ResObj {
            fields,
            prior_values: IndexMap::new(),
            reset_keys: std::collections::HashSet::new(),
        });
        let result = fold_or_skip_prior(&obj, "a", None);
        assert!(result.is_some());
        match result.unwrap() {
            ResolverValue::Obj(o) => {
                let history = o.fields.get("history").expect("history missing");
                let other = o.fields.get("other").expect("other missing");
                assert!(
                    subst_known_absent(history),
                    "history should be known_absent"
                );
                assert!(
                    !subst_known_absent(other),
                    "other should not be known_absent"
                );
            }
            other => panic!("expected Obj, got {:?}", other),
        }
    }

    // ── Fallback branch (`_ => Some(v.clone())`) ─────────────────────────────

    /// Resolved scalar value has no self-ref → `fold_optional_self_ref_absent`
    /// takes the `_` fallback arm, returning `Some(v.clone())`.
    ///
    /// We call `fold_optional_self_ref_absent` directly because
    /// `contains_self_ref` on a Resolved value returns false, so
    /// `fold_or_skip_prior` would return `Some(prior.clone())` before ever
    /// calling `fold_optional_self_ref_absent`.  The fallback arm is only
    /// reachable from within a recursive call where a parent node (e.g.
    /// UnresolvedArray or Obj) contains a self-ref but one of its children is
    /// a non-self-ref Resolved value.
    #[test]
    fn fallback_resolved_scalar_passthrough() {
        let scalar =
            ResolverValue::Resolved(HoconValue::Scalar(ScalarValue::string("hello".to_string())));
        // Direct call to exercise the `_` arm.
        let result = fold_optional_self_ref_absent(&scalar, "a");
        assert!(result.is_some());
        match result.unwrap() {
            ResolverValue::Resolved(HoconValue::Scalar(sv)) => {
                assert_eq!(sv.raw, "hello");
            }
            other => panic!("expected Resolved(Scalar), got {:?}", other),
        }
    }

    /// Resolved number scalar passes through the fallback arm unchanged.
    #[test]
    fn fallback_resolved_number_passthrough() {
        let num =
            ResolverValue::Resolved(HoconValue::Scalar(ScalarValue::number("42".to_string())));
        let result = fold_optional_self_ref_absent(&num, "a");
        assert!(result.is_some());
        match result.unwrap() {
            ResolverValue::Resolved(HoconValue::Scalar(sv)) => {
                assert_eq!(sv.raw, "42");
            }
            other => panic!("expected Resolved(Scalar), got {:?}", other),
        }
    }

    /// UnresolvedArray with Resolved scalar + optional self-ref: verifies that
    /// the Resolved item hits the `_` fallback arm during recursive traversal.
    #[test]
    fn unresolved_array_resolved_item_hits_fallback() {
        let array = ResolverValue::UnresolvedArray(vec![
            ResolverValue::Resolved(HoconValue::Scalar(ScalarValue::string("x".to_string()))),
            make_subst("a", true),
        ]);
        let result = fold_or_skip_prior(&array, "a", None);
        assert!(result.is_some());
        match result.unwrap() {
            ResolverValue::UnresolvedArray(elems) => {
                assert_eq!(elems.len(), 2);
                // First item is Resolved scalar → passes through fallback
                match &elems[0] {
                    ResolverValue::Resolved(HoconValue::Scalar(sv)) => {
                        assert_eq!(sv.raw, "x");
                    }
                    other => panic!("expected Resolved(Scalar) at [0], got {:?}", other),
                }
                // Second item is the self-ref → folded to known_absent
                assert!(subst_known_absent(&elems[1]));
            }
            other => panic!("expected UnresolvedArray, got {:?}", other),
        }
    }
}

/// Recursively folds nested self-references inside a value tree using each
/// enclosing `ResObj`'s `prior_values` as the substitution target. This remains
/// necessary when an object assignment overwrites existing child keys, but sr13
/// avoids using it for pure additions so an already-folded prior is not saved
/// and folded again on a later field.
pub(crate) fn fold_nested_self_refs(v: &ResolverValue, path_prefix: &[String]) -> ResolverValue {
    if let ResolverValue::Obj(o) = v {
        let mut new_fields = indexmap::IndexMap::new();
        for (k, field_val) in &o.fields {
            let mut child_path = path_prefix.to_vec();
            child_path.push(k.clone());
            let full_key =
                super::utils::string_segments_to_key(child_path.iter().map(String::as_str));
            let folded_field = fold_nested_self_refs(field_val, &child_path);
            let final_val = if contains_self_ref(&folded_field, &full_key) {
                if let Some(leaf_prior) = o.prior_values.get(k) {
                    let leaf_prior_folded = fold_nested_self_refs(leaf_prior, &child_path);
                    fold_self_ref(&folded_field, &full_key, &leaf_prior_folded)
                } else {
                    folded_field
                }
            } else {
                folded_field
            };
            new_fields.insert(k.clone(), final_val);
        }
        ResolverValue::Obj(ResObj {
            fields: new_fields,
            prior_values: o.prior_values.clone(),
            reset_keys: o.reset_keys.clone(),
        })
    } else {
        v.clone()
    }
}

/// Path-equality walk: returns true if `v` contains a `Subst` whose segments
/// text-equal `target`. Used by `resolve_subst`'s self-ref detection where a
/// lookup returns a value containing the same placeholder being currently
/// resolved.
///
/// rs.hocon's pre-#120 check used path equality already (in contrast to
/// go.hocon's pointer identity); this helper preserves that criterion and
/// just widens the search scope through `Concat` / `UnresolvedArray` /
/// `Obj`.
pub(crate) fn contains_subst_by_path(v: &ResolverValue, target: &[Segment]) -> bool {
    match v {
        ResolverValue::Subst(sp) => {
            !sp.known_absent && super::utils::segments_text_equal(&sp.segments, target)
        }
        ResolverValue::Concat(c) => c.nodes.iter().any(|n| contains_subst_by_path(n, target)),
        ResolverValue::UnresolvedArray(elems) => {
            elems.iter().any(|e| contains_subst_by_path(e, target))
        }
        ResolverValue::Obj(o) => o.fields.values().any(|f| contains_subst_by_path(f, target)),
        _ => false,
    }
}
