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
