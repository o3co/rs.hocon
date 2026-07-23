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
                // Extension-probe merge: substitutions inside an included file
                // are still file-local at this stage (relativization happens at
                // the caller side after the file group merges complete). Pass
                // empty prefix so fold uses bare-leaf keys.
                deep_merge_res_obj_into(&mut merged, obj, &[]);
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

    // S3.1 (corrected, xx.hocon E10): an empty / whitespace-only / comment-only
    // included file is a valid empty document — `parse_tokens` yields an empty
    // object AST contributing `{}`. The former #105 Lightbend-compat carve-out
    // is now simply the rule, uniform with top-level parses (src/lib.rs) and
    // E11 package includes.
    let ast = crate::parser::parse_tokens(&tokens).map_err(|e| ResolveError {
        message: e.message,
        path: candidate.display().to_string(),
        line: e.line,
        col: e.col,
    })?;
    // S14b.1 (HOCON.md L993-994): an included file must contain an object, not
    // an array. The document is valid syntax (S3.5) — surface the type
    // constraint as a ResolveError naming THIS file. Checking the AST at the
    // parse site means nested include chains name the innermost file that
    // actually has the array root.
    if let crate::parser::AstNode::Array { pos, .. } = &ast {
        return Err(ResolveError {
            message: format!(
                "included file has array at file root — an included file must contain an object, not an array (HOCON.md L993-994): {}",
                candidate.display()
            ),
            path: candidate.display().to_string(),
            line: pos.line,
            col: pos.col,
        });
    }

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

    // S3.1 (corrected, xx.hocon E10): empty registered content parses to `{}`
    // naturally via `parse_tokens` — same uniform rule as top-level parses and
    // file includes (E11 decision 4 note: empty content is not an error).
    let tokens = crate::lexer::tokenize(content).map_err(|e| ResolveError {
        message: e.message,
        path: format!("package({:?}, {:?})", identifier, file),
        line: e.line,
        col: e.col,
    })?;

    let ast = crate::parser::parse_tokens(&tokens).map_err(|e| ResolveError {
        message: e.message,
        path: format!("package({:?}, {:?})", identifier, file),
        line: e.line,
        col: e.col,
    })?;
    // S14b.1: registered package content with an array root — same type
    // constraint as file includes, naming the package source (checked at the
    // parse site so nested chains name the innermost source).
    if let crate::parser::AstNode::Array { pos, .. } = &ast {
        return Err(ResolveError {
            message: format!(
                "included file has array at file root — an included file must contain an object, not an array (HOCON.md L993-994): package({:?}, {:?})",
                identifier, file
            ),
            path: format!("package({:?}, {:?})", identifier, file),
            line: pos.line,
            col: pos.col,
        });
    }

    // Build child ResolveOptions: inherit env + registry; push cycle key
    let mut child_opts = InternalResolveOptions::new(opts.env.clone());
    child_opts.package_registry = opts.package_registry.clone();
    // No base_dir for package includes (content is in-memory, not filesystem)
    child_opts.include_stack = opts.include_stack.clone();
    child_opts.include_stack.push(key);

    StructureBuilder::new(&child_opts).build(ast, &[])
}
