use crate::error::ResolveError;
use crate::numeric_array::numeric_object_to_array;
use crate::value::{HoconValue, ScalarValue};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

use super::types::{AppendPlaceholder, ResObj, ResolverValue, SubstPlaceholder};
use super::utils::{deep_merge_hocon_objects, lookup_path, segments_to_key};

pub(crate) struct SubstitutionResolver<'a> {
    root: &'a ResObj,
    env: &'a HashMap<String, String>,
    resolving: HashSet<String>,
    cache: HashMap<String, HoconValue>,
    /// When false, env-var fallback in resolve_subst_inner is skipped.
    use_system_environment: bool,
    /// When true, missing mandatory substitutions yield Ok(None) instead of Err.
    allow_unresolved: bool,
    /// Path stack tracking the full dotted path of the field currently being
    /// assigned in `resolve_res_obj`.  Leaf keys are pushed on entry to each
    /// `for (key, val)` iteration and popped on exit, so nested objects build
    /// up the full path (e.g. `["foo", "a"]` while resolving `foo.a`).
    ///
    /// Used to tighten the self-ref detection: a substitution `${x}` whose
    /// found value contains a self-reference is only a true self-reference when
    /// the field we are assigning IS `x` (i.e., the joined path == subst key).
    ///
    /// Spec deviation: the S13a.13 spec ★1 decision #1 specified path-equality
    /// preservation for self-ref detection. Round-2 multi-agent-review surfaced
    /// a false-positive on external lookups (`a = ${?a}foo; b = ${a}`), so the
    /// criterion was tightened with this `is_owner` guard — strictly narrower
    /// than the original path-equality check. Spec amendment deferred to a
    /// follow-up xx.hocon PR (see Phase 6 #3f close-out notes).
    resolving_field_path: Vec<String>,
}

impl<'a> SubstitutionResolver<'a> {
    pub fn new_with_opts(
        root: &'a ResObj,
        env: &'a HashMap<String, String>,
        use_system_environment: bool,
        allow_unresolved: bool,
    ) -> Self {
        SubstitutionResolver {
            root,
            env,
            resolving: HashSet::new(),
            cache: HashMap::new(),
            use_system_environment,
            allow_unresolved,
            resolving_field_path: Vec::new(),
        }
    }

    pub fn resolve(&mut self) -> Result<HoconValue, ResolveError> {
        // `self.root: &'a ResObj` is already a shared reference with lifetime
        // 'a; copying the reference value (not cloning the underlying ResObj)
        // is enough to decouple the read of self.root from the &mut self
        // borrow that resolve_res_obj acquires. See issue #47.
        let root: &ResObj = self.root;
        self.resolve_res_obj(root)
    }

    fn resolve_res_obj(&mut self, obj: &ResObj) -> Result<HoconValue, ResolveError> {
        let mut result = IndexMap::new();
        for (key, val) in &obj.fields {
            self.resolving_field_path.push(key.clone());
            let resolved_result = self.resolve_val(val, obj);
            self.resolving_field_path.pop();
            match resolved_result? {
                Some(resolved) => {
                    // Delayed merge: if both current and prior resolve to objects, deep merge
                    if let HoconValue::Object(ref current_fields) = resolved {
                        if let Some(prior) = obj.prior_values.get(key) {
                            self.resolving_field_path.push(key.clone());
                            let prior_result = self.resolve_val(prior, obj);
                            self.resolving_field_path.pop();
                            if let Some(HoconValue::Object(prior_fields)) = prior_result? {
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
                        self.resolving_field_path.push(key.clone());
                        let prior_result = self.resolve_val(prior, obj);
                        self.resolving_field_path.pop();
                        if let Some(prior_resolved) = prior_result? {
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
            ResolverValue::Concat(c) => {
                self.resolve_concat(&c.nodes, &c.separator_flags, c.line, c.col, scope)
            }
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
            //
            // #120 cross-impl with go.hocon's containsSubstByIdentity (PR #123):
            // the outer wrapping type guard widens from Subst/Concat-only to all
            // ResolverValue variants so substitutions embedded as array elements
            // (`a = [${a}, "x"]`) or object field values
            // (`o = { history = ${o}, ... }`) are also detected as self-references.
            // Pre-#120 these patterns silently produced wrong values (object) or
            // crashed (array element) because the outer `matches!` excluded them
            // from the self-ref short-circuit.
            {
                // Guard: the self-ref short-circuit only fires when the field currently
                // being assigned IS the field that the substitution points at.
                // Without this guard, resolving `b = ${a}` would see that `a`'s value
                // is `${?a}foo` (a self-referential concat) and mis-fire the short-circuit,
                // returning an error instead of resolving `a` normally and giving `b = "foo"`.
                //
                // `resolving_field_path` holds the leaf keys pushed by `resolve_res_obj`
                // as it recurses into nested objects, giving the full path of the field
                // being assigned (e.g. `["foo", "a"]` while resolving `foo.a = …`).
                // We compare by text so quoting differences don't cause false negatives.
                // is_owner: the substitution path is an ANCESTOR of (or equal to)
                // the field currently being assigned. Pre-#120 this was a strict
                // length-equality check, which excluded the case where
                // resolving_field_path is deeper than s.segments — e.g. inside
                // `o = { history = ${o} }`, resolving "o.history" with rfp=["o","history"]
                // but s.segments=["o"]. The strict check returned false, so the
                // outer resolve_subst fell through to resolve_val on the looked-up
                // value (which then re-resolved the inner ${o} via the cycle path,
                // producing a wrong outer wrapping). Prefix-match widens this to:
                // s.segments matches rfp[0..s.segments.len()], i.e. s points at
                // an ancestor of the current field. Preserves the original
                // false-positive guard from Phase 6 #3f because path-equality on
                // the prefix is still required.
                let is_owner = self.resolving_field_path.len() >= s.segments.len()
                    && self
                        .resolving_field_path
                        .iter()
                        .zip(s.segments.iter())
                        .all(|(p, seg)| p == &seg.text);
                let is_self_ref =
                    is_owner && super::fold_self_ref::contains_subst_by_path(&found, &s.segments);
                if is_self_ref {
                    let root_seg = s.segments.first().map(|s| s.text.as_str()).unwrap_or("");
                    let prior_root = scope
                        .prior_values
                        .get(root_seg)
                        .or_else(|| self.root.prior_values.get(root_seg))
                        .cloned();
                    if let Some(prior_root) = prior_root {
                        // For multi-segment paths (e.g. foo.a), navigate into the prior
                        // root object to find the value at the full path.
                        let prior = if s.segments.len() > 1 {
                            if let ResolverValue::Obj(ref prior_obj) = prior_root {
                                lookup_path(prior_obj, &s.segments[1..]).cloned()
                            } else {
                                None
                            }
                        } else {
                            Some(prior_root)
                        };
                        if let Some(prior) = prior {
                            let result = self.resolve_val(&prior, scope)?;
                            if let Some(ref r) = result {
                                self.cache.insert(key.to_string(), r.clone());
                            }
                            return Ok(result);
                        }
                        // Prior root exists but nested path not found — fall through to
                        // no-prior short-circuit below.
                    }
                    // Object-literal form fallback: when `foo { a = "x"; a = ${?foo.a}bar }`
                    // is used, the prior for the leaf key `a` is stored directly in the
                    // current scope (inner_obj.prior_values["a"]), not in the root scope
                    // under the parent key "foo".  The root-segment lookup above only
                    // finds the parent object's prior (used for dotted-path reassignments
                    // at root level), so it finds nothing here.
                    //
                    // Guard: only fire this fallback when the substitution is multi-segment
                    // (len > 1) — single-segment priors are already handled above — and the
                    // leaf segment's prior lives directly in `scope`.
                    if s.segments.len() > 1 {
                        let leaf_seg = s.segments.last().map(|seg| seg.text.as_str()).unwrap_or("");
                        if let Some(leaf_prior) = scope.prior_values.get(leaf_seg).cloned() {
                            let result = self.resolve_val(&leaf_prior, scope)?;
                            if let Some(ref r) = result {
                                self.cache.insert(key.to_string(), r.clone());
                            }
                            return Ok(result);
                        }
                    }
                    // Spec L841: no prior + self-ref → optional yields undefined; required errors.
                    if s.optional {
                        // Return None (undefined) — the concat-layer optional-omission rule
                        // (Phase 6 #3b) will omit this from the fold input.
                        // Note: no explicit cache entry is inserted here; undefined is
                        // encoded by absence from the cache (cache stores HoconValue, not
                        // Option<HoconValue>). Re-resolving the same self-ref deterministically
                        // returns None without a cached entry — spec Q2 idempotency is
                        // satisfied structurally.
                        return Ok(None);
                    }
                    return Err(ResolveError {
                        message: format!(
                            "could not resolve substitution: ${{{key}}} (self-referential with no prior value)"
                        ),
                        path: key.to_string(),
                        line: s.line,
                        col: s.col,
                    });
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

        // S14c.2 (rs.hocon#44): config-path fallback for relativized substitutions.
        //
        // When a substitution inside an included file references an ancestor-scope
        // variable that doesn't exist at the relativized path, try the ORIGINAL
        // (non-relativized) path against the merged root. This matches Lightbend's
        // "resolve against the fully merged tree" behaviour — included files see
        // ancestor variables that don't exist at the include's prefix scope.
        //
        // Tried only after the relativized lookup misses, so the relativized path
        // still wins when both exist. Tried BEFORE env-var fallback so config
        // values take precedence over env vars (matching the primary-lookup
        // ordering).
        //
        // Delayed-merge mirror: when the fallback resolves to an `Object` AND the
        // original path is single-segment AND the root has a prior value for that
        // segment, deep-merge prior + current — same rule the primary lookup
        // applies (see lines 295-313 above). Without this, a config like
        // `y = { a = 1 }; y = ${z}; z = { b = 2 }; bar { include "..." }` where
        // the included file does `ref = ${y}` would yield `bar.ref = { b = 2 }`
        // via the fallback while `y` at root would yield `{ a = 1, b = 2 }` —
        // a silent divergence. See Codex review on PR #117 for the reproducer.
        if s.prefix_len > 0 && s.segments.len() > s.prefix_len {
            let original_segments = &s.segments[s.prefix_len..];
            if let Some(fallback_found) = lookup_path(self.root, original_segments).cloned() {
                let mut result = self.resolve_val(&fallback_found, scope)?;

                if original_segments.len() == 1 {
                    if let Some(HoconValue::Object(ref current_fields)) = result {
                        let root_seg = original_segments
                            .first()
                            .map(|s| s.text.as_str())
                            .unwrap_or("");
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
        }

        // S13c: env-var list expansion — `${X[]}` / `${?X[]}`.
        // When list_suffix=true and config lookup missed, delegate entirely to
        // resolve_env_list. The scalar env fallback below is SUPPRESSED (S13c.5).
        if s.list_suffix && self.use_system_environment {
            let result = self.resolve_env_list(s, key)?;
            if let Some(ref r) = result {
                self.cache.insert(key.to_string(), r.clone());
            }
            return Ok(result);
        }

        // Env var fallback — use raw dot-join (no quoting) to match Lightbend behavior.
        // Gated by use_system_environment (E12 T1).
        if self.use_system_environment {
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
        }

        if s.optional {
            return Ok(None);
        }

        // allow_unresolved: keep unresolved mandatory substitutions as Placeholder values
        // (E12 T1). This preserves is_resolved()=false on the result and allows
        // get_*() to return Err("not resolved") rather than Err("key not found").
        if self.allow_unresolved {
            use crate::value::PlaceholderValue;
            return Ok(Some(HoconValue::Placeholder(PlaceholderValue {
                path: key.to_string(),
                optional: false,
            })));
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
    ) -> Result<Option<HoconValue>, ResolveError> {
        let mut resolved: Vec<(HoconValue, bool)> = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            let is_sep = separator_flags.get(i).copied().unwrap_or(false);
            if let Some(v) = self.resolve_val(n, scope)? {
                resolved.push((v, is_sep));
            }
        }

        // All operands collapsed (all optional substitutions undefined):
        // Per HOCON spec § "Optional substitution materialisation in concat contexts",
        // when every operand in a concat resolves to undefined, the entire field
        // is omitted (same rule as a standalone undefined optional substitution).
        // E.g. `a = ${?x}${?y}` with both undefined → `{}` (no `a` key).
        if resolved.is_empty() {
            return Ok(None);
        }
        if resolved.len() == 1 {
            return Ok(Some(resolved.into_iter().next().unwrap().0));
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
                // All operands were separators (unusual): treat as omitted.
                return Ok(None);
            }
            if non_sep.len() == 1 {
                return Ok(Some(non_sep.into_iter().next().unwrap()));
            }

            let mut iter = non_sep.into_iter();
            let first = iter.next().unwrap();
            return iter
                .try_fold(first, |l, r| join_pair(l, r, line, col))
                .map(|v| Ok(Some(v)))?;
        }

        // Scalar-only concat: include separators so whitespace contributes to the result
        // (e.g., "foo bar" where the space token is a separator).
        //
        // Allow-unresolved: if any operand is a Placeholder, the entire concat result
        // is unresolved.  We cannot produce a concrete string because we don't know
        // the actual values yet.  Return a combined Placeholder so that
        // `is_resolved()` stays false and callers get a proper NotResolved error.
        // (dr14: `a = ${x} ${y}` with allow_unresolved=true — both undefined.)
        let has_placeholder = resolved
            .iter()
            .any(|(v, _)| matches!(v, HoconValue::Placeholder(_)));
        if has_placeholder {
            use crate::value::PlaceholderValue;
            // T2 fix: use a sentinel path instead of joining operand paths with `+`.
            // The old `join("+")` approach produced a fake substitution key (e.g.
            // "x+y") that hocon_map_to_res_obj would later try to round-trip back
            // to a SubstPlaceholder, silently corrupting re-resolution.
            //
            // The sentinel "<unresolved-concat>" is detected by hocon_value_to_resolver
            // (in resolver/mod.rs) and passed through as-is rather than reconstructed
            // as a Subst. Re-resolution uses the unresolved_tree preserved by T1 (which
            // retains the real ConcatPlaceholder structure), not this HoconValue marker.
            return Ok(Some(HoconValue::Placeholder(PlaceholderValue {
                path: "<unresolved-concat>".into(),
                optional: false,
            })));
        }
        let s: String = resolved.iter().map(|(v, _)| stringify_value(v)).collect();
        Ok(Some(HoconValue::Scalar(ScalarValue::string(s))))
    }

    fn resolve_append(
        &mut self,
        a: &AppendPlaceholder,
        scope: &ResObj,
    ) -> Result<HoconValue, ResolveError> {
        let existing = self
            .resolve_val(&a.existing, scope)?
            .unwrap_or_else(|| HoconValue::Array(vec![]));

        // E12: under allow_unresolved, if the prior value is itself an unresolved
        // placeholder (e.g. `x = ${missing}\nx += 1`), defer the append by
        // returning a sentinel Placeholder. Mirrors the resolve_concat short-
        // circuit at the top of this file. Without this guard, the S13b.2
        // non-array check below would classify the placeholder as a concrete
        // non-array and throw, violating the E12 deferral contract.
        if self.allow_unresolved && matches!(existing, HoconValue::Placeholder(_)) {
            use crate::value::PlaceholderValue;
            return Ok(HoconValue::Placeholder(PlaceholderValue {
                path: "<unresolved-append>".into(),
                optional: false,
            }));
        }

        let elem = self.resolve_val(&a.elem, scope)?;

        // S13b.2 (HOCON.md L732): `a += b` is sugar for `a = ${?a} [b]`. The
        // prior value must be an array (or absent → empty array fallback). A
        // non-array prior must produce a resolve-time error. Previously the
        // resolver silently wrapped the non-array as a single-element array.
        let mut items: Vec<HoconValue> = match existing {
            HoconValue::Array(arr) => arr,
            other => {
                return Err(ResolveError {
                    message: format!(
                        "'+=' on non-array value: prior value is {} (spec L732)",
                        type_name(&other),
                    ),
                    path: self.resolving_field_path.join("."),
                    line: a.line,
                    col: a.col,
                });
            }
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
                    "array", "object", line, col,
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
                    "object", "array", line, col,
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
        )),

        // Scalar + Array → error per S10.13
        (scalar, HoconValue::Array(_)) => Err(ResolveError::concat_type_mismatch(
            type_name(&scalar),
            "array",
            line,
            col,
        )),

        // Scalar pairs: reject if either side is an Object (S10.13); string-concat otherwise
        (left, right) => {
            if matches!(left, HoconValue::Object(_)) || matches!(right, HoconValue::Object(_)) {
                return Err(ResolveError::concat_type_mismatch(
                    type_name(&left),
                    type_name(&right),
                    line,
                    col,
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
        HoconValue::Placeholder(_) => "placeholder",
        HoconValue::Scalar(sv) => match sv.value_type {
            crate::value::ScalarType::Null => "null",
            crate::value::ScalarType::Boolean => "boolean",
            crate::value::ScalarType::Number => "number",
            crate::value::ScalarType::String => "string",
        },
    }
}

fn stringify_value(v: &HoconValue) -> String {
    match v {
        HoconValue::Scalar(sv) => sv.raw.clone(),
        HoconValue::Array(_) => {
            unreachable!(
                "stringify_value invariant: type-check rejects Array in string-concat per S10.13"
            )
        }
        HoconValue::Object(_) => {
            unreachable!(
                "stringify_value invariant: type-check rejects Object in string-concat per S10.13"
            )
        }
        HoconValue::Placeholder(pv) => {
            // Should not be called on unresolved placeholders in string concat;
            // treat as empty string to avoid panic in allow_unresolved mode.
            format!("${{{}}}", pv.path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::ScalarValue;

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
