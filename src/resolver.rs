use std::collections::{HashMap, HashSet};
use indexmap::IndexMap;
use crate::error::ResolveError;
use crate::parser::{AstNode, AstField};
use crate::value::{HoconValue, ScalarValue};

// ---- Internal placeholder types ----

#[derive(Debug, Clone)]
enum ResolverValue {
    Resolved(HoconValue),
    Subst(SubstPlaceholder),
    Concat(ConcatPlaceholder),
    Append(AppendPlaceholder),
    Obj(ResObj),
    UnresolvedArray(Vec<ResolverValue>),
}

#[derive(Debug, Clone)]
struct SubstPlaceholder {
    path: String,
    optional: bool,
    line: usize,
    col: usize,
}

#[derive(Debug, Clone)]
struct ConcatPlaceholder {
    nodes: Vec<ResolverValue>,
}

#[derive(Debug, Clone)]
struct AppendPlaceholder {
    existing: Box<ResolverValue>,
    elem: Box<ResolverValue>,
}

#[derive(Debug, Clone)]
struct ResObj {
    fields: IndexMap<String, ResolverValue>,
    prior_values: IndexMap<String, ResolverValue>,
}

impl ResObj {
    fn new() -> Self {
        ResObj {
            fields: IndexMap::new(),
            prior_values: IndexMap::new(),
        }
    }
}

// ---- Public entry point ----

pub fn resolve(ast: AstNode, env: &HashMap<String, String>) -> Result<HoconValue, ResolveError> {
    let root = build_res_obj(ast)?;
    let mut resolving = HashSet::new();
    let mut cache = HashMap::new();
    resolve_res_obj(&root, &root, &mut resolving, &mut cache, env)
}

// ---- Pass 1: structure building ----

fn build_res_obj(ast: AstNode) -> Result<ResObj, ResolveError> {
    match ast {
        AstNode::Object { fields, .. } => {
            let mut obj = ResObj::new();
            for field in fields {
                apply_field(&mut obj, field)?;
            }
            Ok(obj)
        }
        _ => Err(ResolveError {
            message: "root AST must be an object".into(),
            path: String::new(),
            line: 0,
            col: 0,
        }),
    }
}

fn apply_field(obj: &mut ResObj, field: AstField) -> Result<(), ResolveError> {
    // Include directive
    if field.key.is_empty() {
        if matches!(field.value, AstNode::Include { .. }) {
            // Phase 1 skips includes
            deep_merge_res_obj_into(obj, ResObj::new());
        }
        return Ok(());
    }

    let head = field.key[0].clone();
    let tail: Vec<String> = field.key[1..].to_vec();

    if !tail.is_empty() {
        // Nested key: create synthetic object
        let synthetic = AstNode::Object {
            fields: vec![AstField {
                key: tail,
                value: field.value,
                append: field.append,
                pos: field.pos.clone(),
            }],
            pos: field.pos.clone(),
        };
        return apply_field(obj, AstField {
            key: vec![head],
            value: synthetic,
            append: false,
            pos: field.pos,
        });
    }

    if field.append {
        let existing = obj.fields.get(&head).cloned().unwrap_or_else(|| {
            ResolverValue::Resolved(HoconValue::Array(vec![]))
        });
        obj.prior_values.insert(head.clone(), existing.clone());
        let elem = ast_to_resolver_value(field.value)?;
        obj.fields.insert(head, ResolverValue::Append(AppendPlaceholder {
            existing: Box::new(existing),
            elem: Box::new(elem),
        }));
        return Ok(());
    }

    // Normal assignment
    let existing = obj.fields.get(&head).cloned();
    let new_val = ast_to_resolver_value(field.value)?;

    if let Some(ref ex) = existing {
        obj.prior_values.insert(head.clone(), ex.clone());
    }

    // Deep merge if both are ResObj
    if let (Some(ResolverValue::Obj(_)), ResolverValue::Obj(new_obj)) = (&existing, &new_val) {
        if let Some(ResolverValue::Obj(existing_obj)) = obj.fields.get_mut(&head) {
            deep_merge_res_obj_into(existing_obj, new_obj.clone());
            return Ok(());
        }
    }

    obj.fields.insert(head, new_val);
    Ok(())
}

fn ast_to_resolver_value(ast: AstNode) -> Result<ResolverValue, ResolveError> {
    match ast {
        AstNode::Scalar { value, .. } => Ok(ResolverValue::Resolved(HoconValue::Scalar(value))),
        AstNode::Array { items, .. } => {
            let rv_items: Vec<ResolverValue> = items
                .into_iter()
                .map(ast_to_resolver_value)
                .collect::<Result<_, _>>()?;
            let all_resolved = rv_items.iter().all(|v| matches!(v, ResolverValue::Resolved(_)));
            if all_resolved {
                let hv_items: Vec<HoconValue> = rv_items
                    .into_iter()
                    .map(|v| match v {
                        ResolverValue::Resolved(hv) => hv,
                        _ => unreachable!(),
                    })
                    .collect();
                Ok(ResolverValue::Resolved(HoconValue::Array(hv_items)))
            } else {
                Ok(ResolverValue::UnresolvedArray(rv_items))
            }
        }
        AstNode::Object { .. } => {
            let inner = build_res_obj(ast)?;
            Ok(ResolverValue::Obj(inner))
        }
        AstNode::Substitution { path, optional, pos } => {
            Ok(ResolverValue::Subst(SubstPlaceholder {
                path,
                optional,
                line: pos.line,
                col: pos.col,
            }))
        }
        AstNode::Concat { nodes, .. } => {
            let rv_nodes: Vec<ResolverValue> = nodes
                .into_iter()
                .map(ast_to_resolver_value)
                .collect::<Result<_, _>>()?;
            Ok(ResolverValue::Concat(ConcatPlaceholder { nodes: rv_nodes }))
        }
        AstNode::Include { .. } => {
            Ok(ResolverValue::Resolved(HoconValue::Scalar(ScalarValue::Null)))
        }
    }
}

fn deep_merge_res_obj_into(dst: &mut ResObj, src: ResObj) {
    for (k, src_val) in src.fields {
        if let Some(ResolverValue::Obj(dst_obj)) = dst.fields.get_mut(&k) {
            if let ResolverValue::Obj(src_obj) = src_val {
                deep_merge_res_obj_into(dst_obj, src_obj);
                continue;
            }
        }
        if let Some(old) = dst.fields.get(&k) {
            dst.prior_values.insert(k.clone(), old.clone());
        }
        dst.fields.insert(k, src_val);
    }
}

// ---- Pass 2: substitution resolution ----

fn resolve_res_obj(
    obj: &ResObj,
    root: &ResObj,
    resolving: &mut HashSet<String>,
    cache: &mut HashMap<String, HoconValue>,
    env: &HashMap<String, String>,
) -> Result<HoconValue, ResolveError> {
    let mut result = IndexMap::new();
    for (key, val) in &obj.fields {
        match resolve_val(val, obj, root, resolving, cache, env)? {
            Some(resolved) => { result.insert(key.clone(), resolved); }
            None => {
                // Unresolved optional: fall back to prior value
                if let Some(prior) = obj.prior_values.get(key) {
                    if let Some(prior_resolved) = resolve_val(prior, obj, root, resolving, cache, env)? {
                        result.insert(key.clone(), prior_resolved);
                    }
                }
            }
        }
    }
    Ok(HoconValue::Object(result))
}

fn resolve_val(
    v: &ResolverValue,
    scope: &ResObj,
    root: &ResObj,
    resolving: &mut HashSet<String>,
    cache: &mut HashMap<String, HoconValue>,
    env: &HashMap<String, String>,
) -> Result<Option<HoconValue>, ResolveError> {
    match v {
        ResolverValue::Subst(s) => resolve_subst(s, scope, root, resolving, cache, env),
        ResolverValue::Concat(c) => resolve_concat(&c.nodes, scope, root, resolving, cache, env).map(Some),
        ResolverValue::Append(a) => resolve_append(a, scope, root, resolving, cache, env).map(Some),
        ResolverValue::Obj(o) => resolve_res_obj(o, root, resolving, cache, env).map(Some),
        ResolverValue::UnresolvedArray(items) => {
            let mut resolved_items = Vec::new();
            for item in items {
                let resolved = resolve_val(item, scope, root, resolving, cache, env)?
                    .unwrap_or(HoconValue::Scalar(ScalarValue::Null));
                resolved_items.push(resolved);
            }
            Ok(Some(HoconValue::Array(resolved_items)))
        }
        ResolverValue::Resolved(hv) => Ok(Some(hv.clone())),
    }
}

fn resolve_subst(
    s: &SubstPlaceholder,
    scope: &ResObj,
    root: &ResObj,
    resolving: &mut HashSet<String>,
    cache: &mut HashMap<String, HoconValue>,
    env: &HashMap<String, String>,
) -> Result<Option<HoconValue>, ResolveError> {
    if let Some(cached) = cache.get(&s.path) {
        return Ok(Some(cached.clone()));
    }

    if resolving.contains(&s.path) {
        // Cycle detected: try prior value for self-referential substitutions
        let segments = parse_subst_path(&s.path);
        let root_seg = segments.first().map(|s| s.as_str()).unwrap_or("");
        let prior = scope.prior_values.get(root_seg)
            .or_else(|| root.prior_values.get(root_seg));
        if let Some(prior) = prior {
            let mut fresh_resolving = resolving.clone();
            return resolve_val(prior, scope, root, &mut fresh_resolving, cache, env);
        }
        if s.optional {
            return Ok(None);
        }
        return Err(ResolveError {
            message: format!("circular substitution: {}", s.path),
            path: s.path.clone(),
            line: s.line,
            col: s.col,
        });
    }

    resolving.insert(s.path.clone());

    let result = (|| -> Result<Option<HoconValue>, ResolveError> {
        let segments = parse_subst_path(&s.path);
        let found = lookup_path(root, &segments);

        if let Some(found) = found {
            // If found value is a subst/concat pointing at itself, use prior value
            if matches!(found, ResolverValue::Subst(_) | ResolverValue::Concat(_)) {
                let root_seg = segments.first().map(|s| s.as_str()).unwrap_or("");
                let prior = scope.prior_values.get(root_seg)
                    .or_else(|| root.prior_values.get(root_seg));
                if let Some(prior) = prior {
                    let result = resolve_val(prior, scope, root, resolving, cache, env)?;
                    if let Some(ref r) = result {
                        cache.insert(s.path.clone(), r.clone());
                    }
                    return Ok(result);
                }
            }
            let result = resolve_val(found, scope, root, resolving, cache, env)?;
            if let Some(ref r) = result {
                cache.insert(s.path.clone(), r.clone());
            }
            return Ok(result);
        }

        // Env var fallback
        if let Some(env_val) = env.get(&s.path) {
            let result = HoconValue::Scalar(ScalarValue::String(env_val.clone()));
            cache.insert(s.path.clone(), result.clone());
            return Ok(Some(result));
        }

        if s.optional {
            return Ok(None);
        }

        Err(ResolveError {
            message: format!("could not resolve substitution: ${{{}}}", s.path),
            path: s.path.clone(),
            line: s.line,
            col: s.col,
        })
    })();

    resolving.remove(&s.path);
    result
}

fn resolve_concat(
    nodes: &[ResolverValue],
    scope: &ResObj,
    root: &ResObj,
    resolving: &mut HashSet<String>,
    cache: &mut HashMap<String, HoconValue>,
    env: &HashMap<String, String>,
) -> Result<HoconValue, ResolveError> {
    let mut resolved = Vec::new();
    for n in nodes {
        if let Some(v) = resolve_val(n, scope, root, resolving, cache, env)? {
            resolved.push(v);
        }
    }

    if resolved.is_empty() {
        return Ok(HoconValue::Scalar(ScalarValue::Null));
    }
    if resolved.len() == 1 {
        return Ok(resolved.into_iter().next().unwrap());
    }

    // Object concatenation
    if resolved.iter().all(|v| matches!(v, HoconValue::Object(_))) {
        let mut merged = IndexMap::new();
        for v in resolved {
            if let HoconValue::Object(fields) = v {
                for (k, val) in fields {
                    merged.insert(k, val);
                }
            }
        }
        return Ok(HoconValue::Object(merged));
    }

    // Array concatenation
    if resolved.iter().any(|v| matches!(v, HoconValue::Array(_))) {
        let mut items = Vec::new();
        for v in resolved {
            match v {
                HoconValue::Array(arr) => items.extend(arr),
                other => items.push(other),
            }
        }
        return Ok(HoconValue::Array(items));
    }

    // String concatenation
    let s: String = resolved.iter().map(|v| stringify_value(v)).collect();
    Ok(HoconValue::Scalar(ScalarValue::String(s)))
}

fn resolve_append(
    a: &AppendPlaceholder,
    scope: &ResObj,
    root: &ResObj,
    resolving: &mut HashSet<String>,
    cache: &mut HashMap<String, HoconValue>,
    env: &HashMap<String, String>,
) -> Result<HoconValue, ResolveError> {
    let existing = resolve_val(&a.existing, scope, root, resolving, cache, env)?
        .unwrap_or_else(|| HoconValue::Array(vec![]));
    let elem = resolve_val(&a.elem, scope, root, resolving, cache, env)?;

    let mut items: Vec<HoconValue> = match existing {
        HoconValue::Array(arr) => arr,
        other => vec![other],
    };
    if let Some(e) = elem {
        items.push(e);
    }
    Ok(HoconValue::Array(items))
}

fn stringify_value(v: &HoconValue) -> String {
    match v {
        HoconValue::Scalar(s) => match s {
            ScalarValue::String(s) => s.clone(),
            ScalarValue::Int(n) => n.to_string(),
            ScalarValue::Float(f) => f.to_string(),
            ScalarValue::Bool(b) => b.to_string(),
            ScalarValue::Null => "null".to_string(),
        },
        HoconValue::Array(_) => format!("{:?}", v),
        HoconValue::Object(_) => format!("{:?}", v),
    }
}

fn lookup_path<'a>(root: &'a ResObj, segments: &[String]) -> Option<&'a ResolverValue> {
    if segments.is_empty() {
        return None;
    }
    let head = &segments[0];
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

fn parse_subst_path(raw: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        if chars[i] == '"' {
            i += 1;
            let mut seg = String::new();
            while i < chars.len() && chars[i] != '"' {
                seg.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            segments.push(seg);
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                i += 1;
            }
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        } else if chars[i] == '.' {
            segments.push(String::new());
            i += 1;
        } else {
            let mut seg = String::new();
            while i < chars.len() && chars[i] != '.' {
                seg.push(chars[i]);
                i += 1;
            }
            segments.push(seg.trim().to_string());
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::parse_tokens;
    use crate::value::{HoconValue, ScalarValue};

    fn resolve_str(input: &str) -> HoconValue {
        resolve_str_with_env(input, &HashMap::new())
    }

    fn resolve_str_with_env(input: &str, env: &HashMap<String, String>) -> HoconValue {
        let tokens = tokenize(input).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        resolve(ast, env).unwrap()
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
        assert_eq!(obj(&v).get("host"), Some(&HoconValue::Scalar(ScalarValue::String("localhost".into()))));
    }

    #[test]
    fn resolves_number() {
        let v = resolve_str("port = 8080");
        assert_eq!(obj(&v).get("port"), Some(&HoconValue::Scalar(ScalarValue::Int(8080))));
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
        assert_eq!(obj(&v).get("x"), Some(&HoconValue::Scalar(ScalarValue::Int(2))));
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
        assert_eq!(obj(&v).get("url"), Some(&HoconValue::Scalar(ScalarValue::String("localhost".into()))));
    }

    #[test]
    fn resolves_nested_path_substitution() {
        let v = resolve_str("server { host = \"x\" }\nhost = ${server.host}");
        assert_eq!(obj(&v).get("host"), Some(&HoconValue::Scalar(ScalarValue::String("x".into()))));
    }

    #[test]
    fn resolves_optional_substitution_exists() {
        let v = resolve_str("a = 1\nb = ${?a}");
        assert_eq!(obj(&v).get("b"), Some(&HoconValue::Scalar(ScalarValue::Int(1))));
    }

    #[test]
    fn drops_field_for_optional_missing() {
        let v = resolve_str("b = ${?missing}");
        assert_eq!(obj(&v).get("b"), None);
    }

    #[test]
    fn falls_back_to_prior_value() {
        let v = resolve_str("port = 50051\nport = ${?GRPC_PORT}");
        assert_eq!(obj(&v).get("port"), Some(&HoconValue::Scalar(ScalarValue::Int(50051))));
    }

    #[test]
    fn uses_env_var_when_present() {
        let mut env = HashMap::new();
        env.insert("GRPC_PORT".into(), "9090".into());
        let v = resolve_str_with_env("port = 50051\nport = ${?GRPC_PORT}", &env);
        assert_eq!(obj(&v).get("port"), Some(&HoconValue::Scalar(ScalarValue::String("9090".into()))));
    }

    #[test]
    fn throws_on_unresolved_mandatory() {
        let tokens = tokenize("b = ${missing}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &HashMap::new()).is_err());
    }

    #[test]
    fn resolves_env_var_fallback() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".into(), "hello".into());
        let v = resolve_str_with_env("b = ${MY_VAR}", &env);
        assert_eq!(obj(&v).get("b"), Some(&HoconValue::Scalar(ScalarValue::String("hello".into()))));
    }

    #[test]
    fn resolves_self_referential_substitution() {
        let v = resolve_str("path = \"/usr\"\npath = ${path}:/extra");
        if let Some(HoconValue::Scalar(ScalarValue::String(s))) = obj(&v).get("path") {
            assert!(s.contains("/usr"));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn resolves_string_concat_with_substitution() {
        let v = resolve_str("host = \"localhost\"\nurl = \"http://\"${host}");
        assert_eq!(obj(&v).get("url"), Some(&HoconValue::Scalar(ScalarValue::String("http://localhost".into()))));
    }

    #[test]
    fn throws_on_circular_substitution() {
        let tokens = tokenize("a = ${b}\nb = ${a}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &HashMap::new()).is_err());
    }

    #[test]
    fn resolves_forward_reference() {
        let v = resolve_str("url = ${host}\nhost = \"localhost\"");
        assert_eq!(obj(&v).get("url"), Some(&HoconValue::Scalar(ScalarValue::String("localhost".into()))));
    }
}
