use crate::error::ResolveError;
use crate::value::HoconValue;
use std::fs;

use super::structure_builder::StructureBuilder;
use super::types::{ResObj, ResolveOptions, ResolverValue};
use super::utils::deep_merge_res_obj_into;

pub(crate) fn load_include(
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

    StructureBuilder::new(&child_opts).build(ast, &[])
}
