use indexmap::IndexMap;

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
}

/// The type tag for a scalar value.
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
