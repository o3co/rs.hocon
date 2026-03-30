use crate::error::ConfigError;
use crate::value::{HoconValue, ScalarValue};
use indexmap::IndexMap;

pub struct Config {
    root: IndexMap<String, HoconValue>,
}

impl Config {
    pub fn new(root: IndexMap<String, HoconValue>) -> Self {
        Self { root }
    }

    // Walk the dot-separated path through nested objects.
    fn lookup_node(&self, path: &str) -> Option<&HoconValue> {
        let mut parts = path.splitn(2, '.');
        let key = parts.next()?;
        let rest = parts.next();

        let value = self.root.get(key)?;

        match rest {
            None => Some(value),
            Some(remaining) => match value {
                HoconValue::Object(map) => lookup_in_map(map, remaining),
                _ => None,
            },
        }
    }

    // Raw access
    pub fn get(&self, path: &str) -> Option<&HoconValue> {
        self.lookup_node(path)
    }

    // Typed getters

    pub fn get_string(&self, path: &str) -> Result<String, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(ScalarValue::String(s))) => Ok(s.clone()),
            _ => Err(type_mismatch(path, "String")),
        }
    }

    pub fn get_i64(&self, path: &str) -> Result<i64, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(ScalarValue::Int(n))) => Ok(*n),
            Some(HoconValue::Scalar(ScalarValue::Float(f))) => {
                // Only accept whole numbers
                if f.fract() == 0.0 && f.is_finite() {
                    Ok(*f as i64)
                } else {
                    Err(type_mismatch(path, "i64"))
                }
            }
            Some(HoconValue::Scalar(ScalarValue::String(s))) => {
                // Strict parse: no hex, no leading/trailing whitespace
                s.parse::<i64>().map_err(|_| type_mismatch(path, "i64"))
            }
            _ => Err(type_mismatch(path, "i64")),
        }
    }

    pub fn get_f64(&self, path: &str) -> Result<f64, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(ScalarValue::Float(f))) => Ok(*f),
            Some(HoconValue::Scalar(ScalarValue::Int(n))) => Ok(*n as f64),
            Some(HoconValue::Scalar(ScalarValue::String(s))) => {
                s.parse::<f64>().map_err(|_| type_mismatch(path, "f64"))
            }
            _ => Err(type_mismatch(path, "f64")),
        }
    }

    pub fn get_bool(&self, path: &str) -> Result<bool, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(ScalarValue::Bool(b))) => Ok(*b),
            Some(HoconValue::Scalar(ScalarValue::String(s))) => match s.to_lowercase().as_str() {
                "true" | "yes" | "on" => Ok(true),
                "false" | "no" | "off" => Ok(false),
                _ => Err(type_mismatch(path, "bool")),
            },
            _ => Err(type_mismatch(path, "bool")),
        }
    }

    pub fn get_config(&self, path: &str) -> Result<Config, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Object(map)) => Ok(Config::new(map.clone())),
            _ => Err(type_mismatch(path, "Object")),
        }
    }

    pub fn get_list(&self, path: &str) -> Result<Vec<HoconValue>, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Array(items)) => Ok(items.clone()),
            _ => Err(type_mismatch(path, "Array")),
        }
    }

    // Option variants

    pub fn get_string_option(&self, path: &str) -> Option<String> {
        self.get_string(path).ok()
    }

    pub fn get_i64_option(&self, path: &str) -> Option<i64> {
        self.get_i64(path).ok()
    }

    pub fn get_f64_option(&self, path: &str) -> Option<f64> {
        self.get_f64(path).ok()
    }

    pub fn get_bool_option(&self, path: &str) -> Option<bool> {
        self.get_bool(path).ok()
    }

    pub fn get_config_option(&self, path: &str) -> Option<Config> {
        self.get_config(path).ok()
    }

    pub fn get_list_option(&self, path: &str) -> Option<Vec<HoconValue>> {
        self.get_list(path).ok()
    }

    pub fn get_duration(&self, path: &str) -> Result<std::time::Duration, ConfigError> {
        match self.lookup_node(path) {
            None => Err(missing(path)),
            Some(HoconValue::Scalar(ScalarValue::String(s))) => {
                parse_duration(s).ok_or_else(|| ConfigError {
                    message: format!("invalid duration at {}: {}", path, s),
                    path: path.to_string(),
                })
            }
            Some(HoconValue::Scalar(ScalarValue::Int(n))) => {
                Ok(std::time::Duration::from_millis(*n as u64))
            }
            Some(HoconValue::Scalar(ScalarValue::Float(f))) => {
                Ok(std::time::Duration::from_secs_f64(*f / 1000.0))
            }
            _ => Err(ConfigError {
                message: format!("expected duration at {}", path),
                path: path.to_string(),
            }),
        }
    }

    pub fn get_duration_option(&self, path: &str) -> Option<std::time::Duration> {
        self.get_duration(path).ok()
    }

    pub fn get_bytes(&self, path: &str) -> Result<i64, ConfigError> {
        let v = self.lookup_node(path).ok_or_else(|| ConfigError {
            message: format!("path not found: {}", path),
            path: path.to_string(),
        })?;
        match v {
            HoconValue::Scalar(ScalarValue::String(s)) => {
                parse_bytes(s).ok_or_else(|| ConfigError {
                    message: format!("invalid byte size at {}: {}", path, s),
                    path: path.to_string(),
                })
            }
            HoconValue::Scalar(ScalarValue::Int(n)) => Ok(*n),
            _ => Err(ConfigError {
                message: format!("expected byte size at {}", path),
                path: path.to_string(),
            }),
        }
    }

    pub fn get_bytes_option(&self, path: &str) -> Option<i64> {
        self.get_bytes(path).ok()
    }

    // Inspection

    pub fn has(&self, path: &str) -> bool {
        self.lookup_node(path).is_some()
    }

    pub fn keys(&self) -> Vec<&str> {
        self.root.keys().map(|s| s.as_str()).collect()
    }

    // Merge — receiver wins, fallback fills gaps
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

// Free function that walks a map recursively using a dot path, avoiding lifetime issues
// in methods that would need to return references into temporary Config values.
fn lookup_in_map<'a>(map: &'a IndexMap<String, HoconValue>, path: &str) -> Option<&'a HoconValue> {
    let mut parts = path.splitn(2, '.');
    let key = parts.next()?;
    let rest = parts.next();

    let value = map.get(key)?;

    match rest {
        None => Some(value),
        Some(remaining) => match value {
            HoconValue::Object(inner) => lookup_in_map(inner, remaining),
            _ => None,
        },
    }
}

#[cfg(feature = "serde")]
impl Config {
    pub fn deserialize<T: ::serde::de::DeserializeOwned>(
        &self,
    ) -> Result<T, crate::serde::DeserializeError> {
        let value = HoconValue::Object(self.root.clone());
        T::deserialize(crate::serde::HoconDeserializer::new(&value))
    }
}

fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    let unit_str = s[num_end..].trim().to_lowercase();

    let num: f64 = num_str.parse().ok()?;

    let nanos_per_unit: f64 = match unit_str.as_str() {
        "ns" | "nanosecond" | "nanoseconds" => 1.0,
        "us" | "microsecond" | "microseconds" => 1_000.0,
        "ms" | "millisecond" | "milliseconds" => 1_000_000.0,
        "s" | "second" | "seconds" => 1_000_000_000.0,
        "m" | "minute" | "minutes" => 60_000_000_000.0,
        "h" | "hour" | "hours" => 3_600_000_000_000.0,
        "d" | "day" | "days" => 86_400_000_000_000.0,
        _ => return None,
    };

    Some(std::time::Duration::from_nanos(
        (num * nanos_per_unit) as u64,
    ))
}

fn parse_bytes(s: &str) -> Option<i64> {
    let s = s.trim();
    let num_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    let unit_str = s[num_end..].trim();

    let num: i64 = num_str.parse().ok()?;

    // Case-sensitive matching: KB vs KiB matters
    let multiplier: i64 = match unit_str {
        "B" | "byte" | "bytes" => 1,
        "KB" | "kilobyte" | "kilobytes" => 1_000,
        "KiB" | "kibibyte" | "kibibytes" => 1_024,
        "MB" | "megabyte" | "megabytes" => 1_000_000,
        "MiB" | "mebibyte" | "mebibytes" => 1_048_576,
        "GB" | "gigabyte" | "gigabytes" => 1_000_000_000,
        "GiB" | "gibibyte" | "gibibytes" => 1_073_741_824,
        "TB" | "terabyte" | "terabytes" => 1_000_000_000_000,
        "TiB" | "tebibyte" | "tebibytes" => 1_099_511_627_776,
        _ => return None,
    };

    Some(num * multiplier)
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
        HoconValue::Scalar(ScalarValue::String(s.into()))
    }
    fn iv(n: i64) -> HoconValue {
        HoconValue::Scalar(ScalarValue::Int(n))
    }
    fn fv(n: f64) -> HoconValue {
        HoconValue::Scalar(ScalarValue::Float(n))
    }
    fn bv(b: bool) -> HoconValue {
        HoconValue::Scalar(ScalarValue::Bool(b))
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
    fn get_string_error_on_non_string() {
        let c = make_config(vec![("port", iv(8080))]);
        assert!(c.get_string("port").is_err());
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
}
