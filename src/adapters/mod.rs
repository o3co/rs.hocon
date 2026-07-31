//! Read config formats owned by *other* programs as HOCON.
//!
//! Each adapter returns a fully resolved [`Config`](crate::Config) you place
//! under your own document with [`Config::with_fallback`](crate::Config::with_fallback),
//! so a `${...}` can reach into it:
//!
//! ```ignore
//! let base = hocon::adapters::env::load(hocon::adapters::env::Options {
//!     prefix: "APP_".into(),
//!     ..Default::default()
//! })?;
//! let cfg = hocon::parse_string_with_options(src, opts_without_resolution)?;
//! let merged = cfg.with_fallback(&base).resolve(Default::default())?;
//! ```
//!
//! Deferring resolution matters: the plain `parse` resolves as it goes, so a
//! `${...}` aimed at the fallback would fail before the fallback is attached.
//!
//! Every adapter is behind a cargo feature, so the crate's default build still
//! depends on `indexmap` alone. `properties` and `env` need nothing extra;
//! `jsonc`, `toml` and `yaml` pull one crate each.
//!
//! Foreign data stays data: a `${a.b}` in an ingested value is literal text,
//! never a reference (spec F0.2). Ingestion is AST-level — a document is
//! decoded and turned into a value tree, never rendered to HOCON text.
//!
//! See the format-ingestion mapping spec:
//! <https://github.com/o3co/xx.hocon/blob/main/docs/format-ingestion-mapping.md>

use crate::value::HoconValue;
use crate::Config;

#[cfg(feature = "adapters-env")]
pub mod env;
#[cfg(feature = "adapters-jsonc")]
pub mod jsonc;
#[cfg(feature = "adapters-properties")]
pub mod properties;
#[cfg(feature = "adapters-toml")]
pub mod toml;
#[cfg(feature = "adapters-yaml")]
pub mod yaml;

/// Drop a leading UTF-8 BOM (spec F0.9).
///
/// Windows editors emit one, and a BOM left in place becomes part of the first
/// key: `a: 1` yields the key `"\u{feff}a"`, so a lookup of `a` misses and the
/// value is silently unreachable. That plausible-but-wrong output is the
/// failure mode this spec most wants to avoid. The core HOCON parser already
/// ignores U+FEFF, so this only brings the adapters into line.
pub(crate) fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{feff}').unwrap_or(input)
}

/// Wrap an already-built object tree as a resolved `Config`.
pub(crate) fn config_from_object(root: HoconValue, origin: Option<&str>) -> Config {
    match root {
        HoconValue::Object(fields) => Config::new_with_meta(fields, origin.map(|s| s.to_owned())),
        _ => unreachable!("callers pass an object"),
    }
}

/// The error every adapter reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterError {
    /// Human-readable description, citing the spec item where one applies.
    pub message: String,
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AdapterError {}

impl AdapterError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
