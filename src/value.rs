use indexmap::IndexMap;

/// Payload for an unresolved substitution placeholder. Used internally by the
/// deferred-resolution path (E12). Not part of the stable public API — marked
/// `#[doc(hidden)]` to exclude from rustdoc and signal non-stable visibility.
/// The type is `pub` for technical reasons (it is a field of the public `HoconValue`
/// enum), but no semver guarantees are made for it.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceholderValue {
    /// The dot-separated substitution path (e.g. `"db.host"`).
    pub(crate) path: String,
    /// Whether this was an optional substitution (`${?x}` vs `${x}`).
    pub(crate) optional: bool,
}

/// A resolved HOCON value.
///
/// This is the tree that [`Config`](crate::Config) wraps. You normally interact
/// with it through the typed getters on `Config`, but it is also returned
/// directly by [`Config::get`](crate::Config::get) and
/// [`Config::get_list`](crate::Config::get_list).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum HoconValue {
    /// An ordered map of key-value pairs (HOCON object / JSON object).
    Object(IndexMap<String, HoconValue>),
    /// An ordered list of values (HOCON array / JSON array).
    Array(Vec<HoconValue>),
    /// A leaf value (string, number, boolean, or null).
    Scalar(ScalarValue),
    /// An unresolved substitution placeholder. Not part of the stable public API.
    /// Marked `#[doc(hidden)]`; callers using the fused parse path will never see
    /// this variant. May be encountered when `allow_unresolved=true` is passed to
    /// [`Config::resolve`](crate::Config::resolve) — check `Config::is_resolved()`
    /// instead of matching on this variant.
    #[doc(hidden)]
    Placeholder(PlaceholderValue),
}

/// The type tag for a scalar value.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    /// A string value.
    String,
    /// A numeric value (integer or floating-point).
    Number,
    /// A boolean value.
    Boolean,
    /// An explicit null.
    Null,
}

/// A scalar (leaf) value inside a HOCON document.
///
/// Stores the raw string representation alongside a type tag.
/// Typed access (i64, f64, bool) is done by parsing `raw` on demand.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarValue {
    /// The raw string as it appeared in the source (or was produced by resolution).
    pub raw: String,
    /// The semantic type of this scalar.
    pub value_type: ScalarType,
}

impl ScalarValue {
    /// Create a new scalar value with explicit type.
    pub fn new(raw: String, value_type: ScalarType) -> Self {
        Self { raw, value_type }
    }

    /// Create a string-typed scalar.
    pub fn string(raw: String) -> Self {
        Self {
            raw,
            value_type: ScalarType::String,
        }
    }

    /// Create a null scalar.
    pub fn null() -> Self {
        Self {
            raw: "null".to_string(),
            value_type: ScalarType::Null,
        }
    }

    /// Create a boolean scalar.
    pub fn boolean(value: bool) -> Self {
        Self {
            raw: if value { "true" } else { "false" }.to_string(),
            value_type: ScalarType::Boolean,
        }
    }

    /// Create a number scalar from a raw string.
    pub fn number(raw: String) -> Self {
        Self {
            raw,
            value_type: ScalarType::Number,
        }
    }
}
