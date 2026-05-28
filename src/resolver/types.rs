use crate::lexer::Segment;
use crate::value::HoconValue;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// ---- Include cycle detection key ----

/// Unified key for include-cycle detection.
///
/// Covers both filesystem-path includes (`include "..."` / `include file(...)`)
/// and package includes (`include package("id", "file")`). A single stack of
/// `IncludeKey` values replaces the former `Vec<PathBuf>` in `ResolveOptions`.
///
/// NOTE: marked `#[doc(hidden)] pub` (rather than `pub(crate)`) because it is
/// referenced via `InternalResolveOptions.include_stack` — itself exposed via
/// `pub mod resolver` to support integration tests like
/// `tests/resolver_phase_split.rs`. The `#[doc(hidden)]` signals that this
/// type is not stable public API.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeKey {
    /// A filesystem-path include (bare or `file(...)` qualifier).
    Path(PathBuf),
    /// A package include (`package("identifier", "file")` qualifier) — E11.
    #[cfg(feature = "include-package")]
    Package { identifier: String, file: String },
}

// ---- Public types ----

/// Internal resolver options (env, base_dir, etc.).
///
/// Distinct from the public `crate::ResolveOptions` (T3) which carries only
/// `use_system_environment` and `allow_unresolved`. Translation happens at the
/// `Config::resolve` boundary (T9).
pub struct InternalResolveOptions {
    pub env: HashMap<String, String>,
    pub base_dir: Option<PathBuf>,
    /// Include-cycle detection stack. Each entry represents a file/package
    /// currently being loaded in the call chain above this resolver invocation.
    pub include_stack: Vec<IncludeKey>,
    /// Package registry for `include package(...)` — E11.
    /// Only present when the `include-package` feature is enabled.
    #[cfg(feature = "include-package")]
    pub package_registry: std::sync::Arc<std::collections::HashMap<(String, String), String>>,
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
            #[cfg(feature = "include-package")]
            package_registry: std::sync::Arc::new(std::collections::HashMap::new()),
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
    Obj(ResObj),
    UnresolvedArray(Vec<ResolverValue>),
}

/// Substitution placeholder in the resolver's pre-resolve tree.
///
/// `#[non_exhaustive]`: the resolver module is `pub` but `#[doc(hidden)]`
/// (see lib.rs:130-136) for integration-test access. External consumers
/// that reach this type via `hocon::resolver::types` do so outside the
/// stable API contract; `#[non_exhaustive]` formally signals that
/// additional fields may be added without a major version bump.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SubstPlaceholder {
    pub segments: Vec<Segment>,
    pub optional: bool,
    /// Internal sentinel used when folding an optional self-reference with no
    /// prior value. It resolves to undefined without performing a lookup.
    /// `pub(crate)` because this is purely an internal fold-time marker —
    /// callers outside the resolver crate have no reason to set it. The
    /// surrounding `#[non_exhaustive]` keeps that semantic forward-compatible
    /// (downstream cannot construct the struct via pattern literal anyway).
    /// (Review #124 Issue 2.)
    pub(crate) known_absent: bool,
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

#[derive(Debug, Clone, Default)]
pub struct ResObj {
    pub fields: IndexMap<String, ResolverValue>,
    pub prior_values: IndexMap<String, ResolverValue>,
    /// Keys whose net value in THIS object was established by an explicit
    /// non-self-referential assignment (`k = [...]`), i.e. a reset rather than
    /// a `+=`/self-ref append. Used by [`deep_merge_res_obj_into`] to decide
    /// whether an included file's `k` chains off the destination's pre-merge
    /// value (append origin → splice) or replaces it (reset origin → discard).
    /// See go.hocon#134 (S13b.2 `+=` accumulation across includes).
    pub reset_keys: HashSet<String>,
}

impl ResObj {
    pub fn new() -> Self {
        Self::default()
    }
}
