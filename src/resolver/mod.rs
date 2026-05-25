mod fold_self_ref;
mod include_loader;
mod structure_builder;
mod substitution_resolver;
pub mod types;
mod utils;

use crate::error::ResolveError;
use crate::parser::AstNode;
use crate::value::HoconValue;

pub use types::ResolverValue;
pub use types::{InternalResolveOptions, ResObj};

use structure_builder::StructureBuilder;
use substitution_resolver::SubstitutionResolver;

// ---- Public entry point (backward compat) ----

pub fn resolve(ast: AstNode, opts: &InternalResolveOptions) -> Result<HoconValue, ResolveError> {
    let root = build_tree(ast, opts)?;
    resolve_tree(root, opts)
}

/// Phase 1: build unresolved ResObj tree (includes expanded, substitutions left as placeholders).
pub fn build_tree(ast: AstNode, opts: &InternalResolveOptions) -> Result<ResObj, ResolveError> {
    StructureBuilder::new(opts).build(ast, &[])
}

/// Phase 2: resolve substitution/concat placeholders in tree.
pub fn resolve_tree(
    tree: ResObj,
    opts: &InternalResolveOptions,
) -> Result<HoconValue, ResolveError> {
    SubstitutionResolver::new_with_opts(
        &tree,
        &opts.env,
        opts.use_system_environment,
        opts.allow_unresolved,
    )
    .resolve()
}

/// Returns true if obj or any nested ResObj contains an unresolved Subst or Concat placeholder.
pub fn contains_placeholders(obj: &ResObj) -> bool {
    obj.fields.values().any(rv_has_placeholder)
}

fn rv_has_placeholder(v: &types::ResolverValue) -> bool {
    use types::ResolverValue;
    match v {
        ResolverValue::Subst(_) | ResolverValue::Concat(_) => true,
        ResolverValue::Obj(inner) => contains_placeholders(inner),
        ResolverValue::UnresolvedArray(items) => items.iter().any(rv_has_placeholder),
        ResolverValue::Append(a) => rv_has_placeholder(&a.existing) || rv_has_placeholder(&a.elem),
        ResolverValue::Resolved(_) => false,
    }
}

/// Binary merge primitive for E12 WithFallback on unresolved trees.
/// Receiver's keys win; on non-Obj collision, fallback's value is stored as
/// prior_values[key] for cross-layer self-reference lookback in phase 2.
/// Both-Obj collisions recurse (deep merge). Fallback-only keys are included.
///
/// The caller (Config::with_fallback in T8) is responsible for the
/// composition-barrier semantic (HOCON.md L1485): once a non-object value
/// has won at a path across WithFallback chain iterations, subsequent
/// fallback objects at that path must not contribute. merge_unresolved
/// itself is a binary primitive and does not track barriers across calls.
pub fn merge_unresolved(receiver: ResObj, fallback: ResObj) -> ResObj {
    use types::ResolverValue;
    // Seed result with fallback (so fallback-only keys exist).
    let mut result = fallback;
    // Extract receiver fields and priors.
    let mut recv_fields = receiver.fields;
    let mut recv_priors = receiver.prior_values;

    // Apply receiver: receiver keys win.
    for (k, rv) in recv_fields.drain(..) {
        if let Some(existing) = result.fields.get(&k) {
            // Both Obj → recurse, UNLESS there is a composition barrier.
            // Composition barrier (HOCON.md L1485): if receiver's prior_values[k]
            // is a non-Obj value, a scalar has already won at this path in the
            // receiver-side chain. Subsequent fallback objects MUST NOT be merged —
            // treat the collision as non-Obj (receiver object wins; fallback object
            // is discarded into prior rather than merged).
            let has_barrier = recv_priors
                .get(&k)
                .map(|p| !matches!(p, ResolverValue::Obj(_)))
                .unwrap_or(false);

            if let (ResolverValue::Obj(_), ResolverValue::Obj(_)) = (&rv, existing) {
                if !has_barrier {
                    let rec_obj = match rv {
                        ResolverValue::Obj(o) => o,
                        _ => unreachable!(),
                    };
                    let fb_obj = match result.fields.shift_remove(&k).unwrap() {
                        ResolverValue::Obj(o) => o,
                        _ => unreachable!(),
                    };
                    result.fields.insert(
                        k.clone(),
                        ResolverValue::Obj(merge_unresolved(rec_obj, fb_obj)),
                    );
                    // Carry receiver's own prior for this key if it had one.
                    if let Some(rp) = recv_priors.shift_remove(&k) {
                        result.prior_values.insert(k, rp);
                    }
                    continue;
                }
                // Barrier: both are Obj, but receiver's prior is non-Obj.
                // Receiver's Obj wins; fallback Obj is NOT merged.
                let prior = result.fields.insert(k.clone(), rv).unwrap();
                result.prior_values.entry(k.clone()).or_insert(prior);
            } else {
                // Non-obj collision: receiver wins; capture existing (fallback's value) as prior.
                let prior = result.fields.insert(k.clone(), rv).unwrap(); // replace, get old
                                                                          // prior = the fallback value we just displaced
                result.prior_values.entry(k.clone()).or_insert(prior);
            }
        } else {
            result.fields.insert(k.clone(), rv);
        }
        // Carry receiver's own prior for this key (receiver history wins).
        if let Some(rp) = recv_priors.shift_remove(&k) {
            result.prior_values.insert(k, rp);
        }
    }
    result
}

/// Returns true if `obj` or any nested ResObj contains prior_values entries.
/// Used by `Config::new_from_res_obj` to determine whether to keep the
/// unresolved_tree for composition-barrier tracking across future with_fallback calls.
pub(crate) fn res_obj_has_priors(obj: &ResObj) -> bool {
    if !obj.prior_values.is_empty() {
        return true;
    }
    obj.fields.values().any(|v| match v {
        types::ResolverValue::Obj(inner) => res_obj_has_priors(inner),
        _ => false,
    })
}

/// Walk a `HoconValue` map and return `true` if any value is a placeholder.
pub(crate) fn contains_placeholders_in_hocon_map(
    map: &indexmap::IndexMap<String, crate::value::HoconValue>,
) -> bool {
    map.values().any(hocon_value_has_placeholder)
}

fn hocon_value_has_placeholder(v: &crate::value::HoconValue) -> bool {
    use crate::value::HoconValue;
    match v {
        HoconValue::Placeholder(_) => true,
        HoconValue::Object(inner) => contains_placeholders_in_hocon_map(inner),
        HoconValue::Array(items) => items.iter().any(hocon_value_has_placeholder),
        HoconValue::Scalar(_) => false,
    }
}

/// Convert a post-phase-1 `ResObj` to `IndexMap<String, HoconValue>` using
/// `HoconValue::Placeholder` for unresolved nodes.
pub(crate) fn res_obj_to_hocon_partial(
    obj: &ResObj,
) -> indexmap::IndexMap<String, crate::value::HoconValue> {
    obj.fields
        .iter()
        .map(|(k, v)| (k.clone(), resolver_value_to_hocon(v)))
        .collect()
}

fn resolver_value_to_hocon(v: &types::ResolverValue) -> crate::value::HoconValue {
    use crate::value::{HoconValue, PlaceholderValue};
    use types::ResolverValue;
    match v {
        ResolverValue::Resolved(hv) => hv.clone(),
        ResolverValue::Subst(s) => {
            let path = s
                .segments
                .iter()
                .map(|seg| seg.text.as_str())
                .collect::<Vec<_>>()
                .join(".");
            HoconValue::Placeholder(PlaceholderValue {
                path,
                optional: s.optional,
            })
        }
        ResolverValue::Concat(_) => HoconValue::Placeholder(PlaceholderValue {
            path: "<concat>".into(),
            optional: false,
        }),
        ResolverValue::Obj(inner) => HoconValue::Object(res_obj_to_hocon_partial(inner)),
        ResolverValue::UnresolvedArray(items) => {
            HoconValue::Array(items.iter().map(resolver_value_to_hocon).collect())
        }
        ResolverValue::Append(_) => HoconValue::Placeholder(PlaceholderValue {
            path: "<append>".into(),
            optional: false,
        }),
    }
}

/// Convert a `HoconValue` map back to a `ResObj` (inverse of `res_obj_to_hocon_partial`).
pub(crate) fn hocon_map_to_res_obj(
    map: &indexmap::IndexMap<String, crate::value::HoconValue>,
) -> ResObj {
    let mut obj = ResObj::new();
    for (k, v) in map {
        obj.fields.insert(k.clone(), hocon_value_to_resolver(v));
    }
    obj
}

fn hocon_value_to_resolver(v: &crate::value::HoconValue) -> types::ResolverValue {
    use crate::value::HoconValue;
    use types::{ResolverValue, SubstPlaceholder};
    match v {
        HoconValue::Placeholder(pv) => {
            // T2 fix: sentinel paths (those beginning with '<') are internal markers
            // produced by resolver_value_to_hocon for Concat/Append/unresolved-concat
            // placeholders. They must NOT be reconstructed as substitution keys — doing
            // so would silently produce a bogus lookup like "${<unresolved-concat>}".
            // Pass them through as Resolved(Placeholder) so the re-resolution path
            // (driven by the unresolved_tree preserved by T1) handles them correctly.
            if pv.path.starts_with('<') {
                return ResolverValue::Resolved(v.clone());
            }
            // Normal substitution placeholder: reconstruct a Subst from the path string.
            use crate::lexer::Segment;
            let segments: Vec<Segment> = pv
                .path
                .split('.')
                .map(|part| Segment {
                    text: part.to_owned(),
                    line: 0,
                    col: 0,
                })
                .collect();
            ResolverValue::Subst(SubstPlaceholder {
                segments,
                optional: pv.optional,
                known_absent: false,
                list_suffix: false,
                line: 0,
                col: 0,
                prefix_len: 0,
            })
        }
        HoconValue::Object(inner) => ResolverValue::Obj(hocon_map_to_res_obj(inner)),
        HoconValue::Array(items) => {
            ResolverValue::UnresolvedArray(items.iter().map(hocon_value_to_resolver).collect())
        }
        HoconValue::Scalar(_) => ResolverValue::Resolved(v.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::parse_tokens;
    use crate::value::{HoconValue, ScalarValue};
    use indexmap::IndexMap;
    use std::collections::HashMap;

    fn resolve_str(input: &str) -> HoconValue {
        resolve_str_with_env(input, &HashMap::new())
    }

    fn resolve_str_with_env(input: &str, env: &HashMap<String, String>) -> HoconValue {
        let tokens = tokenize(input).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        resolve(ast, &InternalResolveOptions::new(env.clone())).unwrap()
    }

    fn obj(v: &HoconValue) -> &IndexMap<String, HoconValue> {
        match v {
            HoconValue::Object(m) => m,
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn resolves_simple_string() {
        let v = resolve_str("host = \"localhost\"");
        assert_eq!(
            obj(&v).get("host"),
            Some(&HoconValue::Scalar(ScalarValue::string("localhost".into())))
        );
    }

    #[test]
    fn resolves_number() {
        let v = resolve_str("port = 8080");
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::number("8080".into())))
        );
    }

    #[test]
    fn resolves_nested_objects() {
        let v = resolve_str("server { host = \"localhost\" }");
        assert!(matches!(obj(&v).get("server"), Some(HoconValue::Object(_))));
    }

    #[test]
    fn merges_duplicate_object_keys() {
        let v = resolve_str("server { host = \"a\" }\nserver { port = 8080 }");
        if let Some(HoconValue::Object(server)) = obj(&v).get("server") {
            assert!(server.contains_key("host"));
            assert!(server.contains_key("port"));
        } else {
            panic!("expected server object");
        }
    }

    #[test]
    fn last_value_wins_for_scalars() {
        let v = resolve_str("x = 1\nx = 2");
        assert_eq!(
            obj(&v).get("x"),
            Some(&HoconValue::Scalar(ScalarValue::number("2".into())))
        );
    }

    #[test]
    fn resolves_arrays() {
        let v = resolve_str("list = [1, 2, 3]");
        if let Some(HoconValue::Array(items)) = obj(&v).get("list") {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn handles_plus_equals_on_existing_array() {
        let v = resolve_str("list = [1, 2]\nlist += 3");
        if let Some(HoconValue::Array(items)) = obj(&v).get("list") {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn handles_plus_equals_on_missing_key() {
        let v = resolve_str("list += 1");
        if let Some(HoconValue::Array(items)) = obj(&v).get("list") {
            assert_eq!(items.len(), 1);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn preserves_key_order() {
        let v = resolve_str("c = 3\na = 1\nb = 2");
        let keys: Vec<&String> = obj(&v).keys().collect();
        assert_eq!(keys, vec!["c", "a", "b"]);
    }

    #[test]
    fn resolves_substitution() {
        let v = resolve_str("host = \"localhost\"\nurl = ${host}");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::string("localhost".into())))
        );
    }

    #[test]
    fn resolves_nested_path_substitution() {
        let v = resolve_str("server { host = \"x\" }\nhost = ${server.host}");
        assert_eq!(
            obj(&v).get("host"),
            Some(&HoconValue::Scalar(ScalarValue::string("x".into())))
        );
    }

    #[test]
    fn resolves_optional_substitution_exists() {
        let v = resolve_str("a = 1\nb = ${?a}");
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::number("1".into())))
        );
    }

    #[test]
    fn drops_field_for_optional_missing() {
        let v = resolve_str("b = ${?missing}");
        assert_eq!(obj(&v).get("b"), None);
    }

    #[test]
    fn falls_back_to_prior_value() {
        let v = resolve_str("port = 50051\nport = ${?GRPC_PORT}");
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::number("50051".into())))
        );
    }

    #[test]
    fn uses_env_var_when_present() {
        let mut env = HashMap::new();
        env.insert("GRPC_PORT".into(), "9090".into());
        let v = resolve_str_with_env("port = 50051\nport = ${?GRPC_PORT}", &env);
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::string("9090".into())))
        );
    }

    #[test]
    fn throws_on_unresolved_mandatory() {
        let tokens = tokenize("b = ${missing}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &InternalResolveOptions::new(HashMap::new())).is_err());
    }

    #[test]
    fn resolves_env_var_fallback() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".into(), "hello".into());
        let v = resolve_str_with_env("b = ${MY_VAR}", &env);
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::string("hello".into())))
        );
    }

    #[test]
    fn resolves_self_referential_substitution() {
        let v = resolve_str("path = \"/usr\"\npath = ${path}:/extra");
        if let Some(HoconValue::Scalar(sv)) = obj(&v).get("path") {
            assert!(sv.raw.contains("/usr"));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn resolves_last_assignment_wins_for_substitution() {
        // b=${x} then b=${y} — ${b} should resolve to y's value (5), not x's ({q:10})
        let v = resolve_str("x={q:10}\ny=5\nb=${x}\nb=${y}");
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::number("5".into())))
        );
    }

    #[test]
    fn resolves_string_concat_with_substitution() {
        let v = resolve_str("host = \"localhost\"\nurl = \"http://\"${host}");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::string(
                "http://localhost".into()
            )))
        );
    }

    #[test]
    fn throws_on_circular_substitution() {
        let tokens = tokenize("a = ${b}\nb = ${a}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &InternalResolveOptions::new(HashMap::new())).is_err());
    }

    #[test]
    fn resolves_forward_reference() {
        let v = resolve_str("url = ${host}\nhost = \"localhost\"");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::string("localhost".into())))
        );
    }

    #[test]
    fn delayed_merge_object_with_substitution() {
        // a=${x} then a={c:3} should deep merge: {q:10, c:3}
        let v = resolve_str("x={q:10}\na=${x}\na={c:3}");
        let a = obj(&v).get("a").cloned().unwrap();
        match a {
            HoconValue::Object(map) => {
                assert_eq!(
                    map.get("c"),
                    Some(&HoconValue::Scalar(ScalarValue::number("3".into())))
                );
                assert_eq!(
                    map.get("q"),
                    Some(&HoconValue::Scalar(ScalarValue::number("10".into())))
                );
            }
            other => panic!("expected object, got {:?}", other),
        }
    }
}
