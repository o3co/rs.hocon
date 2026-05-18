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
                .resolve_concat(&c.nodes, &c.separator_flags, c.line, c.col, scope)
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
        // Cache key includes list_suffix to prevent `${X}` and `${X[]}` collisions:
        // both resolve via different code paths (scalar fallback vs resolve_env_list)
        // and can produce different values, so they must occupy distinct cache slots.
        // Convergent with ts.hocon fix (same bug pattern). go.hocon is unaffected
        // because its cache is only used for self-ref recovery, not general memo.
        // Pin: tests/env_var_list_test.rs cache-disambiguation regression.
        let key = if s.list_suffix {
            format!("{}[]", segments_to_key(&s.segments))
        } else {
            segments_to_key(&s.segments)
        };

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

        // S13c: env-var list expansion — `${X[]}` / `${?X[]}`.
        // When list_suffix=true and config lookup missed, delegate entirely to
        // resolve_env_list. The scalar env fallback below is SUPPRESSED (S13c.5).
        if s.list_suffix {
            let result = self.resolve_env_list(s, key)?;
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

    /// Resolve env-var-list expansion for `${X[]}` / `${?X[]}` (S13c).
    ///
    /// Candidates are tried in order: fully-qualified base first, then bare
    /// (prefix-stripped) base (matching the scalar env fallback order).
    /// First candidate whose `<base>_0` key is present in the env wins entirely —
    /// no cross-base merging. Empty-string values are preserved (ev10).
    ///
    /// Returns:
    /// - `Ok(Some(HoconValue::Array(...)))` — one or more elements found.
    /// - `Ok(None)` — no elements found AND `s.optional`.
    /// - `Err(ResolveError)` — no elements found AND `!s.optional`.
    fn resolve_env_list(
        &self,
        s: &SubstPlaceholder,
        key: &str,
    ) -> Result<Option<HoconValue>, ResolveError> {
        // Build candidate base names (same order as scalar env fallback).
        let full_base = s
            .segments
            .iter()
            .map(|seg| seg.text.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let mut candidates: Vec<String> = vec![full_base];
        if s.prefix_len > 0 && s.segments.len() > s.prefix_len {
            let bare_base = s.segments[s.prefix_len..]
                .iter()
                .map(|seg| seg.text.as_str())
                .collect::<Vec<_>>()
                .join(".");
            candidates.push(bare_base);
        }

        for base in &candidates {
            let probe = format!("{}_0", base);
            if self.env.contains_key(&probe) {
                // This base has _0 — scan _0, _1, … until first absent key.
                let mut elements: Vec<HoconValue> = Vec::new();
                let mut i: usize = 0;
                loop {
                    let k = format!("{}_{}", base, i);
                    match self.env.get(&k) {
                        Some(v) => {
                            elements.push(HoconValue::Scalar(ScalarValue::string(v.clone())));
                            i += 1;
                        }
                        None => break,
                    }
                }
                return Ok(Some(HoconValue::Array(elements)));
            }
        }

        // No candidate base had _0.
        if s.optional {
            return Ok(None);
        }
        Err(ResolveError {
            message: format!(
                "could not resolve substitution: ${{{key}}} (no environment variable {}_0 found)",
                candidates[0]
            ),
            path: key.to_string(),
            line: s.line,
            col: s.col,
        })
    }

    fn resolve_concat(
        &mut self,
        nodes: &[ResolverValue],
        separator_flags: &[bool],
        line: usize,
        col: usize,
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
        //   Array  + Object → numeric_object_to_array; if Some → concat; if None → Err (S10.4)
        //   Object + Array  → symmetric (S10.4)
        //   Array  + Array  → array concat
        //   Array  + Scalar → Err (S10.13)
        //   Scalar + Array  → Err (S10.13)
        //   Scalar + Object → Err (S10.13)
        //   Object + Scalar → Err (S10.13)
        //   Scalar + Scalar → string concat (separator whitespace contributes its raw value)

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
            return iter
                .try_fold(first, |l, r| join_pair(l, r, line, col))
                .map(Ok)?;
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
/// Implements the `join_pair(left, right)` spec pseudocode per S10/S15.
/// Allowed pairs: Object+Object (deep-merge), Array+Object (S15 numeric bridge,
/// else Err S10.4), Object+Array (symmetric), Array+Array (concat),
/// Scalar+Scalar (string-concat). All other pairs return `Err(ResolveError)`.
///
/// Produces `Err(ResolveError)` for every spec-disallowed type pair
/// (S10.4 array/object mix, S10.13 scalar/structured mix, S10.19 subst-resolved).
fn join_pair(
    left: HoconValue,
    right: HoconValue,
    line: usize,
    col: usize,
) -> Result<HoconValue, ResolveError> {
    match (left, right) {
        // Object + Object → deep-merge (S10.3)
        (HoconValue::Object(lf), HoconValue::Object(rf)) => Ok(deep_merge_hocon_objects(lf, rf)),

        // Array + Object → S15.3: try numeric-keyed object conversion; error if None (S10.4)
        (HoconValue::Array(mut arr), obj @ HoconValue::Object(_)) => {
            match numeric_object_to_array(&obj) {
                Some(converted) => {
                    arr.extend(converted);
                    Ok(HoconValue::Array(arr))
                }
                None => Err(ResolveError::concat_type_mismatch(
                    "array",
                    "object",
                    line,
                    col,
                    String::new(),
                )),
            }
        }

        // Object + Array → S15.3 symmetric; error if None (S10.4)
        (obj @ HoconValue::Object(_), HoconValue::Array(right_arr)) => {
            match numeric_object_to_array(&obj) {
                Some(mut converted) => {
                    converted.extend(right_arr);
                    Ok(HoconValue::Array(converted))
                }
                None => Err(ResolveError::concat_type_mismatch(
                    "object",
                    "array",
                    line,
                    col,
                    String::new(),
                )),
            }
        }

        // Array + Array → array concat
        (HoconValue::Array(mut left_arr), HoconValue::Array(right_arr)) => {
            left_arr.extend(right_arr);
            Ok(HoconValue::Array(left_arr))
        }

        // Array + Scalar → error per S10.13 (spec L373: arrays invalid in string concat)
        (HoconValue::Array(_), scalar) => Err(ResolveError::concat_type_mismatch(
            "array",
            type_name(&scalar),
            line,
            col,
            String::new(),
        )),

        // Scalar + Array → error per S10.13
        (scalar, HoconValue::Array(_)) => Err(ResolveError::concat_type_mismatch(
            type_name(&scalar),
            "array",
            line,
            col,
            String::new(),
        )),

        // Scalar pairs: reject if either side is an Object (S10.13); string-concat otherwise
        (left, right) => {
            if matches!(left, HoconValue::Object(_)) || matches!(right, HoconValue::Object(_)) {
                return Err(ResolveError::concat_type_mismatch(
                    type_name(&left),
                    type_name(&right),
                    line,
                    col,
                    String::new(),
                ));
            }
            let s = format!("{}{}", stringify_value(&left), stringify_value(&right));
            Ok(HoconValue::Scalar(ScalarValue::string(s)))
        }
    }
}

/// Return the type-name string for a HoconValue, used in error messages.
///
/// For scalars, returns the specific subtype name ("null", "boolean", "number",
/// "string") per spec §"Required content in the error message".
fn type_name(v: &HoconValue) -> &'static str {
    match v {
        HoconValue::Object(_) => "object",
        HoconValue::Array(_) => "array",
        HoconValue::Scalar(sv) => match sv.value_type {
            crate::value::ScalarType::Null => "null",
            crate::value::ScalarType::Boolean => "boolean",
            crate::value::ScalarType::Number => "number",
            crate::value::ScalarType::String => "string",
            _ => "scalar",
        },
    }
}

fn stringify_value(v: &HoconValue) -> String {
    match v {
        HoconValue::Scalar(sv) => sv.raw.clone(),
        HoconValue::Array(_) => format!("{:?}", v),
        HoconValue::Object(_) => format!("{:?}", v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{ScalarType, ScalarValue};

    /// Fix #2: type_name must return scalar subtype names (not "scalar").
    #[test]
    fn type_name_null_scalar() {
        let v = HoconValue::Scalar(ScalarValue::null());
        assert_eq!(type_name(&v), "null");
    }

    #[test]
    fn type_name_boolean_scalar() {
        let v = HoconValue::Scalar(ScalarValue::boolean(true));
        assert_eq!(type_name(&v), "boolean");
    }

    #[test]
    fn type_name_number_scalar() {
        let v = HoconValue::Scalar(ScalarValue::number("42".to_string()));
        assert_eq!(type_name(&v), "number");
    }

    #[test]
    fn type_name_string_scalar() {
        let v = HoconValue::Scalar(ScalarValue::string("hello".to_string()));
        assert_eq!(type_name(&v), "string");
    }

    #[test]
    fn concat_error_message_contains_null_not_scalar() {
        // a = null [1] → null + array → error message must say "null", not "scalar"
        let err = join_pair(
            HoconValue::Scalar(ScalarValue::null()),
            HoconValue::Array(vec![]),
            0,
            0,
        )
        .unwrap_err();
        assert!(
            err.message.contains("null"),
            "expected 'null' in error message, got: {}",
            err.message
        );
        assert!(
            !err.message.contains("scalar"),
            "error message must not say 'scalar', got: {}",
            err.message
        );
    }

    /// Fix #3: concat type-mismatch errors must include non-zero line/col.
    #[test]
    fn concat_type_mismatch_error_has_position() {
        // Line 2, col 5 is where the concat value starts: "a = [1] {b:1}"
        // The value [1] {b:1} is a concat whose offending pair is array+object.
        let input = "\na = [1] {b: 1}\n";
        let env = std::collections::HashMap::new();
        let err = crate::parse_with_env(input, &env).unwrap_err();
        if let crate::HoconError::Resolve(re) = err {
            assert!(
                re.line != 0,
                "concat type-mismatch error must have non-zero line, got line={}",
                re.line
            );
            assert!(
                re.col != 0,
                "concat type-mismatch error must have non-zero col, got col={}",
                re.col
            );
        } else {
            panic!("expected ResolveError, got: {:?}", err);
        }
    }

    /// Fix #3: scalar+object concat error must also carry position.
    #[test]
    fn concat_scalar_plus_object_error_has_position() {
        let input = "a = x {b: 1}\n";
        let env = std::collections::HashMap::new();
        let err = crate::parse_with_env(input, &env).unwrap_err();
        if let crate::HoconError::Resolve(re) = err {
            assert!(re.line != 0, "line must be non-zero, got {}", re.line);
            assert!(re.col != 0, "col must be non-zero, got {}", re.col);
        } else {
            panic!("expected ResolveError, got: {:?}", err);
        }
    }
}
