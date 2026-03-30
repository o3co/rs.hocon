use indexmap::IndexMap;

/// A resolved HOCON value.
///
/// This is the tree that [`Config`](crate::Config) wraps. You normally interact
/// with it through the typed getters on `Config`, but it is also returned
/// directly by [`Config::get`](crate::Config::get) and
/// [`Config::get_list`](crate::Config::get_list).
#[derive(Debug, Clone, PartialEq)]
pub enum HoconValue {
    /// An ordered map of key-value pairs (HOCON object / JSON object).
    Object(IndexMap<String, HoconValue>),
    /// An ordered list of values (HOCON array / JSON array).
    Array(Vec<HoconValue>),
    /// A leaf value (string, number, boolean, or null).
    Scalar(ScalarValue),
}

/// A scalar (leaf) value inside a HOCON document.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    /// A string value.
    String(String),
    /// A 64-bit signed integer.
    Int(i64),
    /// A 64-bit floating-point number.
    Float(f64),
    /// A boolean value.
    Bool(bool),
    /// An explicit null.
    Null,
}
