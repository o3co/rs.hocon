use crate::error::ResolveError;
use crate::parser::{AstField, AstNode};
use crate::value::{HoconValue, ScalarValue};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub struct ResolveOptions {
    pub env: HashMap<String, String>,
    pub base_dir: Option<PathBuf>,
    pub include_stack: Vec<PathBuf>,
}

impl ResolveOptions {
    pub fn new(env: HashMap<String, String>) -> Self {
        ResolveOptions {
            env,
            base_dir: None,
            include_stack: Vec::new(),
        }
    }

    pub fn with_base_dir(mut self, base_dir: PathBuf) -> Self {
        self.base_dir = Some(base_dir);
        self
    }
}

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
    prefix_len: usize,
}

#[derive(Debug, Clone)]
struct ConcatPlaceholder {
    nodes: Vec<ResolverValue>,
    /// Parallel array: true if the corresponding node is a parser-synthesized separator.
    separator_flags: Vec<bool>,
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

pub fn resolve(ast: AstNode, opts: &ResolveOptions) -> Result<HoconValue, ResolveError> {
    let root = build_res_obj(ast, opts, &[])?;
    let mut resolving = HashSet::new();
    let mut cache = HashMap::new();
    resolve_res_obj(&root, &root, &mut resolving, &mut cache, &opts.env)
}

// ---- Pass 1: structure building ----

fn build_res_obj(
    ast: AstNode,
    opts: &ResolveOptions,
    path_prefix: &[String],
) -> Result<ResObj, ResolveError> {
    match ast {
        AstNode::Object { fields, .. } => {
            let mut obj = ResObj::new();
            for field in fields {
                apply_field(&mut obj, field, opts, path_prefix)?;
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

fn apply_field(
    obj: &mut ResObj,
    field: AstField,
    opts: &ResolveOptions,
    path_prefix: &[String],
) -> Result<(), ResolveError> {
    // Include directive
    if field.key.is_empty() {
        if let AstNode::Include {
            path: include_path,
            required,
            pos,
        } = &field.value
        {
            let mut included = load_include(
                include_path,
                *required,
                pos.line,
                pos.col,
                opts,
                path_prefix,
            )?;
            if !path_prefix.is_empty() {
                let prefix_str = path_prefix.join(".");
                relativize_res_obj(&mut included, &prefix_str, path_prefix.len());
            }
            deep_merge_res_obj_into(obj, included);
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
        return apply_field(
            obj,
            AstField {
                key: vec![head],
                value: synthetic,
                append: false,
                pos: field.pos,
            },
            opts,
            path_prefix,
        );
    }

    if field.append {
        let existing = obj
            .fields
            .get(&head)
            .cloned()
            .unwrap_or_else(|| ResolverValue::Resolved(HoconValue::Array(vec![])));
        obj.prior_values.insert(head.clone(), existing.clone());
        let mut child_prefix = path_prefix.to_vec();
        child_prefix.push(head.clone());
        let elem = ast_to_resolver_value(field.value, opts, &child_prefix)?;
        obj.fields.insert(
            head,
            ResolverValue::Append(AppendPlaceholder {
                existing: Box::new(existing),
                elem: Box::new(elem),
            }),
        );
        return Ok(());
    }

    // Normal assignment
    let existing = obj.fields.get(&head).cloned();
    let mut child_prefix = path_prefix.to_vec();
    child_prefix.push(head.clone());
    let new_val = ast_to_resolver_value(field.value, opts, &child_prefix)?;

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

fn ast_to_resolver_value(
    ast: AstNode,
    opts: &ResolveOptions,
    path_prefix: &[String],
) -> Result<ResolverValue, ResolveError> {
    match ast {
        AstNode::Scalar { value, .. } => Ok(ResolverValue::Resolved(HoconValue::Scalar(value))),
        AstNode::Array { items, .. } => {
            let rv_items: Vec<ResolverValue> = items
                .into_iter()
                .map(|item| ast_to_resolver_value(item, opts, path_prefix))
                .collect::<Result<_, _>>()?;
            let all_resolved = rv_items
                .iter()
                .all(|v| matches!(v, ResolverValue::Resolved(_)));
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
            let inner = build_res_obj(ast, opts, path_prefix)?;
            Ok(ResolverValue::Obj(inner))
        }
        AstNode::Substitution {
            path,
            optional,
            pos,
        } => Ok(ResolverValue::Subst(SubstPlaceholder {
            path,
            optional,
            line: pos.line,
            col: pos.col,
            prefix_len: 0,
        })),
        AstNode::Concat { nodes, .. } => {
            let mut separator_flags = Vec::with_capacity(nodes.len());
            let mut rv_nodes = Vec::with_capacity(nodes.len());
            for node in nodes {
                let is_sep = matches!(
                    node,
                    AstNode::Scalar {
                        separator: true,
                        ..
                    }
                );
                rv_nodes.push(ast_to_resolver_value(node, opts, path_prefix)?);
                separator_flags.push(is_sep);
            }
            Ok(ResolverValue::Concat(ConcatPlaceholder {
                nodes: rv_nodes,
                separator_flags,
            }))
        }
        AstNode::Include { .. } => Ok(ResolverValue::Resolved(HoconValue::Scalar(
            ScalarValue::Null,
        ))),
    }
}

fn load_include(
    include_path: &str,
    required: bool,
    line: usize,
    col: usize,
    opts: &ResolveOptions,
    _path_prefix: &[String],
) -> Result<ResObj, ResolveError> {
    let base = match &opts.base_dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().unwrap_or_default(),
    };

    let abs_path = base.join(include_path);

    let has_extension = abs_path.extension().is_some();

    if has_extension {
        // Exact path: try only this candidate, silently ignore if file not found (unless required)
        return match load_single_include(&abs_path, opts) {
            Ok(obj) => Ok(obj),
            Err(_) if !abs_path.exists() => {
                if required {
                    return Err(ResolveError {
                        message: format!("required include file not found: {}", abs_path.display()),
                        path: abs_path.display().to_string(),
                        line,
                        col,
                    });
                }
                Ok(ResObj::new())
            }
            Err(e) => Err(e),
        };
    }

    // No extension: probe and merge in .properties, .json, .conf order; later merges win, so .conf has highest precedence
    let extensions = ["properties", "json", "conf"];
    let mut merged = ResObj::new();
    let mut found_any = false;
    for ext in &extensions {
        let candidate = abs_path.with_extension(ext);
        match load_single_include(&candidate, opts) {
            Ok(obj) => {
                found_any = true;
                deep_merge_res_obj_into(&mut merged, obj);
            }
            Err(e) => {
                if candidate.exists() {
                    // File exists but parsing failed — propagate the error
                    return Err(e);
                }
                // File not found — try next extension
            }
        }
    }

    if found_any {
        Ok(merged)
    } else if required {
        Err(ResolveError {
            message: format!("required include file not found: {}", abs_path.display()),
            path: abs_path.display().to_string(),
            line,
            col,
        })
    } else {
        // Missing includes silently ignored per HOCON spec
        Ok(ResObj::new())
    }
}

fn load_single_include(
    candidate: &std::path::Path,
    opts: &ResolveOptions,
) -> Result<ResObj, ResolveError> {
    // Circular include detection
    if opts.include_stack.contains(&candidate.to_path_buf()) {
        return Err(ResolveError {
            message: format!("circular include: {}", candidate.display()),
            path: candidate.display().to_string(),
            line: 0,
            col: 0,
        });
    }

    let content = fs::read_to_string(candidate).map_err(|e| ResolveError {
        message: format!("failed to read {}: {}", candidate.display(), e),
        path: candidate.display().to_string(),
        line: 0,
        col: 0,
    })?;

    // Handle .properties files specially
    if candidate.extension().and_then(|e| e.to_str()) == Some("properties") {
        let hv = crate::properties::properties_to_hocon(&content);
        if let HoconValue::Object(fields) = hv {
            let mut obj = ResObj::new();
            for (k, v) in fields {
                obj.fields.insert(k, ResolverValue::Resolved(v));
            }
            return Ok(obj);
        }
        return Ok(ResObj::new());
    }

    let tokens = crate::lexer::tokenize(&content).map_err(|e| ResolveError {
        message: e.message,
        path: candidate.display().to_string(),
        line: e.line,
        col: e.col,
    })?;
    let ast = crate::parser::parse_tokens(&tokens).map_err(|e| ResolveError {
        message: e.message,
        path: candidate.display().to_string(),
        line: e.line,
        col: e.col,
    })?;

    let mut child_opts = ResolveOptions::new(opts.env.clone());
    if let Some(parent) = candidate.parent() {
        child_opts = child_opts.with_base_dir(parent.to_path_buf());
    }
    child_opts.include_stack = opts.include_stack.clone();
    child_opts.include_stack.push(candidate.to_path_buf());

    build_res_obj(ast, &child_opts, &[])
}

/// Relativize all substitution paths in a ResolverValue tree by prepending the given prefix.
/// Called when including a file into a nested scope so `${y}` becomes `${prefix.y}`.
fn relativize_subst_paths(val: &mut ResolverValue, prefix: &str, prefix_segment_count: usize) {
    match val {
        ResolverValue::Subst(s) => {
            s.path = format!("{}.{}", prefix, s.path);
            s.prefix_len = prefix_segment_count;
        }
        ResolverValue::Concat(c) => {
            for node in &mut c.nodes {
                relativize_subst_paths(node, prefix, prefix_segment_count);
            }
        }
        ResolverValue::Append(a) => {
            relativize_subst_paths(&mut a.existing, prefix, prefix_segment_count);
            relativize_subst_paths(&mut a.elem, prefix, prefix_segment_count);
        }
        ResolverValue::Obj(o) => {
            relativize_res_obj(o, prefix, prefix_segment_count);
        }
        ResolverValue::UnresolvedArray(items) => {
            for item in items {
                relativize_subst_paths(item, prefix, prefix_segment_count);
            }
        }
        ResolverValue::Resolved(_) => {}
    }
}

fn relativize_res_obj(obj: &mut ResObj, prefix: &str, prefix_segment_count: usize) {
    for val in obj.fields.values_mut() {
        relativize_subst_paths(val, prefix, prefix_segment_count);
    }
    for val in obj.prior_values.values_mut() {
        relativize_subst_paths(val, prefix, prefix_segment_count);
    }
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

fn deep_merge_res_obj_into(dst: &mut ResObj, src: ResObj) {
    for (k, src_val) in src.fields {
        let dst_is_obj = dst.fields.get(&k).and_then(as_res_obj);
        let src_obj = as_res_obj(&src_val);

        if let (Some(mut dst_obj), Some(src_obj)) = (dst_is_obj, src_obj) {
            deep_merge_res_obj_into(&mut dst_obj, src_obj);
            dst.fields.insert(k, ResolverValue::Obj(dst_obj));
            continue;
        }

        if let Some(old) = dst.fields.get(&k) {
            dst.prior_values.insert(k.clone(), old.clone());
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
            Some(resolved) => {
                // Delayed merge: if both current and prior resolve to objects, deep merge
                if let HoconValue::Object(ref current_fields) = resolved {
                    if let Some(prior) = obj.prior_values.get(key) {
                        if let Some(HoconValue::Object(prior_fields)) =
                            resolve_val(prior, obj, root, resolving, cache, env)?
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
                    if let Some(prior_resolved) =
                        resolve_val(prior, obj, root, resolving, cache, env)?
                    {
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
        ResolverValue::Concat(c) => resolve_concat(
            &c.nodes,
            &c.separator_flags,
            scope,
            root,
            resolving,
            cache,
            env,
        )
        .map(Some),
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
        let prior = scope
            .prior_values
            .get(root_seg)
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
            // Self-referential substitution: only use prior value when the substitution
            // path matches the key we found (e.g., b=${b} where fields[b]=Subst(b)).
            if matches!(found, ResolverValue::Subst(_) | ResolverValue::Concat(_)) {
                let is_self_ref = match found {
                    ResolverValue::Subst(sub) => sub.path == s.path,
                    ResolverValue::Concat(c) => c
                        .nodes
                        .iter()
                        .any(|n| matches!(n, ResolverValue::Subst(sub) if sub.path == s.path)),
                    _ => false,
                };
                if is_self_ref {
                    let root_seg = segments.first().map(|s| s.as_str()).unwrap_or("");
                    let prior = scope
                        .prior_values
                        .get(root_seg)
                        .or_else(|| root.prior_values.get(root_seg));
                    if let Some(prior) = prior {
                        let result = resolve_val(prior, scope, root, resolving, cache, env)?;
                        if let Some(ref r) = result {
                            cache.insert(s.path.clone(), r.clone());
                        }
                        return Ok(result);
                    }
                }
            }
            let mut result = resolve_val(found, scope, root, resolving, cache, env)?;

            // Delayed merge: if the resolved value is an Object and there is a prior
            // value for the root segment, resolve the prior and deep merge underneath.
            // Only apply for single-segment paths; for multi-segment paths (e.g. foo.bar),
            // the prior value of the root segment (foo) is a different object and must not
            // be merged into the resolved value of the full path.
            if segments.len() == 1 {
                if let Some(HoconValue::Object(ref current_fields)) = result {
                    let root_seg = segments.first().map(|s| s.as_str()).unwrap_or("");
                    if let Some(prior) = root.prior_values.get(root_seg) {
                        if let Some(HoconValue::Object(prior_fields)) =
                            resolve_val(prior, scope, root, resolving, cache, env)?
                        {
                            let merged =
                                deep_merge_hocon_objects(prior_fields, current_fields.clone());
                            result = Some(merged);
                        }
                    }
                }
            }

            if let Some(ref r) = result {
                cache.insert(s.path.clone(), r.clone());
            }
            return Ok(result);
        }

        // Env var fallback — also try the original (non-relativized) path
        let env_result = env.get(&s.path).cloned().or_else(|| {
            if s.prefix_len > 0 {
                let segments = parse_subst_path(&s.path);
                if segments.len() > s.prefix_len {
                    let original_path = segments[s.prefix_len..].join(".");
                    env.get(&original_path).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        });
        if let Some(env_val) = env_result {
            let result = HoconValue::Scalar(ScalarValue::String(env_val));
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
    separator_flags: &[bool],
    scope: &ResObj,
    root: &ResObj,
    resolving: &mut HashSet<String>,
    cache: &mut HashMap<String, HoconValue>,
    env: &HashMap<String, String>,
) -> Result<HoconValue, ResolveError> {
    let mut resolved: Vec<(HoconValue, bool)> = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        let is_sep = separator_flags.get(i).copied().unwrap_or(false);
        if let Some(v) = resolve_val(n, scope, root, resolving, cache, env)? {
            resolved.push((v, is_sep));
        }
    }

    if resolved.is_empty() {
        return Ok(HoconValue::Scalar(ScalarValue::Null));
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
                    if let (Some(HoconValue::Object(existing)), HoconValue::Object(new_fields)) =
                        (merged.get(&k).cloned(), &val)
                    {
                        merged.insert(k, deep_merge_hocon_objects(existing, new_fields.clone()));
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
    Ok(HoconValue::Scalar(ScalarValue::String(s)))
}

fn deep_merge_hocon_objects(
    base: IndexMap<String, HoconValue>,
    overlay: IndexMap<String, HoconValue>,
) -> HoconValue {
    let mut merged = base;
    for (k, v) in overlay {
        if let (Some(HoconValue::Object(existing)), HoconValue::Object(new_fields)) =
            (merged.get(&k).cloned(), &v)
        {
            merged.insert(k, deep_merge_hocon_objects(existing, new_fields.clone()));
        } else {
            merged.insert(k, v);
        }
    }
    HoconValue::Object(merged)
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
        resolve(ast, &ResolveOptions::new(env.clone())).unwrap()
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
            Some(&HoconValue::Scalar(ScalarValue::String("localhost".into())))
        );
    }

    #[test]
    fn resolves_number() {
        let v = resolve_str("port = 8080");
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::Int(8080)))
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
            Some(&HoconValue::Scalar(ScalarValue::Int(2)))
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
            Some(&HoconValue::Scalar(ScalarValue::String("localhost".into())))
        );
    }

    #[test]
    fn resolves_nested_path_substitution() {
        let v = resolve_str("server { host = \"x\" }\nhost = ${server.host}");
        assert_eq!(
            obj(&v).get("host"),
            Some(&HoconValue::Scalar(ScalarValue::String("x".into())))
        );
    }

    #[test]
    fn resolves_optional_substitution_exists() {
        let v = resolve_str("a = 1\nb = ${?a}");
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::Int(1)))
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
            Some(&HoconValue::Scalar(ScalarValue::Int(50051)))
        );
    }

    #[test]
    fn uses_env_var_when_present() {
        let mut env = HashMap::new();
        env.insert("GRPC_PORT".into(), "9090".into());
        let v = resolve_str_with_env("port = 50051\nport = ${?GRPC_PORT}", &env);
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::String("9090".into())))
        );
    }

    #[test]
    fn throws_on_unresolved_mandatory() {
        let tokens = tokenize("b = ${missing}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &ResolveOptions::new(HashMap::new())).is_err());
    }

    #[test]
    fn resolves_env_var_fallback() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".into(), "hello".into());
        let v = resolve_str_with_env("b = ${MY_VAR}", &env);
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::String("hello".into())))
        );
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
    fn resolves_last_assignment_wins_for_substitution() {
        // b=${x} then b=${y} — ${b} should resolve to y's value (5), not x's ({q:10})
        let v = resolve_str("x={q:10}\ny=5\nb=${x}\nb=${y}");
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::Int(5)))
        );
    }

    #[test]
    fn resolves_string_concat_with_substitution() {
        let v = resolve_str("host = \"localhost\"\nurl = \"http://\"${host}");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::String(
                "http://localhost".into()
            )))
        );
    }

    #[test]
    fn throws_on_circular_substitution() {
        let tokens = tokenize("a = ${b}\nb = ${a}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &ResolveOptions::new(HashMap::new())).is_err());
    }

    #[test]
    fn resolves_forward_reference() {
        let v = resolve_str("url = ${host}\nhost = \"localhost\"");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::String("localhost".into())))
        );
    }

    #[test]
    fn delayed_merge_object_with_substitution() {
        // a=${x} then a={c:3} should deep merge: {q:10, c:3}
        let v = resolve_str("x={q:10}\na=${x}\na={c:3}");
        let a = obj(&v).get("a").cloned().unwrap();
        match a {
            HoconValue::Object(map) => {
                assert_eq!(map.get("c"), Some(&HoconValue::Scalar(ScalarValue::Int(3))));
                assert_eq!(
                    map.get("q"),
                    Some(&HoconValue::Scalar(ScalarValue::Int(10)))
                );
            }
            other => panic!("expected object, got {:?}", other),
        }
    }
}
