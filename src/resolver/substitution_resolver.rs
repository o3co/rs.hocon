use crate::error::ResolveError;
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
                    ResolverValue::Subst(sub) => {
                        sub.segments.len() == s.segments.len()
                            && sub.segments.iter().zip(s.segments.iter()).all(|(a, b)| a.text == b.text)
                    }
                    ResolverValue::Concat(c) => c.nodes.iter().any(|n| {
                        matches!(n, ResolverValue::Subst(sub)
                            if sub.segments.len() == s.segments.len()
                                && sub.segments.iter().zip(s.segments.iter()).all(|(a, b)| a.text == b.text))
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
        let env_key = s.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(".");
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

        // Object concatenation — only skip parser-synthesized separators (not user-authored strings).
        if resolved
            .iter()
            .all(|(v, is_sep)| matches!(v, HoconValue::Object(_)) || *is_sep)
            && resolved
                .iter()
                .any(|(v, _)| matches!(v, HoconValue::Object(_)))
        {
            let mut merged = IndexMap::new();
            for (v, is_sep) in resolved {
                if is_sep {
                    continue; // skip parser-synthesized separator whitespace
                }
                if let HoconValue::Object(fields) = v {
                    for (k, val) in fields {
                        if let (
                            Some(HoconValue::Object(existing)),
                            HoconValue::Object(new_fields),
                        ) = (merged.get(&k).cloned(), &val)
                        {
                            merged
                                .insert(k, deep_merge_hocon_objects(existing, new_fields.clone()));
                        } else {
                            merged.insert(k, val);
                        }
                    }
                }
            }
            return Ok(HoconValue::Object(merged));
        }

        // Array concatenation
        if resolved
            .iter()
            .any(|(v, _)| matches!(v, HoconValue::Array(_)))
        {
            let mut items = Vec::new();
            for (v, _) in resolved {
                match v {
                    HoconValue::Array(arr) => items.extend(arr),
                    other => items.push(other),
                }
            }
            return Ok(HoconValue::Array(items));
        }

        // String concatenation
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

fn stringify_value(v: &HoconValue) -> String {
    match v {
        HoconValue::Scalar(sv) => sv.raw.clone(),
        HoconValue::Array(_) => format!("{:?}", v),
        HoconValue::Object(_) => format!("{:?}", v),
    }
}
