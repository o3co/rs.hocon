use crate::lexer::Segment;
use crate::value::HoconValue;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::PathBuf;

// ---- Public types ----

/// Internal resolver options (env, base_dir, etc.).
///
/// Distinct from the public `crate::ResolveOptions` (T3) which carries only
/// `use_system_environment` and `allow_unresolved`. Translation happens at the
/// `Config::resolve` boundary (T9).
pub struct InternalResolveOptions {
    pub env: HashMap<String, String>,
    pub base_dir: Option<PathBuf>,
    pub include_stack: Vec<PathBuf>,
    /// When false, env-var fallback in phase 2 is skipped.
    /// Default true for backward compat (fused parse-and-resolve path).
    pub use_system_environment: bool,
    /// When true, missing mandatory substitutions yield Ok(None) instead of Err.
    pub allow_unresolved: bool,
}

impl InternalResolveOptions {
    pub fn new(env: HashMap<String, String>) -> Self {
        InternalResolveOptions {
            env,
            base_dir: None,
            include_stack: Vec::new(),
            use_system_environment: true,
            allow_unresolved: false,
        }
    }

    pub fn with_base_dir(mut self, base_dir: PathBuf) -> Self {
        self.base_dir = Some(base_dir);
        self
    }

    pub fn with_base_dir_opt(mut self, base_dir: Option<PathBuf>) -> Self {
        self.base_dir = base_dir;
        self
    }

    pub fn with_allow_unresolved(mut self, b: bool) -> Self {
        self.allow_unresolved = b;
        self
    }

    pub fn with_use_system_environment(mut self, b: bool) -> Self {
        self.use_system_environment = b;
        self
    }
}

/// Alias kept for backward compat within this crate; resolves to `InternalResolveOptions`.
#[allow(dead_code)]
pub type ResolveOptions = InternalResolveOptions;

// ---- Internal placeholder types ----

#[derive(Debug, Clone)]
pub enum ResolverValue {
    Resolved(HoconValue),
    Subst(SubstPlaceholder),
    Concat(ConcatPlaceholder),
    Append(AppendPlaceholder),
    Obj(ResObj),
    UnresolvedArray(Vec<ResolverValue>),
}

#[derive(Debug, Clone)]
pub struct SubstPlaceholder {
    pub segments: Vec<Segment>,
    pub optional: bool,
    /// Propagated from `AstNode::Substitution::list_suffix`; true for `${X[]}` / `${?X[]}`.
    pub list_suffix: bool,
    pub line: usize,
    pub col: usize,
    pub prefix_len: usize,
}

#[derive(Debug, Clone)]
pub struct ConcatPlaceholder {
    pub nodes: Vec<ResolverValue>,
    /// Parallel array: true if the corresponding node is a parser-synthesized separator.
    pub separator_flags: Vec<bool>,
    /// 1-based line of the concat value in the source file (from AST Concat pos).
    pub line: usize,
    /// 1-based column of the concat value in the source file (from AST Concat pos).
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct AppendPlaceholder {
    pub existing: Box<ResolverValue>,
    pub elem: Box<ResolverValue>,
}

#[derive(Debug, Clone)]
pub struct ResObj {
    pub fields: IndexMap<String, ResolverValue>,
    pub prior_values: IndexMap<String, ResolverValue>,
}

impl ResObj {
    pub fn new() -> Self {
        ResObj {
            fields: IndexMap::new(),
            prior_values: IndexMap::new(),
        }
    }
}
