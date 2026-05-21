use crate::error::ResolveError;
use crate::value::HoconValue;
use std::fs;

use super::structure_builder::StructureBuilder;
use super::types::{IncludeKey, InternalResolveOptions, ResObj, ResolverValue};
use super::utils::deep_merge_res_obj_into;

pub(crate) fn load_include(
    include_path: &str,
    required: bool,
    is_file: bool,
    line: usize,
    col: usize,
    opts: &InternalResolveOptions,
    _path_prefix: &[String],
) -> Result<ResObj, ResolveError> {
    // file() includes resolve relative to CWD (or as absolute), NOT relative
    // to the including file's directory.  Bare includes use the including
    // file's base_dir (falling back to CWD when there is none).
    let base = if is_file {
        std::env::current_dir().unwrap_or_default()
    } else {
        match &opts.base_dir {
            Some(dir) => dir.clone(),
            None => std::env::current_dir().unwrap_or_default(),
        }
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
    opts: &InternalResolveOptions,
) -> Result<ResObj, ResolveError> {
    // Circular include detection — check against IncludeKey::Path entries
    let candidate_key = IncludeKey::Path(candidate.to_path_buf());
    if opts.include_stack.contains(&candidate_key) {
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

    // Lightbend-compat carve-out (#105 cross-impl): an empty / whitespace-only /
    // comment-only included file contributes an empty config rather than
    // erroring with S3.1. Top-level parses (parse_string / parse_file on a
    // top-level empty document) continue to enforce S3.1 in `parse_with_env` /
    // `parse_file_with_env` (src/lib.rs); the carve-out is scoped to the
    // file-include path only. E11 package includes are unchanged.
    let has_content = tokens.iter().any(|t| {
        !matches!(
            t.kind,
            crate::lexer::TokenKind::Newline | crate::lexer::TokenKind::Eof
        )
    });
    if !has_content {
        return Ok(ResObj::new());
    }

    let ast = crate::parser::parse_tokens(&tokens).map_err(|e| ResolveError {
        message: e.message,
        path: candidate.display().to_string(),
        line: e.line,
        col: e.col,
    })?;

    let mut child_opts = InternalResolveOptions::new(opts.env.clone());
    if let Some(parent) = candidate.parent() {
        child_opts = child_opts.with_base_dir(parent.to_path_buf());
    }
    child_opts.include_stack = opts.include_stack.clone();
    child_opts.include_stack.push(candidate_key);
    #[cfg(feature = "include-package")]
    {
        child_opts.package_registry = opts.package_registry.clone();
    }

    StructureBuilder::new(&child_opts).build(ast, &[])
}

/// Load a `package(...)` include — E11.
///
/// Looks up `(identifier, file)` in the per-parser registry, parses the
/// registered content, and returns the resolved object. Cycle detection
/// uses `IncludeKey::Package { identifier, file }` (E11 decision 8).
#[cfg(feature = "include-package")]
pub(crate) fn load_package_include(
    identifier: &str,
    file: &str,
    required: bool,
    line: usize,
    col: usize,
    opts: &InternalResolveOptions,
) -> Result<ResObj, ResolveError> {
    // Cycle detection (E11 decision 8)
    let key = IncludeKey::Package {
        identifier: identifier.to_string(),
        file: file.to_string(),
    };
    if opts.include_stack.contains(&key) {
        return Err(ResolveError {
            message: format!("circular package include: ({:?}, {:?})", identifier, file),
            path: format!("package({:?}, {:?})", identifier, file),
            line,
            col,
        });
    }

    // Registry lookup (E11 decision 4)
    // Borrow &str from the Arc-owned map to avoid cloning the content String on every
    // include call — tokenize/parse work on &str, so no owned copy is needed here.
    let content: &str = match opts
        .package_registry
        .get(&(identifier.to_string(), file.to_string()))
    {
        Some(c) => c.as_str(),
        None => {
            let _ = required; // required semantics: miss is always an error for package includes
            return Err(ResolveError {
                message: format!(
                    "include package not found: ({:?}, {:?}) — was Parser::register_package called?",
                    identifier, file
                ),
                path: format!("package({:?}, {:?})", identifier, file),
                line,
                col,
            });
        }
    };

    // E11 decision 4 note: empty registered content => empty merge object, not an error.
    // Do NOT call assert_non_empty_document here.
    let tokens = crate::lexer::tokenize(content).map_err(|e| ResolveError {
        message: e.message,
        path: format!("package({:?}, {:?})", identifier, file),
        line: e.line,
        col: e.col,
    })?;

    let has_content = tokens.iter().any(|t| {
        !matches!(
            t.kind,
            crate::lexer::TokenKind::Newline | crate::lexer::TokenKind::Eof
        )
    });
    if !has_content {
        // Empty registered content → empty merge object (E11 decision 4 note)
        return Ok(ResObj::new());
    }

    let ast = crate::parser::parse_tokens(&tokens).map_err(|e| ResolveError {
        message: e.message,
        path: format!("package({:?}, {:?})", identifier, file),
        line: e.line,
        col: e.col,
    })?;

    // Build child ResolveOptions: inherit env + registry; push cycle key
    let mut child_opts = InternalResolveOptions::new(opts.env.clone());
    child_opts.package_registry = opts.package_registry.clone();
    // No base_dir for package includes (content is in-memory, not filesystem)
    child_opts.include_stack = opts.include_stack.clone();
    child_opts.include_stack.push(key);

    StructureBuilder::new(&child_opts).build(ast, &[])
}
