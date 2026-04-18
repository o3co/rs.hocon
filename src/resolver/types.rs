use crate::lexer::Segment;
use crate::value::HoconValue;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::PathBuf;

// ---- Public types ----

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
pub(crate) enum ResolverValue {
    Resolved(HoconValue),
    Subst(SubstPlaceholder),
    Concat(ConcatPlaceholder),
    Append(AppendPlaceholder),
    Obj(ResObj),
    UnresolvedArray(Vec<ResolverValue>),
}

#[derive(Debug, Clone)]
pub(crate) struct SubstPlaceholder {
    pub segments: Vec<Segment>,
    pub optional: bool,
    pub line: usize,
    pub col: usize,
    pub prefix_len: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ConcatPlaceholder {
    pub nodes: Vec<ResolverValue>,
    /// Parallel array: true if the corresponding node is a parser-synthesized separator.
    pub separator_flags: Vec<bool>,
}

#[derive(Debug, Clone)]
pub(crate) struct AppendPlaceholder {
    pub existing: Box<ResolverValue>,
    pub elem: Box<ResolverValue>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResObj {
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
