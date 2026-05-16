use crate::error::ResolveError;
use crate::numeric_array::numeric_object_to_array;
use crate::value::{HoconValue, ScalarValue};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

use super::types::{AppendPlaceholder, ResObj, ResolverValue, SubstPlaceholder};
use super::utils::{deep_merge_hocon_objects, lookup_path, segments_text_equal, segments_to_key};

pub(crate) struct SubstitutionResolver<'a> {
    root: &'a ResObj,
    env: &'a HashMap<String, String>,
    resolving: HashSet<String>,
    cache: HashMap<String, HoconValue>,
}

impl<'a> SubstitutionResolver<'a> {
    pub fn new(root: &'a ResObj, env: &'a HashMap<String, String>) -> Self {
        SubstitutionResolver {
            root,
            env,
            resolving: HashSet::new(),
            cache: HashMap::new(),
        }
    }

    pub fn resolve(&mut self) -> Result<HoconValue, ResolveError> {
        let root = self.root.clone();
        self.resolve_res_obj(&root)
    }

    fn resolve_res_obj(&mut self, obj: &ResObj) -> Result<HoconValue, ResolveError> {
        let mut result = IndexMap::new();
        for (key, val) in &obj.fields {
            match self.resolve_val(val, obj)? {
                Some(resolved) => {
                    // Delayed merge: if both current and prior resolve to objects, deep merge
                    if let HoconValue::Object(ref current_fields) = resolved {
                        if let Some(prior) = obj.prior_values.get(key) {
                            if let Some(HoconValue::Object(prior_fields)) =
                                self.resolve_val(prior, obj)?
                            {
                                let merged =
                                    deep_merge_hocon_objects(prior_fields, current_fields.clone());
                                result.insert(key.clone(), merged);
                                continue;
                            }
                        }
                    }
                    result.insert(key.clone(), resolved);
                }
                None => {
                    // Unresolved optional: fall back to prior value
                    if let Some(prior) = obj.prior_values.get(key) {
                        if let Some(prior_resolved) = self.resolve_val(prior, obj)? {
                            result.insert(key.clone(), prior_resolved);
                        }
                    }
                }
            }
        }
        Ok(HoconValue::Object(result))
    }

    fn resolve_val(
        &mut self,
        v: &ResolverValue,
        scope: &ResObj,
    ) -> Result<Option<HoconValue>, ResolveError> {
        match v {
            ResolverValue::Subst(s) => self.resolve_subst(s, scope),
            ResolverValue::Concat(c) => self
                .resolve_concat(&c.nodes, &c.separator_flags, scope)
                .map(Some),
            ResolverValue::Append(a) => self.resolve_append(a, scope).map(Some),
            ResolverValue::Obj(o) => self.resolve_res_obj(o).map(Some),
            ResolverValue::UnresolvedArray(items) => {
                let mut resolved_items = Vec::new();
                for item in items {
                    let resolved = self
                        .resolve_val(item, scope)?
                        .unwrap_or(HoconValue::Scalar(ScalarValue::null()));
                    resolved_items.push(resolved);
                }
                Ok(Some(HoconValue::Array(resolved_items)))
            }
            ResolverValue::Resolved(hv) => Ok(Some(hv.clone())),
        }
    }

    fn resolve_subst(
        &mut self,
        s: &SubstPlaceholder,
        scope: &ResObj,
    ) -> Result<Option<HoconValue>, ResolveError> {
        let key = segments_to_key(&s.segments);

        if let Some(cached) = self.cache.get(&key) {
            return Ok(Some(cached.clone()));
        }

        if self.resolving.contains(&key) {
            // Cycle detected: try prior value for self-referential substitutions
            let root_seg = s.segments.first().map(|s| s.text.as_str()).unwrap_or("");
            let prior = scope
                .prior_values
                .get(root_seg)
                .or_else(|| self.root.prior_values.get(root_seg));
            if let Some(prior) = prior {
                let prior = prior.clone();
                let mut fresh_resolving = self.resolving.clone();
                std::mem::swap(&mut self.resolving, &mut fresh_resolving);
                let result = self.resolve_val(&prior, scope);
                std::mem::swap(&mut self.resolving, &mut fresh_resolving);
                return result;
            }
            if s.optional {
                return Ok(None);
            }
            return Err(ResolveError {
                message: format!("circular substitution: {}", key),
                path: key,
                line: s.line,
                col: s.col,
            });
        }

        self.resolving.insert(key.clone());

        let result = self.resolve_subst_inner(s, scope, &key);

        self.resolving.remove(&key);
        result
    }

    fn resolve_subst_inner(
        &mut self,
        s: &SubstPlaceholder,
        scope: &ResObj,
        key: &str,
    ) -> Result<Option<HoconValue>, ResolveError> {
        let found = lookup_path(self.root, &s.segments).cloned();

        if let Some(found) = found {
            // Self-referential substitution: only use prior value when the substitution
            // path matches the key we found (e.g., b=${b} where fields[b]=Subst(b)).
            if matches!(found, ResolverValue::Subst(_) | ResolverValue::Concat(_)) {
                let is_self_ref = match &found {
                    ResolverValue::Subst(sub) => segments_text_equal(&sub.segments, &s.segments),
                    ResolverValue::Concat(c) => c.nodes.iter().any(|n| {
                        matches!(n, ResolverValue::Subst(sub) if segments_text_equal(&sub.segments, &s.segments))
                    }),
                    _ => false,
                };
                if is_self_ref {
                    let root_seg = s.segments.first().map(|s| s.text.as_str()).unwrap_or("");
                    let prior = scope
                        .prior_values
                        .get(root_seg)
                        .or_else(|| self.root.prior_values.get(root_seg))
                        .cloned();
                    if let Some(prior) = prior {
                        let result = self.resolve_val(&prior, scope)?;
                        if let Some(ref r) = result {
                            self.cache.insert(key.to_string(), r.clone());
                        }
                        return Ok(result);
                    }
                }
            }
            let mut result = self.resolve_val(&found, scope)?;

            // Delayed merge: if the resolved value is an Object and there is a prior
            // value for the root segment, resolve the prior and deep merge underneath.
            // Only apply for single-segment paths; for multi-segment paths (e.g. foo.bar),
            // the prior value of the root segment (foo) is a different object and must not
            // be merged into the resolved value of the full path.
            if s.segments.len() == 1 {
                if let Some(HoconValue::Object(ref current_fields)) = result {
                    let root_seg = s.segments.first().map(|s| s.text.as_str()).unwrap_or("");
                    let prior = self.root.prior_values.get(root_seg).cloned();
                    if let Some(prior) = prior {
                        if let Some(HoconValue::Object(prior_fields)) =
                            self.resolve_val(&prior, scope)?
                        {
                            let merged =
                                deep_merge_hocon_objects(prior_fields, current_fields.clone());
                            result = Some(merged);
                        }
                    }
                }
            }

            if let Some(ref r) = result {
                self.cache.insert(key.to_string(), r.clone());
            }
            return Ok(result);
        }

        // Env var fallback — use raw dot-join (no quoting) to match Lightbend behavior
        let env_key = s
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let env_result = self.env.get(&env_key).cloned().or_else(|| {
            if s.prefix_len > 0 && s.segments.len() > s.prefix_len {
                let original_key = s.segments[s.prefix_len..]
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                self.env.get(&original_key).cloned()
            } else {
                None
            }
        });
        if let Some(env_val) = env_result {
            let result = HoconValue::Scalar(ScalarValue::string(env_val));
            self.cache.insert(key.to_string(), result.clone());
            return Ok(Some(result));
        }

        if s.optional {
            return Ok(None);
        }

        Err(ResolveError {
            message: format!("could not resolve substitution: ${{{}}}", key),
            path: key.to_string(),
            line: s.line,
            col: s.col,
        })
    }

    fn resolve_concat(
        &mut self,
        nodes: &[ResolverValue],
        separator_flags: &[bool],
        scope: &ResObj,
    ) -> Result<HoconValue, ResolveError> {
        let mut resolved: Vec<(HoconValue, bool)> = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            let is_sep = separator_flags.get(i).copied().unwrap_or(false);
            if let Some(v) = self.resolve_val(n, scope)? {
                resolved.push((v, is_sep));
            }
        }

        if resolved.is_empty() {
            return Ok(HoconValue::Scalar(ScalarValue::null()));
        }
        if resolved.len() == 1 {
            return Ok(resolved.into_iter().next().unwrap().0);
        }

        // Pairwise left-to-right fold (NORMATIVE per spec §"Multi-piece concat is
        // left-to-right pairwise").
        //
        // Why a fold instead of a single-pass classify-then-dispatch loop:
        //   A single-pass loop that checks "any array present → array branch" converts
        //   each Object element independently. When adjacent Objects have overlapping
        //   numeric keys, independent conversion preserves both values (wrong). The
        //   spec requires Object+Object to merge first (so the later key wins), then
        //   convert when a list partner is reached. A pairwise fold matches Lightbend's
        //   ConfigConcatenation.consolidate semantics exactly.
        //
        // Separator handling:
        //   Separators (parser-synthesized whitespace tokens) are kept in the sequence
        //   but join_pair treats them as pass-through scalars for non-object/array
        //   concat. For object and array concat, separators are skipped (is_sep=true).
        //   This preserves the original behaviour where whitespace contributes to
        //   string concatenation (e.g. "foo bar") but is discarded for structured types.
        //
        // join_pair type-pair cases (separators are folded as scalars):
        //   Object + Object → deep-merge, skip is_sep between them (S10.3)
        //   Array  + Object → numeric_object_to_array; if Some concat arrays (S15.3)
        //   Object + Array  → numeric_object_to_array; if Some concat arrays (S15.3)
        //   Array  + Array  → array concat
        //   Scalar + *      → string concat (separator whitespace contributes its raw value)

        // Determine whether we are in an object/array concat (where separators are
        // skipped) or a scalar concat (where separators contribute their text).
        let has_structured = resolved
            .iter()
            .any(|(v, _)| matches!(v, HoconValue::Object(_) | HoconValue::Array(_)));

        if has_structured {
            // Object/array concat: filter separators first, then pairwise fold.
            let non_sep: Vec<HoconValue> = resolved
                .into_iter()
                .filter(|(_, is_sep)| !is_sep)
                .map(|(v, _)| v)
                .collect();

            if non_sep.is_empty() {
                return Ok(HoconValue::Scalar(ScalarValue::null()));
            }
            if non_sep.len() == 1 {
                return Ok(non_sep.into_iter().next().unwrap());
            }

            let mut iter = non_sep.into_iter();
            let first = iter.next().unwrap();
            return iter.try_fold(first, join_pair).map(Ok)?;
        }

        // Scalar-only concat: include separators so whitespace contributes to the result
        // (e.g., "foo bar" where the space token is a separator).
        let s: String = resolved.iter().map(|(v, _)| stringify_value(v)).collect();
        Ok(HoconValue::Scalar(ScalarValue::string(s)))
    }

    fn resolve_append(
        &mut self,
        a: &AppendPlaceholder,
        scope: &ResObj,
    ) -> Result<HoconValue, ResolveError> {
        let existing = self
            .resolve_val(&a.existing, scope)?
            .unwrap_or_else(|| HoconValue::Array(vec![]));
        let elem = self.resolve_val(&a.elem, scope)?;

        let mut items: Vec<HoconValue> = match existing {
            HoconValue::Array(arr) => arr,
            other => vec![other],
        };
        if let Some(e) = elem {
            items.push(e);
        }
        Ok(HoconValue::Array(items))
    }
}

/// Pairwise join for the left-to-right concat fold.
///
/// Implements the `join_pair(left, right)` spec pseudocode:
/// - Object + Object → deep-merge (S10.3)
/// - Array  + Object → attempt numeric_object_to_array; if Some → array-array concat
/// - Object + Array  → attempt numeric_object_to_array; if Some → array-array concat
/// - Array  + Array  → array-array concat
/// - other pairs     → string concat (scalars coerced to string)
///
/// The `Result` wrapper exists so it can be used directly with `try_fold`.
/// This function currently never produces an `Err` — type-mismatch cases (e.g.
/// a non-convertible Object next to an Array) push the unconverted Object into
/// the Array, which preserves the existing "mixed concat" behaviour rather than
/// erroring, consistent with how the original single-pass loop behaved.
fn join_pair(left: HoconValue, right: HoconValue) -> Result<HoconValue, ResolveError> {
    match (left, right) {
        // Object + Object → deep-merge (S10.3)
        (HoconValue::Object(lf), HoconValue::Object(rf)) => {
            Ok(deep_merge_hocon_objects(lf, rf))
        }

        // Array + Object → S15.3: try numeric-keyed object conversion
        (HoconValue::Array(mut arr), obj @ HoconValue::Object(_)) => {
            match numeric_object_to_array(&obj) {
                Some(converted) => arr.extend(converted),
                None => arr.push(obj),
            }
            Ok(HoconValue::Array(arr))
        }

        // Object + Array → S15.3 symmetric: try numeric-keyed object conversion
        (obj @ HoconValue::Object(_), HoconValue::Array(right_arr)) => {
            match numeric_object_to_array(&obj) {
                Some(mut converted) => {
                    converted.extend(right_arr);
                    Ok(HoconValue::Array(converted))
                }
                None => {
                    let mut arr = vec![obj];
                    arr.extend(right_arr);
                    Ok(HoconValue::Array(arr))
                }
            }
        }

        // Array + Array → array concat
        (HoconValue::Array(mut left_arr), HoconValue::Array(right_arr)) => {
            left_arr.extend(right_arr);
            Ok(HoconValue::Array(left_arr))
        }

        // Array + Scalar → push scalar into array (preserves prior single-pass behaviour
        // where scalar elements were appended to an in-progress array concat).
        (HoconValue::Array(mut arr), scalar) => {
            arr.push(scalar);
            Ok(HoconValue::Array(arr))
        }

        // Scalar + Array → prepend array to scalar (push scalar as first element).
        (scalar, HoconValue::Array(right_arr)) => {
            let mut arr = vec![scalar];
            arr.extend(right_arr);
            Ok(HoconValue::Array(arr))
        }

        // Scalar pairs → string concat
        (left, right) => {
            let s = format!("{}{}", stringify_value(&left), stringify_value(&right));
            Ok(HoconValue::Scalar(ScalarValue::string(s)))
        }
    }
}

fn stringify_value(v: &HoconValue) -> String {
    match v {
        HoconValue::Scalar(sv) => sv.raw.clone(),
        HoconValue::Array(_) => format!("{:?}", v),
        HoconValue::Object(_) => format!("{:?}", v),
    }
}
