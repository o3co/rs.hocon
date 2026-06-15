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

impl HoconValue {
    /// The string, if this is a **string** scalar.
    ///
    /// Strict: numbers/booleans/null return `None` (mirrors `serde_json::Value::as_str`).
    /// For coercing any scalar to text, use [`Config::get_string`](crate::Config::get_string).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            HoconValue::Scalar(sv) if sv.value_type == ScalarType::String => Some(&sv.raw),
            _ => None,
        }
    }

    /// This value as `i64`, if it is a scalar coercible to one.
    ///
    /// HOCON-aware coercion matching [`Config::get_i64`](crate::Config::get_i64):
    /// a quoted `"8080"` and a bare `8080` both yield `Some(8080)`, and a
    /// whole-number numeric scalar (`1.0`, `1e3`) is truncated to its integer
    /// value. A non-whole number (`1.5`) yields `None`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            HoconValue::Scalar(sv) => scalar_as_i64(sv),
            _ => None,
        }
    }

    /// This value as `f64`, if it is a scalar whose raw text parses as one.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            HoconValue::Scalar(sv) => sv.raw.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// This value as `bool`, if it is a scalar with a recognised boolean spelling.
    ///
    /// Accepts `true`/`yes`/`on` and `false`/`no`/`off` (case-insensitive),
    /// matching the serde boolean coercion.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            HoconValue::Scalar(sv) => match sv.raw.to_lowercase().as_str() {
                "true" | "yes" | "on" => Some(true),
                "false" | "no" | "off" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    /// The underlying ordered map, if this is an object.
    pub fn as_object(&self) -> Option<&IndexMap<String, HoconValue>> {
        match self {
            HoconValue::Object(map) => Some(map),
            _ => None,
        }
    }

    /// The underlying slice, if this is an array.
    ///
    /// Structural: a numeric-keyed object is **not** coerced to an array here
    /// (unlike [`Config::get_list`](crate::Config::get_list) / serde sequence
    /// deserialization). Use `get_as::<Vec<_>>` / `from_value::<Vec<_>>` for that.
    pub fn as_array(&self) -> Option<&[HoconValue]> {
        match self {
            HoconValue::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Whether this value is an object.
    pub fn is_object(&self) -> bool {
        matches!(self, HoconValue::Object(_))
    }

    /// Whether this value is an array.
    pub fn is_array(&self) -> bool {
        matches!(self, HoconValue::Array(_))
    }

    /// Whether this value is a scalar (string, number, boolean, or null).
    pub fn is_scalar(&self) -> bool {
        matches!(self, HoconValue::Scalar(_))
    }

    /// Whether this value is an explicit null scalar.
    pub fn is_null(&self) -> bool {
        matches!(
            self,
            HoconValue::Scalar(sv) if sv.value_type == ScalarType::Null
        )
    }
}

/// Coerce a scalar to `i64` with the same rules as [`Config::get_i64`] and the
/// serde integer path (`parse_int_from_scalar`): direct parse, else whole-number
/// float/exponent coercion for any float-like raw (quoted or not). Kept in sync
/// so `HoconValue::as_i64`, `Config::get_i64`, and `get_as::<i64>` all agree.
fn scalar_as_i64(sv: &ScalarValue) -> Option<i64> {
    sv.raw
        .parse::<i64>()
        .ok()
        .or_else(|| whole_float_to_i64(&sv.raw))
}

/// Coerce a whole-number float/exponent **raw string** to an exact `i64`.
///
/// This is the float-like fallback shared by [`HoconValue::as_i64`],
/// [`Config::get_i64`](crate::Config::get_i64), and the serde integer path; it is
/// used only after a direct `parse::<i64>()` of the raw string fails. Returns
/// `None` unless the text is float-like (`.`/`e`/`E`), denotes a whole number, and
/// fits `i64`.
///
/// Wholeness and the integer value are derived from the decimal digits, never
/// from an intermediate `f64` (xx.hocon#56). Above 2^52 an `f64` can no longer
/// represent fractional parts, so `"4503599627370496.5".parse::<f64>().fract()`
/// is `0.0` (false-accept) and `"9007199254740993.0"` rounds to the wrong integer
/// (silent off-by-one). Parsing the digits directly avoids both, while still
/// accepting genuinely whole large literals such as `1e16`.
pub(crate) fn whole_float_to_i64(raw: &str) -> Option<i64> {
    // Plain integers are handled by the caller's direct `parse::<i64>()`.
    if !raw.contains(['.', 'e', 'E']) {
        return None;
    }
    let s = raw.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    // Mantissa and base-10 exponent (default 0).
    let (mantissa, exp) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (body, 0),
    };
    // Integer and fractional digit runs.
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    // Significant digits as one run; leading zeros never affect the value, so
    // drop them (keeps the overflow guard and u64 parse below significant-only).
    // The decimal point sits `r` places from the right after applying exponent.
    let combined = format!("{int_part}{frac_part}");
    let digits = combined.trim_start_matches('0');
    let r = frac_part.len() as i64 - exp as i64;
    // i64 has at most 19 decimal digits; any longer magnitude cannot fit, so it
    // is reported as out-of-range below — but for the right-shift (append-zeros)
    // branch we must bound BEFORE materializing the zeros, or a huge exponent
    // such as "1e2147483647" would allocate gigabytes before failing.
    let mag_str: String = if r <= 0 {
        let zeros = (-r) as usize;
        if digits.len().saturating_add(zeros) > 19 {
            return None;
        }
        let mut t = String::with_capacity(digits.len() + zeros);
        t.push_str(digits);
        t.extend(std::iter::repeat_n('0', zeros));
        t
    } else {
        let r = r as usize;
        if r >= digits.len() {
            // |value| < 1: whole only if every digit is zero.
            if digits.bytes().all(|b| b == b'0') {
                String::from("0")
            } else {
                return None;
            }
        } else {
            let (head, tail) = digits.split_at(digits.len() - r);
            // Whole only if the fractional digits are all zero.
            if !tail.bytes().all(|b| b == b'0') {
                return None;
            }
            head.to_string()
        }
    };
    // Parse the unsigned magnitude, then apply the sign with an explicit range
    // check so that i64::MIN (magnitude 2^63, which does not fit i64) is
    // preserved for float-like spellings like "-9223372036854775808.0".
    let mag: u64 = if mag_str.is_empty() {
        0
    } else {
        mag_str.parse().ok()? // overflow -> None
    };
    if neg {
        match mag.cmp(&((i64::MAX as u64) + 1)) {
            std::cmp::Ordering::Less => Some(-(mag as i64)),
            std::cmp::Ordering::Equal => Some(i64::MIN),
            std::cmp::Ordering::Greater => None,
        }
    } else {
        i64::try_from(mag).ok()
    }
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
