// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use std::collections::HashMap;
use std::path::PathBuf;

/// Options controlling the parse phase.
///
/// Construct via [`ParseOptions::defaults()`] and chain `with_*` methods.
/// The struct literal `ParseOptions { .. }` is **not** a valid invocation —
/// it would set `resolve_substitutions: false`, contradicting the Lightbend
/// default of `true`. Per E12 §"Options encoding per language".
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Whether phase 2 (substitution resolution) runs immediately after parsing.
    /// Default: `true` (Lightbend default — fused parse-and-resolve).
    pub resolve_substitutions: bool,
    /// User-visible source name for error messages when no file path is available.
    /// Default: `None`.
    pub origin_description: Option<String>,
    /// Base directory for resolving relative include directives.
    /// Default: `None` (current directory is used when building the internal opts).
    pub(crate) base_dir: Option<PathBuf>,
    /// Custom environment variable map.  When `None`, the process environment is
    /// used (gated by `ResolveOptions::use_system_environment` at resolve time).
    pub(crate) env: Option<HashMap<String, String>>,
}

impl ParseOptions {
    /// Return `ParseOptions` with Lightbend-equivalent defaults:
    /// `resolve_substitutions = true`, everything else `None`.
    pub fn defaults() -> Self {
        ParseOptions {
            resolve_substitutions: true,
            origin_description: None,
            base_dir: None,
            env: None,
        }
    }

    /// Return a copy with `resolve_substitutions` set to `b`.
    pub fn with_resolve_substitutions(mut self, b: bool) -> Self {
        self.resolve_substitutions = b;
        self
    }

    /// Return a copy with `origin_description` set to `s`.
    pub fn with_origin_description(mut self, s: String) -> Self {
        self.origin_description = Some(s);
        self
    }

    /// Return a copy with `base_dir` set to `p`.
    pub fn with_base_dir(mut self, p: PathBuf) -> Self {
        self.base_dir = Some(p);
        self
    }

    /// Return a copy with a custom `env` map (overrides process environment).
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }
}

/// Options controlling the resolve phase.
///
/// Construct via [`ResolveOptions::defaults()`] and chain `with_*` methods.
///
/// **Distinct from the internal `resolver::InternalResolveOptions`** which
/// also carries env / base_dir / include_stack for the resolver module.
/// Translation from `ResolveOptions` to `InternalResolveOptions` happens
/// at the `Config::resolve` boundary (T9).
#[derive(Debug, Clone)]
pub struct ResolveOptions {
    /// When `true`, substitution paths not satisfied within the config tree
    /// fall back to process environment variables. Default: `true`.
    pub use_system_environment: bool,
    /// When `true`, required-but-unsatisfied substitutions are left as
    /// placeholders instead of returning a `ResolveError`. Default: `false`.
    pub allow_unresolved: bool,
}

impl ResolveOptions {
    /// Return `ResolveOptions` with Lightbend-equivalent defaults:
    /// `use_system_environment = true`, `allow_unresolved = false`.
    pub fn defaults() -> Self {
        ResolveOptions {
            use_system_environment: true,
            allow_unresolved: false,
        }
    }

    /// Return a copy with `use_system_environment` set to `b`.
    pub fn with_use_system_environment(mut self, b: bool) -> Self {
        self.use_system_environment = b;
        self
    }

    /// Return a copy with `allow_unresolved` set to `b`.
    pub fn with_allow_unresolved(mut self, b: bool) -> Self {
        self.allow_unresolved = b;
        self
    }
}
