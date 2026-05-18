use crate::error::ConfigError;
use crate::lexer::is_hocon_whitespace;
use crate::numeric_array::numeric_object_to_array;
use crate::value::{HoconValue, ScalarType};
use indexmap::IndexMap;

/// A parsed HOCON configuration object.
///
/// `Config` wraps an ordered map of top-level keys to [`HoconValue`]s and
/// provides typed getters that accept dot-separated paths
/// (e.g., `"server.host"`).
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    root: IndexMap<String, HoconValue>,
}

impl Config {
    /// Create a `Config` from a pre-built ordered map of key-value pairs.
    pub fn new(root: IndexMap<String, HoconValue>) -> Self {
        Self { root }
    }

    // Walk the dot-separated path through nested objects.
    fn lookup_node(&self, path: &str) -> Option<&HoconValue> {
        let segments = split_config_path(path);
        lookup_in_map_by_segments(&self.root, &segments)
    }

    /// Return the raw [`HoconValue`] at the given dot-separated path,
    /// or `None` if the path does not exist.
    pub fn get(&self, path: &str) -> Option<&HoconValue> {
        self.lookup_node(path)
    }

    /// Return the value at `path` as a `String`.
    ///
    /// Returns the raw string for any scalar value (string, number, boolean,
    /// or null). Returns [`ConfigError`] if the path is missing or the value
    /// is an Object or Array.
    pub fn get_string(&self, path: &str) -> Result<String, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(sv)) => Ok(sv.raw.clone()),
            _ => Err(type_mismatch(path, "String")),
        }
    }

    /// Return the value at `path` as an `i64`.
    ///
    /// Whole-number floats and numeric strings are coerced automatically.
    /// Returns [`ConfigError`] if the path is missing or the value cannot be
    /// represented as `i64`.
    pub fn get_i64(&self, path: &str) -> Result<i64, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(sv)) => {
                // Try direct i64 parse first
                if let Ok(n) = sv.raw.parse::<i64>() {
                    return Ok(n);
                }
                // Only use f64 fallback for float-like literals (contains '.' or exponent)
                let is_float_like =
                    sv.raw.contains('.') || sv.raw.contains('e') || sv.raw.contains('E');
                if is_float_like {
                    if let Ok(f) = sv.raw.parse::<f64>() {
                        if f.fract() == 0.0
                            && f.is_finite()
                            && f >= i64::MIN as f64
                            && f < (i64::MAX as f64)
                        {
                            return Ok(f as i64);
                        }
                    }
                }
                Err(type_mismatch(path, "i64"))
            }
            _ => Err(type_mismatch(path, "i64")),
        }
    }

    /// Return the value at `path` as an `f64`.
    ///
    /// Integers and numeric strings are coerced automatically.
    /// Returns [`ConfigError`] if the path is missing or the value cannot be
    /// represented as `f64`.
    pub fn get_f64(&self, path: &str) -> Result<f64, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(sv)) => sv
                .raw
                .parse::<f64>()
                .map_err(|_| type_mismatch(path, "f64")),
            _ => Err(type_mismatch(path, "f64")),
        }
    }

    /// Return the value at `path` as a `bool`.
    ///
    /// String values `"true"`, `"yes"`, `"on"` (case-insensitive) coerce to
    /// `true`; `"false"`, `"no"`, `"off"` coerce to `false`.
    /// Returns [`ConfigError`] if the path is missing or the value is not boolean-like.
    pub fn get_bool(&self, path: &str) -> Result<bool, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(sv)) => match sv.raw.to_lowercase().as_str() {
                "true" | "yes" | "on" => Ok(true),
                "false" | "no" | "off" => Ok(false),
                _ => Err(type_mismatch(path, "bool")),
            },
            _ => Err(type_mismatch(path, "bool")),
        }
    }

    /// Return the sub-object at `path` as a new [`Config`].
    ///
    /// Returns [`ConfigError`] if the path is missing or the value is not an object.
    pub fn get_config(&self, path: &str) -> Result<Config, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Object(map)) => Ok(Config::new(map.clone())),
            _ => Err(type_mismatch(path, "Object")),
        }
    }

    /// Return the array at `path` as a `Vec<HoconValue>`.
    ///
    /// Returns [`ConfigError`] if the path is missing or the value is not an array.
    ///
    /// Numerically-indexed objects (S15) are converted to an array on demand:
    /// `{"0":"a","1":"b"}` returns `["a","b"]`. Empty objects and objects with
    /// no integer keys are NOT converted — they return a type-mismatch error.
    pub fn get_list(&self, path: &str) -> Result<Vec<HoconValue>, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Array(items)) => Ok(items.clone()),
            Some(v @ HoconValue::Object(_)) => {
                // S15: attempt numeric-keyed object → array conversion.
                // Returns None for empty objects (S15.4) and objects with no
                // eligible integer keys (S15.12 / na12). In those cases fall
                // through to the type-mismatch error.
                numeric_object_to_array(v).ok_or_else(|| type_mismatch(path, "Array"))
            }
            _ => Err(type_mismatch(path, "Array")),
        }
    }

    /// Like [`get_string`](Self::get_string) but returns `None` instead of an error.
    pub fn get_string_option(&self, path: &str) -> Option<String> {
        self.get_string(path).ok()
    }

    /// Like [`get_i64`](Self::get_i64) but returns `None` instead of an error.
    pub fn get_i64_option(&self, path: &str) -> Option<i64> {
        self.get_i64(path).ok()
    }

    /// Like [`get_f64`](Self::get_f64) but returns `None` instead of an error.
    pub fn get_f64_option(&self, path: &str) -> Option<f64> {
        self.get_f64(path).ok()
    }

    /// Like [`get_bool`](Self::get_bool) but returns `None` instead of an error.
    pub fn get_bool_option(&self, path: &str) -> Option<bool> {
        self.get_bool(path).ok()
    }

    /// Like [`get_config`](Self::get_config) but returns `None` instead of an error.
    pub fn get_config_option(&self, path: &str) -> Option<Config> {
        self.get_config(path).ok()
    }

    /// Like [`get_list`](Self::get_list) but returns `None` instead of an error.
    pub fn get_list_option(&self, path: &str) -> Option<Vec<HoconValue>> {
        self.get_list(path).ok()
    }

    /// Return the value at `path` as a [`Duration`](std::time::Duration).
    ///
    /// Accepts HOCON duration strings (e.g., `"30 seconds"`, `"100ms"`,
    /// `"2 hours"`). Bare integers are interpreted as milliseconds.
    ///
    /// Supported units: `ns`/`nano`/`nanos`/`nanosecond`/`nanoseconds`,
    /// `us`/`micro`/`micros`/`microsecond`/`microseconds`,
    /// `ms`/`milli`/`millis`/`millisecond`/`milliseconds`,
    /// `s`/`second`/`seconds`, `m`/`minute`/`minutes`,
    /// `h`/`hour`/`hours`, `d`/`day`/`days`, `w`/`week`/`weeks`.
    pub fn get_duration(&self, path: &str) -> Result<std::time::Duration, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(sv)) => {
                // Try as duration string first
                if let Some(d) = parse_duration(&sv.raw) {
                    return Ok(d);
                }
                // Number types: bare integer = milliseconds, bare float = milliseconds
                if sv.value_type == ScalarType::Number {
                    if let Ok(n) = sv.raw.parse::<i64>() {
                        if n < 0 {
                            return Err(ConfigError {
                                message: format!("negative duration at {}: {}", path, sv.raw),
                                path: path.to_string(),
                            });
                        }
                        return Ok(std::time::Duration::from_millis(n as u64));
                    }
                    if let Ok(f) = sv.raw.parse::<f64>() {
                        if f < 0.0 || !f.is_finite() {
                            return Err(ConfigError {
                                message: format!("invalid duration at {}: {}", path, sv.raw),
                                path: path.to_string(),
                            });
                        }
                        let secs = f / 1000.0;
                        if secs > u64::MAX as f64 {
                            return Err(ConfigError {
                                message: format!("duration too large at {}: {}", path, sv.raw),
                                path: path.to_string(),
                            });
                        }
                        return Ok(std::time::Duration::from_secs_f64(secs));
                    }
                }
                Err(ConfigError {
                    message: format!("invalid duration at {}: {}", path, sv.raw),
                    path: path.to_string(),
                })
            }
            _ => Err(ConfigError {
                message: format!("expected duration at {}", path),
                path: path.to_string(),
            }),
        }
    }

    /// Like [`get_duration`](Self::get_duration) but returns `None` instead of an error.
    pub fn get_duration_option(&self, path: &str) -> Option<std::time::Duration> {
        self.get_duration(path).ok()
    }

    /// Return the value at `path` as a byte count (`i64`).
    ///
    /// Accepts HOCON byte-size strings (e.g., `"512 MB"`, `"1 GiB"`).
    /// Bare integers are returned as-is (assumed bytes).
    ///
    /// Supported units: `B`/`byte`/`bytes`, `K`/`KB`/`kilobyte`/`kilobytes`,
    /// `KiB`/`kibibyte`/`kibibytes`, `M`/`MB`/`megabyte`/`megabytes`,
    /// `MiB`/`mebibyte`/`mebibytes`, `G`/`GB`/`gigabyte`/`gigabytes`,
    /// `GiB`/`gibibyte`/`gibibytes`, `T`/`TB`/`terabyte`/`terabytes`,
    /// `TiB`/`tebibyte`/`tebibytes`. Fractional numbers (e.g. `0.5M`) are supported.
    pub fn get_bytes(&self, path: &str) -> Result<i64, ConfigError> {
        let v = self.lookup_node(path).ok_or_else(|| ConfigError {
            message: format!("path not found: {}", path),
            path: path.to_string(),
        })?;
        match v {
            HoconValue::Scalar(sv) => {
                // Bare integer number: return as-is (assumed bytes)
                if sv.value_type == ScalarType::Number {
                    if let Ok(n) = sv.raw.parse::<i64>() {
                        return Ok(n);
                    }
                    // Bare float without unit (e.g. "1.5") is not valid for bytes
                    return Err(ConfigError {
                        message: format!("expected byte size at {}", path),
                        path: path.to_string(),
                    });
                }
                // String type: try byte-size string (e.g. "512 MB", "1.5 KiB")
                let n = parse_bytes(&sv.raw).ok_or_else(|| ConfigError {
                    message: format!("invalid byte size at {}: {}", path, sv.raw),
                    path: path.to_string(),
                })?;
                // ub04: Lightbend `getBytesBigInteger` rejects negative byte sizes.
                // Bytes represent a resource size and must be non-negative.
                if n < 0 {
                    return Err(ConfigError {
                        message: format!("negative byte size at {}: {}", path, sv.raw),
                        path: path.to_string(),
                    });
                }
                Ok(n)
            }
            _ => Err(ConfigError {
                message: format!("expected byte size at {}", path),
                path: path.to_string(),
            }),
        }
    }

    /// Like [`get_bytes`](Self::get_bytes) but returns `None` instead of an error.
    pub fn get_bytes_option(&self, path: &str) -> Option<i64> {
        self.get_bytes(path).ok()
    }

    /// Return the value at `path` as a calendar period `(years, months, days)`.
    ///
    /// Accepts HOCON period strings (e.g. `"7d"`, `"2w"`, `"3m"`, `"1y"`) or a bare
    /// integer string, which is taken as days per HOCON.md L1321.
    ///
    /// Supported units: `d`/`day`/`days` (default), `w`/`week`/`weeks` (× 7 days),
    /// `m`/`mo`/`month`/`months`, `y`/`year`/`years`.
    ///
    /// Negative values are permitted (matches Lightbend behaviour).
    ///
    /// Return type is `(years, months, days)` using `i32` to support negative periods.
    /// A `chrono` dependency is intentionally avoided; callers that need a typed `Period`
    /// can decompose the tuple as needed.
    pub fn get_period(&self, path: &str) -> Result<(i32, i32, i32), ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(sv)) => {
                if let Some(p) = parse_period(&sv.raw) {
                    return Ok(p);
                }
                // Bare integer scalar (non-string): treat as days default (S18.1 parallel).
                if sv.value_type == ScalarType::Number {
                    if let Ok(n) = sv.raw.parse::<i32>() {
                        return Ok((0, 0, n));
                    }
                }
                Err(ConfigError {
                    message: format!("invalid period at {}: {}", path, sv.raw),
                    path: path.to_string(),
                })
            }
            _ => Err(ConfigError {
                message: format!("expected period at {}", path),
                path: path.to_string(),
            }),
        }
    }

    /// Like [`get_period`](Self::get_period) but returns `None` instead of an error.
    pub fn get_period_option(&self, path: &str) -> Option<(i32, i32, i32)> {
        self.get_period(path).ok()
    }

    /// Return `true` if a value exists at the given dot-separated path.
    pub fn has(&self, path: &str) -> bool {
        self.lookup_node(path).is_some()
    }

    /// Return the top-level keys in insertion order.
    pub fn keys(&self) -> Vec<&str> {
        self.root.keys().map(|s| s.as_str()).collect()
    }

    /// Merge this config with a fallback. Keys present in `self` win;
    /// missing keys are filled from `fallback`. Nested objects are deep-merged.
    ///
    /// ```rust
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let app = hocon::parse(r#"server.port = 9090"#)?;
    /// let defaults = hocon::parse(r#"server { host = "0.0.0.0", port = 8080 }"#)?;
    /// let merged = app.with_fallback(&defaults);
    ///
    /// assert_eq!(merged.get_i64("server.port")?, 9090);       // app wins
    /// assert_eq!(merged.get_string("server.host")?, "0.0.0.0"); // filled from defaults
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_fallback(&self, fallback: &Config) -> Config {
        let mut merged = self.root.clone();
        for (key, fallback_val) in &fallback.root {
            if let Some(receiver_val) = merged.get(key) {
                // Both sides have this key — deep merge if both are objects
                if let (HoconValue::Object(recv_map), HoconValue::Object(fb_map)) =
                    (receiver_val, fallback_val)
                {
                    let recv_cfg = Config::new(recv_map.clone());
                    let fb_cfg = Config::new(fb_map.clone());
                    let deep = recv_cfg.with_fallback(&fb_cfg);
                    merged.insert(key.clone(), HoconValue::Object(deep.root));
                }
                // else: receiver value wins, no insert needed
            } else {
                // Key missing in receiver — take from fallback
                merged.insert(key.clone(), fallback_val.clone());
            }
        }
        Config::new(merged)
    }
}

/// Split a HOCON config path into segments, respecting quoted keys.
/// e.g. `server."web.api".port` → `["server", "web.api", "port"]`
/// Empty segments are preserved: `a..b` → `["a", "", "b"]`.
/// Quoted segments process escape sequences (e.g. `\"` → `"`).
fn split_config_path(path: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let chars: Vec<char> = path.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            // Quoted segment — collect until closing quote, processing escapes
            i += 1; // skip opening quote
            let mut seg = String::new();
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    seg.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    closed = true;
                    i += 1;
                    break;
                }
                seg.push(chars[i]);
                i += 1;
            }
            if !closed {
                return vec![path.to_string()]; // treat as literal if unterminated
            }
            segments.push(seg);
            // skip optional '.' separator
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        } else {
            // Unquoted segment — collect until '.' or '"'
            // Always push the segment (even empty) to preserve consecutive-dot semantics.
            let start = i;
            while i < chars.len() && chars[i] != '.' && chars[i] != '"' {
                i += 1;
            }
            segments.push(chars[start..i].iter().collect());
            // skip optional '.' separator
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        }
    }
    // A trailing dot means there is a final empty segment
    if path.ends_with('.') {
        segments.push(String::new());
    }
    segments
}

fn lookup_in_map_by_segments<'a>(
    map: &'a IndexMap<String, HoconValue>,
    segments: &[String],
) -> Option<&'a HoconValue> {
    if segments.is_empty() {
        return None;
    }
    let key = &segments[0];
    let rest = &segments[1..];
    let value = map.get(key)?;
    if rest.is_empty() {
        Some(value)
    } else {
        match value {
            HoconValue::Object(inner) => lookup_in_map_by_segments(inner, rest),
            _ => None,
        }
    }
}

#[cfg(feature = "serde")]
impl Config {
    /// Deserialize this config into any type implementing [`serde::Deserialize`].
    ///
    /// Requires the `serde` feature. HOCON-aware coercion (e.g., string-to-number)
    /// is applied during deserialization.
    pub fn deserialize<T: ::serde::de::DeserializeOwned>(
        &self,
    ) -> Result<T, crate::serde::DeserializeError> {
        let value = HoconValue::Object(self.root.clone());
        T::deserialize(crate::serde::HoconDeserializer::new(&value))
    }
}

/// Trim HOCON whitespace (per `is_hocon_whitespace`) from both ends of `s`.
///
/// Unlike `str::trim()` (which is ASCII-only for whitespace), this respects the full
/// HOCON_WS set (U+00A0 NBSP, U+FEFF BOM, various Unicode space separators, etc.).
fn trim_hocon_ws(s: &str) -> &str {
    s.trim_matches(|c| is_hocon_whitespace(c))
}

/// Returns `true` if `s` matches `[+-]?[0-9]+` (integer pre-classification).
///
/// Mirrors Lightbend `SimpleConfig.isWholeNumber`. Used to choose the integer fast-path
/// vs fractional fallback in `parse_duration` and `parse_bytes`.
fn is_integer_str(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Parse a HOCON duration string into a [`std::time::Duration`].
///
/// Accepts `[ws] number [ws] [unit] [ws]` where `unit` is one of the HOCON duration
/// units (HOCON.md L1304-1313). When no unit is present the default is **milliseconds**
/// (HOCON.md L1301: "bare numbers are taken to be in milliseconds already").
///
/// # Fractional values (Lightbend-faithful per-family)
///
/// - Integer form `[+-]?[0-9]+`: parsed as `i64` milliseconds → scaled to nanos.
/// - Fractional form: parsed as `f64`, multiplied by `nanos_per_unit`.
///
/// # Negative values (rs-specific limitation)
///
/// `std::time::Duration` is unsigned; this function returns `None` for negative inputs.
/// Callers that need signed duration semantics should inspect the raw string first.
/// `get_duration` documents this constraint; ud06 conformance test asserts `is_err()`.
fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = trim_hocon_ws(s);
    if s.is_empty() {
        return None;
    }

    // Scan to end of the numeric prefix (digits, optional leading sign, optional decimal).
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    // Trim HOCON whitespace between number and unit, then lowercase the unit.
    let unit_str = trim_hocon_ws(&s[num_end..]).to_lowercase();

    if num_str.is_empty() {
        return None;
    }

    let nanos_per_unit: f64 = match unit_str.as_str() {
        // Default: milliseconds (HOCON.md L1301).
        "" | "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => 1_000_000.0,
        "ns" | "nano" | "nanos" | "nanosecond" | "nanoseconds" => 1.0,
        "us" | "micro" | "micros" | "microsecond" | "microseconds" => 1_000.0,
        "s" | "second" | "seconds" => 1_000_000_000.0,
        "m" | "minute" | "minutes" => 60_000_000_000.0,
        "h" | "hour" | "hours" => 3_600_000_000_000.0,
        "d" | "day" | "days" => 86_400_000_000_000.0,
        "w" | "week" | "weeks" => 604_800_000_000_000.0,
        _ => return None,
    };

    // Integer fast-path (matches Lightbend `Long.parseLong`).
    if is_integer_str(num_str) {
        let n: i64 = num_str.parse().ok()?;
        // rs-specific: std::time::Duration is unsigned; negative values are rejected.
        // Lightbend's java.time.Duration is signed. See CHANGELOG for rs-specific note.
        if n < 0 {
            return None;
        }
        let nanos = (n as f64 * nanos_per_unit) as u64;
        return Some(std::time::Duration::from_nanos(nanos));
    }

    // Fractional fallback (matches Lightbend `Double.parseDouble`).
    let f: f64 = num_str.parse().ok()?;
    if f < 0.0 || !f.is_finite() {
        return None;
    }
    let nanos = (f * nanos_per_unit) as u64;
    Some(std::time::Duration::from_nanos(nanos))
}

/// Parse a HOCON period string into a `(years, months, days)` tuple.
///
/// Accepts `[ws] integer [ws] [unit] [ws]` where `unit` is one of the HOCON period
/// units (HOCON.md L1324-1333). When no unit is present the default is **days**
/// (HOCON.md L1321: "bare numbers are taken to be in days").
///
/// Period is integer-only (matches Lightbend `Integer.parseInt`). Fractional strings
/// like `"7.5"` return `None`.
///
/// Returns `(years, months, days)` as `i32` to support negative periods (Lightbend allows
/// negative). A `chrono` dependency is intentionally avoided; callers that need a typed
/// `Period` can decompose the tuple.
pub(crate) fn parse_period(s: &str) -> Option<(i32, i32, i32)> {
    let s = trim_hocon_ws(s);
    if s.is_empty() {
        return None;
    }

    // Scan numeric prefix.
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '-' && c != '+')
        .unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    let unit_str = trim_hocon_ws(&s[num_end..]);

    if num_str.is_empty() {
        return None;
    }

    // Period is integer-only (Lightbend `Integer.parseInt`). Reject fractional.
    if !is_integer_str(num_str) {
        return None;
    }

    let n: i32 = num_str.parse().ok()?;

    // Unit match — case-sensitive per HOCON.md L1304 (period shares the same
    // "unit names are case-sensitive" rule as duration; only lowercase accepted).
    match unit_str {
        // Default: days (HOCON.md L1321).
        "" | "d" | "day" | "days" => Some((0, 0, n)),
        "w" | "week" | "weeks" => Some((0, 0, n.checked_mul(7)?)),
        "m" | "mo" | "month" | "months" => Some((0, n, 0)),
        "y" | "year" | "years" => Some((n, 0, 0)),
        _ => None,
    }
}

/// Parse a HOCON byte-size string into a byte count.
///
/// Accepts `[ws] number [ws] [unit] [ws]`. When no unit is present the default is
/// **bytes** (HOCON.md L1341: "bare numbers are taken to be in bytes").
///
/// Fractional values are accepted and **truncated** (not rounded) per Lightbend's
/// `BigDecimal.toBigInteger()` semantics (e.g. `"1024.5"` → 1024 bytes).
///
/// Note: `get_bytes` rejects negative results at the accessor level (ub04 / Lightbend
/// `getBytesBigInteger` positive-only invariant). `parse_bytes` itself allows negative
/// integer inputs.
fn parse_bytes(s: &str) -> Option<i64> {
    let s = trim_hocon_ws(s);
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    let unit_str = trim_hocon_ws(&s[num_end..]);

    if num_str.is_empty() {
        return None;
    }

    // Case-sensitive matching: KB vs KiB matters. Short forms (K, M, G, T) are
    // treated as SI decimal units (KB, MB, GB, TB) per HOCON.md L1344.
    let multiplier: i64 = match unit_str {
        "" | "B" | "byte" | "bytes" => 1,
        "K" | "KB" | "kilobyte" | "kilobytes" => 1_000,
        "KiB" | "kibibyte" | "kibibytes" => 1_024,
        "M" | "MB" | "megabyte" | "megabytes" => 1_000_000,
        "MiB" | "mebibyte" | "mebibytes" => 1_048_576,
        "G" | "GB" | "gigabyte" | "gigabytes" => 1_000_000_000,
        "GiB" | "gibibyte" | "gibibytes" => 1_073_741_824,
        "T" | "TB" | "terabyte" | "terabytes" => 1_000_000_000_000,
        "TiB" | "tebibyte" | "tebibytes" => 1_099_511_627_776,
        _ => return None,
    };

    // Integer fast-path: lossless, avoids any floating-point rounding.
    if is_integer_str(num_str) {
        let n: i64 = num_str.parse().ok()?;
        return n.checked_mul(multiplier);
    }

    // Fractional fallback: truncate toward zero per Lightbend `BigDecimal.toBigInteger()`.
    let f: f64 = num_str.parse().ok()?;
    let result = (f * multiplier as f64) as i64; // `as i64` truncates (Rust saturates on overflow)
    if !f.is_finite() || f.abs() * multiplier as f64 > i64::MAX as f64 {
        return None;
    }
    Some(result)
}

fn missing(path: &str) -> ConfigError {
    ConfigError {
        message: "key not found".to_string(),
        path: path.to_string(),
    }
}

fn type_mismatch(path: &str, expected: &str) -> ConfigError {
    ConfigError {
        message: format!("expected {}", expected),
        path: path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{HoconValue, ScalarValue};
    use indexmap::IndexMap;

    fn make_config(entries: Vec<(&str, HoconValue)>) -> Config {
        let mut map = IndexMap::new();
        for (k, v) in entries {
            map.insert(k.to_string(), v);
        }
        Config::new(map)
    }

    fn sv(s: &str) -> HoconValue {
        HoconValue::Scalar(ScalarValue::string(s.into()))
    }
    fn iv(n: i64) -> HoconValue {
        HoconValue::Scalar(ScalarValue::number(n.to_string()))
    }
    fn fv(n: f64) -> HoconValue {
        HoconValue::Scalar(ScalarValue::number(n.to_string()))
    }
    fn bv(b: bool) -> HoconValue {
        HoconValue::Scalar(ScalarValue::boolean(b))
    }

    #[test]
    fn get_returns_value_at_path() {
        let c = make_config(vec![("host", sv("localhost"))]);
        assert!(c.get("host").is_some());
    }

    #[test]
    fn get_returns_none_for_missing() {
        let c = make_config(vec![]);
        assert!(c.get("missing").is_none());
    }

    #[test]
    fn get_string_returns_string() {
        let c = make_config(vec![("host", sv("localhost"))]);
        assert_eq!(c.get_string("host").unwrap(), "localhost");
    }

    #[test]
    fn get_string_coerces_int() {
        let c = make_config(vec![("port", iv(8080))]);
        assert_eq!(c.get_string("port").unwrap(), "8080");
    }

    #[test]
    fn get_string_coerces_float() {
        let c = make_config(vec![("ratio", fv(3.14))]);
        // f64::to_string may produce "3.14" or similar; just check it parses back
        let s = c.get_string("ratio").unwrap();
        let v: f64 = s.parse().unwrap();
        assert!((v - 3.14).abs() < 1e-10);
    }

    #[test]
    fn get_string_coerces_bool() {
        let c = make_config(vec![("flag", bv(true))]);
        assert_eq!(c.get_string("flag").unwrap(), "true");
    }

    #[test]
    fn get_string_coerces_null() {
        let c = make_config(vec![("v", HoconValue::Scalar(ScalarValue::null()))]);
        assert_eq!(c.get_string("v").unwrap(), "null");
    }

    #[test]
    fn get_string_error_on_object() {
        let mut inner = IndexMap::new();
        inner.insert("x".into(), iv(1));
        let c = make_config(vec![("obj", HoconValue::Object(inner))]);
        assert!(c.get_string("obj").is_err());
    }

    #[test]
    fn get_i64_returns_number() {
        let c = make_config(vec![("port", iv(8080))]);
        assert_eq!(c.get_i64("port").unwrap(), 8080);
    }

    #[test]
    fn get_i64_coerces_numeric_string() {
        let c = make_config(vec![("port", sv("9999"))]);
        assert_eq!(c.get_i64("port").unwrap(), 9999);
    }

    #[test]
    fn get_i64_error_on_non_numeric() {
        let c = make_config(vec![("host", sv("localhost"))]);
        assert!(c.get_i64("host").is_err());
    }

    #[test]
    fn get_i64_error_on_overflow() {
        // "1e20" parses as f64 but overflows i64 range
        let c = make_config(vec![("big", sv("1e20"))]);
        assert!(c.get_i64("big").is_err());
    }

    #[test]
    fn get_i64_error_on_i64_max_plus_one() {
        // 9223372036854775808 == i64::MAX + 1, parses as f64 but must not saturate
        let c = make_config(vec![("big", sv("9223372036854775808"))]);
        assert!(c.get_i64("big").is_err());
    }

    #[test]
    fn get_f64_returns_float() {
        let c = make_config(vec![("rate", fv(3.14))]);
        assert!((c.get_f64("rate").unwrap() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn get_f64_coerces_numeric_string() {
        let c = make_config(vec![("rate", sv("3.14"))]);
        assert!((c.get_f64("rate").unwrap() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn get_bool_returns_bool() {
        let c = make_config(vec![("debug", bv(true))]);
        assert!(c.get_bool("debug").unwrap());
    }

    #[test]
    fn get_bool_coerces_string_true() {
        let c = make_config(vec![("debug", sv("true"))]);
        assert!(c.get_bool("debug").unwrap());
    }

    #[test]
    fn get_bool_coerces_string_false() {
        let c = make_config(vec![("debug", sv("false"))]);
        assert!(!c.get_bool("debug").unwrap());
    }

    #[test]
    fn get_bool_coerces_yes_no_on_off() {
        let c1 = make_config(vec![("v", sv("yes"))]);
        assert!(c1.get_bool("v").unwrap());
        let c2 = make_config(vec![("v", sv("no"))]);
        assert!(!c2.get_bool("v").unwrap());
        let c3 = make_config(vec![("v", sv("on"))]);
        assert!(c3.get_bool("v").unwrap());
        let c4 = make_config(vec![("v", sv("off"))]);
        assert!(!c4.get_bool("v").unwrap());
    }

    #[test]
    fn get_bool_is_case_insensitive() {
        let c = make_config(vec![("v", sv("TRUE"))]);
        assert!(c.get_bool("v").unwrap());
        let c2 = make_config(vec![("v", sv("Off"))]);
        assert!(!c2.get_bool("v").unwrap());
    }

    #[test]
    fn get_bool_error_on_non_boolean() {
        let c = make_config(vec![("v", sv("maybe"))]);
        assert!(c.get_bool("v").is_err());
    }

    #[test]
    fn has_returns_true_for_existing() {
        let c = make_config(vec![("host", sv("localhost"))]);
        assert!(c.has("host"));
    }

    #[test]
    fn has_returns_false_for_missing() {
        let c = make_config(vec![]);
        assert!(!c.has("missing"));
    }

    #[test]
    fn keys_returns_in_order() {
        let c = make_config(vec![("b", iv(2)), ("a", iv(1))]);
        assert_eq!(c.keys(), vec!["b", "a"]);
    }

    #[test]
    fn get_nested_dot_path() {
        let mut inner = IndexMap::new();
        inner.insert("host".into(), sv("localhost"));
        let c = make_config(vec![("server", HoconValue::Object(inner))]);
        assert_eq!(c.get_string("server.host").unwrap(), "localhost");
    }

    #[test]
    fn get_config_returns_sub_config() {
        let mut inner = IndexMap::new();
        inner.insert("host".into(), sv("localhost"));
        let c = make_config(vec![("server", HoconValue::Object(inner))]);
        let sub = c.get_config("server").unwrap();
        assert_eq!(sub.get_string("host").unwrap(), "localhost");
    }

    #[test]
    fn get_list_returns_array() {
        let items = vec![iv(1), iv(2), iv(3)];
        let c = make_config(vec![("list", HoconValue::Array(items))]);
        let list = c.get_list("list").unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn with_fallback_receiver_wins() {
        let c1 = make_config(vec![("host", sv("prod"))]);
        let c2 = make_config(vec![("host", sv("dev")), ("port", iv(8080))]);
        let merged = c1.with_fallback(&c2);
        assert_eq!(merged.get_string("host").unwrap(), "prod");
        assert_eq!(merged.get_i64("port").unwrap(), 8080);
    }

    #[test]
    fn option_variants_return_none_on_missing() {
        let c = make_config(vec![]);
        assert!(c.get_string_option("x").is_none());
        assert!(c.get_i64_option("x").is_none());
        assert!(c.get_f64_option("x").is_none());
        assert!(c.get_bool_option("x").is_none());
    }

    #[test]
    fn get_duration_nanoseconds() {
        let c = make_config(vec![("t", sv("100 ns"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_nanos(100)
        );
    }

    #[test]
    fn get_duration_milliseconds() {
        let c = make_config(vec![("t", sv("500 ms"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_millis(500)
        );
    }

    #[test]
    fn get_duration_seconds() {
        let c = make_config(vec![("t", sv("30 seconds"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_secs(30)
        );
    }

    #[test]
    fn get_duration_minutes() {
        let c = make_config(vec![("t", sv("5 m"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_secs(300)
        );
    }

    #[test]
    fn get_duration_hours() {
        let c = make_config(vec![("t", sv("2 hours"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_secs(7200)
        );
    }

    #[test]
    fn get_duration_days() {
        let c = make_config(vec![("t", sv("1 d"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_secs(86400)
        );
    }

    #[test]
    fn get_duration_fractional() {
        let c = make_config(vec![("t", sv("1.5 hours"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_secs(5400)
        );
    }

    #[test]
    fn get_duration_no_space() {
        let c = make_config(vec![("t", sv("100ms"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_millis(100)
        );
    }

    #[test]
    fn get_duration_singular_unit() {
        let c = make_config(vec![("t", sv("1 second"))]);
        assert_eq!(
            c.get_duration("t").unwrap(),
            std::time::Duration::from_secs(1)
        );
    }

    #[test]
    fn get_duration_error_invalid_unit() {
        let c = make_config(vec![("t", sv("100 foos"))]);
        assert!(c.get_duration("t").is_err());
    }

    #[test]
    fn get_duration_option_missing() {
        let c = make_config(vec![]);
        assert!(c.get_duration_option("t").is_none());
    }

    #[test]
    fn get_bytes_plain() {
        let c = make_config(vec![("s", sv("100 B"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 100);
    }

    #[test]
    fn get_bytes_kilobytes() {
        let c = make_config(vec![("s", sv("10 KB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 10_000);
    }

    #[test]
    fn get_bytes_kibibytes() {
        let c = make_config(vec![("s", sv("1 KiB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 1_024);
    }

    #[test]
    fn get_bytes_megabytes() {
        let c = make_config(vec![("s", sv("5 MB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 5_000_000);
    }

    #[test]
    fn get_bytes_mebibytes() {
        let c = make_config(vec![("s", sv("1 MiB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 1_048_576);
    }

    #[test]
    fn get_bytes_gigabytes() {
        let c = make_config(vec![("s", sv("2 GB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 2_000_000_000);
    }

    #[test]
    fn get_bytes_gibibytes() {
        let c = make_config(vec![("s", sv("1 GiB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 1_073_741_824);
    }

    #[test]
    fn get_bytes_terabytes() {
        let c = make_config(vec![("s", sv("1 TB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 1_000_000_000_000);
    }

    #[test]
    fn get_bytes_tebibytes() {
        let c = make_config(vec![("s", sv("1 TiB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 1_099_511_627_776);
    }

    #[test]
    fn get_bytes_no_space() {
        let c = make_config(vec![("s", sv("512MB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 512_000_000);
    }

    #[test]
    fn get_bytes_long_unit() {
        let c = make_config(vec![("s", sv("2 megabytes"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 2_000_000);
    }

    #[test]
    fn get_bytes_error_invalid_unit() {
        let c = make_config(vec![("s", sv("100 XB"))]);
        assert!(c.get_bytes("s").is_err());
    }

    #[test]
    fn get_bytes_option_missing() {
        let c = make_config(vec![]);
        assert!(c.get_bytes_option("s").is_none());
    }

    #[test]
    fn get_bytes_fractional_rounds() {
        // 1.5 KiB = 1536 bytes exactly; rounding should not change it
        let c = make_config(vec![("s", sv("1.5 KiB"))]);
        assert_eq!(c.get_bytes("s").unwrap(), 1536);
    }

    // ──────────────────────────────────────────────────────────────
    // Unit B — parse_duration bare/fractional/ws tests (RED phase)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_duration_bare_integer_uses_ms_default() {
        assert_eq!(
            parse_duration("500"),
            Some(std::time::Duration::from_millis(500))
        );
    }
    #[test]
    fn parse_duration_leading_ws_bare() {
        assert_eq!(
            parse_duration(" 500"),
            Some(std::time::Duration::from_millis(500))
        );
    }
    #[test]
    fn parse_duration_trailing_ws_bare() {
        assert_eq!(
            parse_duration("500 "),
            Some(std::time::Duration::from_millis(500))
        );
    }
    #[test]
    fn parse_duration_both_ws_bare() {
        assert_eq!(
            parse_duration(" 500 "),
            Some(std::time::Duration::from_millis(500))
        );
    }
    #[test]
    fn parse_duration_fractional_bare_uses_nanos() {
        let d = parse_duration("500.5").unwrap();
        assert_eq!(d.as_nanos(), 500_500_000);
    }
    #[test]
    fn parse_duration_empty_is_none() {
        assert!(parse_duration("").is_none());
    }
    #[test]
    fn parse_duration_ws_only_is_none() {
        assert!(parse_duration("   ").is_none());
    }
    #[test]
    fn parse_duration_unit_only_is_none() {
        assert!(parse_duration("ms").is_none());
    }

    // ──────────────────────────────────────────────────────────────
    // Unit C — parse_bytes ws / truncate / negative-accessor (RED)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_bytes_leading_trailing_ws_bare() {
        assert_eq!(parse_bytes(" 1024 "), Some(1024));
    }
    #[test]
    fn parse_bytes_fractional_truncated() {
        assert_eq!(parse_bytes("1024.5"), Some(1024));
    }
    #[test]
    fn get_bytes_negative_accessor_rejects() {
        use std::collections::HashMap;
        let cfg = crate::parse_with_env(r#"b = "-1""#, &HashMap::new()).unwrap();
        assert!(
            cfg.get_bytes("b").is_err(),
            "ub04: negative byte size must error at accessor"
        );
    }

    // ──────────────────────────────────────────────────────────────
    // Unit D — parse_period (RED phase)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_period_bare_integer_uses_days_default() {
        assert_eq!(parse_period("7"), Some((0, 0, 7)));
    }
    #[test]
    fn parse_period_leading_trailing_ws() {
        assert_eq!(parse_period(" 7 "), Some((0, 0, 7)));
    }
    #[test]
    fn parse_period_fractional_rejected() {
        assert!(parse_period("7.5").is_none());
    }
    #[test]
    fn parse_period_negative_allowed() {
        assert_eq!(parse_period("-7"), Some((0, 0, -7)));
    }
    #[test]
    fn parse_period_weeks_unit() {
        assert_eq!(parse_period("7w"), Some((0, 0, 49)));
    }
    #[test]
    fn parse_period_months_unit() {
        assert_eq!(parse_period("3m"), Some((0, 3, 0)));
    }
    #[test]
    fn parse_period_years_unit() {
        assert_eq!(parse_period("2y"), Some((2, 0, 0)));
    }
    #[test]
    fn parse_period_days_explicit() {
        assert_eq!(parse_period("5d"), Some((0, 0, 5)));
    }
    #[test]
    fn parse_period_empty_is_none() {
        assert!(parse_period("").is_none());
    }

    #[test]
    fn split_config_path_consecutive_dots_preserve_empty() {
        let segs = split_config_path("a..b");
        assert_eq!(segs, vec!["a", "", "b"]);
    }

    #[test]
    fn split_config_path_trailing_dot_empty_segment() {
        let segs = split_config_path("a.b.");
        assert_eq!(segs, vec!["a", "b", ""]);
    }

    #[test]
    fn split_config_path_quoted_escape() {
        // "a\"b" as a path key should produce the key: a"b
        let segs = split_config_path(r#""a\"b""#);
        assert_eq!(segs, vec!["a\"b"]);
    }

    #[test]
    fn split_config_path_quoted_with_dot() {
        let segs = split_config_path(r#"server."web.api".port"#);
        assert_eq!(segs, vec!["server", "web.api", "port"]);
    }
}
