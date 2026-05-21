use crate::numeric_array::numeric_object_to_array;
use crate::value::{HoconValue, ScalarType, ScalarValue};
use indexmap::IndexMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[non_exhaustive]
#[derive(Debug)]
pub struct DeserializeError {
    pub message: String,
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HOCON deserialization error: {}", self.message)
    }
}

impl std::error::Error for DeserializeError {}

impl ::serde::de::Error for DeserializeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        DeserializeError {
            message: msg.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Deserializer
// ---------------------------------------------------------------------------

pub(crate) struct HoconDeserializer<'de> {
    value: &'de HoconValue,
}

impl<'de> HoconDeserializer<'de> {
    pub(crate) fn new(value: &'de HoconValue) -> Self {
        Self { value }
    }
}

/// Helper: parse raw string as integer type with float truncation fallback.
fn parse_int_from_scalar<T>(sv: &ScalarValue, type_name: &str) -> Result<T, DeserializeError>
where
    T: std::str::FromStr + TryFrom<i64>,
    <T as std::str::FromStr>::Err: fmt::Display,
    <T as TryFrom<i64>>::Error: fmt::Display,
{
    // Try direct parse
    if let Ok(n) = sv.raw.parse::<T>() {
        return Ok(n);
    }
    // Float truncation fallback for Number types — only for float-like literals
    if sv.value_type == ScalarType::Number {
        let is_float_like = sv.raw.contains('.') || sv.raw.contains('e') || sv.raw.contains('E');
        if is_float_like {
            if let Ok(f) = sv.raw.parse::<f64>() {
                if f.fract() == 0.0
                    && f.is_finite()
                    && f >= i64::MIN as f64
                    && f < (i64::MAX as f64)
                {
                    let as_i64 = f as i64;
                    if let Ok(n) = T::try_from(as_i64) {
                        return Ok(n);
                    }
                }
            }
        }
    }
    Err(DeserializeError {
        message: format!("cannot parse \"{}\" as {}", sv.raw, type_name),
    })
}

macro_rules! deserialize_int {
    ($method:ident, $visit:ident, $ty:ty) => {
        fn $method<V: ::serde::de::Visitor<'de>>(
            self,
            visitor: V,
        ) -> Result<V::Value, Self::Error> {
            match self.value {
                HoconValue::Scalar(sv) => {
                    let n: $ty = parse_int_from_scalar(sv, stringify!($ty))?;
                    visitor.$visit(n)
                }
                other => Err(DeserializeError {
                    message: format!("expected {}, got {:?}", stringify!($ty), other),
                }),
            }
        }
    };
}

impl<'de> ::serde::Deserializer<'de> for HoconDeserializer<'de> {
    type Error = DeserializeError;

    fn deserialize_any<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => match sv.value_type {
                ScalarType::Null => visitor.visit_unit(),
                ScalarType::Boolean => visitor.visit_bool(sv.raw == "true"),
                ScalarType::Number => {
                    // Try i64 first (no dot/exponent), then f64
                    if !sv.raw.contains('.') && !sv.raw.contains('e') && !sv.raw.contains('E') {
                        if let Ok(n) = sv.raw.parse::<i64>() {
                            return visitor.visit_i64(n);
                        }
                    }
                    if let Ok(f) = sv.raw.parse::<f64>() {
                        return visitor.visit_f64(f);
                    }
                    visitor.visit_string(sv.raw.clone())
                }
                ScalarType::String => visitor.visit_string(sv.raw.clone()),
            },
            HoconValue::Object(map) => visitor.visit_map(HoconMapAccess::new(map)),
            HoconValue::Array(items) => visitor.visit_seq(HoconSeqAccess::new(items)),
            HoconValue::Placeholder(pv) => Err(DeserializeError {
                message: format!(
                    "cannot deserialize unresolved substitution at path {:?}: call Config::resolve() first",
                    pv.path
                ),
            }),
        }
    }

    fn deserialize_bool<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => match sv.raw.to_lowercase().as_str() {
                "true" | "yes" | "on" => visitor.visit_bool(true),
                "false" | "no" | "off" => visitor.visit_bool(false),
                _ => Err(DeserializeError {
                    message: format!("cannot coerce \"{}\" to bool", sv.raw),
                }),
            },
            other => Err(DeserializeError {
                message: format!("expected bool, got {:?}", other),
            }),
        }
    }

    deserialize_int!(deserialize_i8, visit_i8, i8);
    deserialize_int!(deserialize_i16, visit_i16, i16);
    deserialize_int!(deserialize_i32, visit_i32, i32);
    deserialize_int!(deserialize_i64, visit_i64, i64);
    deserialize_int!(deserialize_u8, visit_u8, u8);
    deserialize_int!(deserialize_u16, visit_u16, u16);
    deserialize_int!(deserialize_u32, visit_u32, u32);
    deserialize_int!(deserialize_u64, visit_u64, u64);

    fn deserialize_f32<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => {
                let f: f32 = sv.raw.parse().map_err(|_| DeserializeError {
                    message: format!("cannot parse \"{}\" as f32", sv.raw),
                })?;
                visitor.visit_f32(f)
            }
            other => Err(DeserializeError {
                message: format!("expected f32, got {:?}", other),
            }),
        }
    }

    fn deserialize_f64<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => {
                let f: f64 = sv.raw.parse().map_err(|_| DeserializeError {
                    message: format!("cannot parse \"{}\" as f64", sv.raw),
                })?;
                visitor.visit_f64(f)
            }
            other => Err(DeserializeError {
                message: format!("expected f64, got {:?}", other),
            }),
        }
    }

    fn deserialize_char<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => {
                let mut chars = sv.raw.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => visitor.visit_char(c),
                    _ => Err(DeserializeError {
                        message: format!("expected single char, got \"{}\"", sv.raw),
                    }),
                }
            }
            other => Err(DeserializeError {
                message: format!("expected char, got {:?}", other),
            }),
        }
    }

    fn deserialize_str<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => visitor.visit_string(sv.raw.clone()),
            other => Err(DeserializeError {
                message: format!("expected string, got {:?}", other),
            }),
        }
    }

    fn deserialize_bytes<V: ::serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(DeserializeError {
            message: "HOCON does not support byte arrays".into(),
        })
    }

    fn deserialize_byte_buf<V: ::serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(DeserializeError {
            message: "HOCON does not support byte arrays".into(),
        })
    }

    fn deserialize_option<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) if sv.value_type == ScalarType::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) if sv.value_type == ScalarType::Null => visitor.visit_unit(),
            other => Err(DeserializeError {
                message: format!("expected null/unit, got {:?}", other),
            }),
        }
    }

    fn deserialize_unit_struct<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Array(items) => visitor.visit_seq(HoconSeqAccess::new(items)),
            // S15 §"Accessor behaviour" L160-161: every typed-array accessor must invoke
            // numeric_object_to_array. If the object has at least one eligible integer key,
            // convert and deserialize as a sequence; otherwise fall through to the type-mismatch
            // error so serde receives a meaningful message.
            obj @ HoconValue::Object(_) => match numeric_object_to_array(obj) {
                Some(items) => visitor.visit_seq(HoconOwnedSeqAccess::new(items)),
                None => Err(DeserializeError {
                    message: format!("expected array, got {:?}", obj),
                }),
            },
            other => Err(DeserializeError {
                message: format!("expected array, got {:?}", other),
            }),
        }
    }

    fn deserialize_tuple<V: ::serde::de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Object(map) => visitor.visit_map(HoconMapAccess::new(map)),
            other => Err(DeserializeError {
                message: format!("expected object, got {:?}", other),
            }),
        }
    }

    fn deserialize_struct<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => visitor.visit_enum(sv.raw.as_str().into_deserializer()),
            other => Err(DeserializeError {
                message: format!("expected string for enum variant, got {:?}", other),
            }),
        }
    }

    fn deserialize_identifier<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_unit()
    }
}

use ::serde::de::IntoDeserializer;

// ---------------------------------------------------------------------------
// MapAccess
// ---------------------------------------------------------------------------

pub(crate) struct HoconMapAccess<'de> {
    iter: indexmap::map::Iter<'de, String, HoconValue>,
    current_value: Option<&'de HoconValue>,
}

impl<'de> HoconMapAccess<'de> {
    fn new(map: &'de IndexMap<String, HoconValue>) -> Self {
        Self {
            iter: map.iter(),
            current_value: None,
        }
    }
}

impl<'de> ::serde::de::MapAccess<'de> for HoconMapAccess<'de> {
    type Error = DeserializeError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: ::serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.current_value = Some(value);
                seed.deserialize(StringKeyDeserializer { key }).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: ::serde::de::DeserializeSeed<'de>,
    {
        let value = self.current_value.take().ok_or_else(|| DeserializeError {
            message: "next_value_seed called before next_key_seed".into(),
        })?;
        seed.deserialize(HoconDeserializer::new(value))
    }
}

// A simple key deserializer that yields a string.
struct StringKeyDeserializer<'a> {
    key: &'a str,
}

impl<'de, 'a> ::serde::Deserializer<'de> for StringKeyDeserializer<'a> {
    type Error = DeserializeError;

    fn deserialize_any<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_str(self.key)
    }

    ::serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// ---------------------------------------------------------------------------
// SeqAccess
// ---------------------------------------------------------------------------

pub(crate) struct HoconSeqAccess<'de> {
    iter: std::slice::Iter<'de, HoconValue>,
}

impl<'de> HoconSeqAccess<'de> {
    fn new(items: &'de [HoconValue]) -> Self {
        Self { iter: items.iter() }
    }
}

impl<'de> ::serde::de::SeqAccess<'de> for HoconSeqAccess<'de> {
    type Error = DeserializeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: ::serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(HoconDeserializer::new(value)).map(Some),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// OwnedSeqAccess — for sequences built from owned Vec<HoconValue>
// (used when numeric_object_to_array produces a fresh Vec)
// ---------------------------------------------------------------------------

pub(crate) struct HoconOwnedSeqAccess {
    /// Items in their original (forward) order.
    items: Vec<HoconValue>,
    /// Current index into items; advances on each `next_element_seed` call.
    idx: usize,
}

impl HoconOwnedSeqAccess {
    fn new(items: Vec<HoconValue>) -> Self {
        Self { items, idx: 0 }
    }
}

impl<'de> ::serde::de::SeqAccess<'de> for HoconOwnedSeqAccess {
    type Error = DeserializeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: ::serde::de::DeserializeSeed<'de>,
    {
        if self.idx >= self.items.len() {
            return Ok(None);
        }
        // Clone the value at idx so OwnedHoconDeserializer can own it.
        // The clone is bounded by the element count (typically small: O(n) where n
        // is the number of numeric keys in the source object).
        let value = self.items[self.idx].clone();
        self.idx += 1;
        seed.deserialize(OwnedHoconDeserializer { value }).map(Some)
    }
}

/// A variant of HoconDeserializer that owns its value (no lifetime parameter).
/// Used exclusively by HoconOwnedSeqAccess to avoid the lifetime issue when
/// deserializing from an owned Vec<HoconValue>.
///
/// This duplicates some deserialize methods from HoconDeserializer; the
/// duplication is justified by the need to avoid self-referential borrows.
struct OwnedHoconDeserializer {
    value: HoconValue,
}

impl<'de> ::serde::Deserializer<'de> for OwnedHoconDeserializer {
    type Error = DeserializeError;

    fn deserialize_any<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => match sv.value_type {
                ScalarType::Null => visitor.visit_unit(),
                ScalarType::Boolean => visitor.visit_bool(sv.raw == "true"),
                ScalarType::Number => {
                    if !sv.raw.contains('.') && !sv.raw.contains('e') && !sv.raw.contains('E') {
                        if let Ok(n) = sv.raw.parse::<i64>() {
                            return visitor.visit_i64(n);
                        }
                    }
                    if let Ok(f) = sv.raw.parse::<f64>() {
                        return visitor.visit_f64(f);
                    }
                    visitor.visit_string(sv.raw)
                }
                ScalarType::String => visitor.visit_string(sv.raw),
            },
            HoconValue::Object(map) => visitor.visit_map(OwnedHoconMapAccess::new(map)),
            HoconValue::Array(items) => visitor.visit_seq(HoconOwnedSeqAccess::new(items)),
            HoconValue::Placeholder(pv) => Err(DeserializeError {
                message: format!(
                    "cannot deserialize unresolved substitution at path {:?}: call Config::resolve() first",
                    pv.path
                ),
            }),
        }
    }

    fn deserialize_string<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => visitor.visit_string(sv.raw),
            other => Err(DeserializeError {
                message: format!("expected string, got {:?}", other),
            }),
        }
    }

    fn deserialize_str<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_bool<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => match sv.raw.to_lowercase().as_str() {
                "true" | "yes" | "on" => visitor.visit_bool(true),
                "false" | "no" | "off" => visitor.visit_bool(false),
                _ => Err(DeserializeError {
                    message: format!("cannot coerce \"{}\" to bool", sv.raw),
                }),
            },
            other => Err(DeserializeError {
                message: format!("expected bool, got {:?}", other),
            }),
        }
    }

    fn deserialize_i64<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => {
                let n: i64 = parse_int_from_scalar(&sv, "i64")?;
                visitor.visit_i64(n)
            }
            other => Err(DeserializeError {
                message: format!("expected i64, got {:?}", other),
            }),
        }
    }

    fn deserialize_u64<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => {
                let n: u64 = parse_int_from_scalar(&sv, "u64")?;
                visitor.visit_u64(n)
            }
            other => Err(DeserializeError {
                message: format!("expected u64, got {:?}", other),
            }),
        }
    }

    fn deserialize_f64<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) => {
                let f: f64 = sv.raw.parse().map_err(|_| DeserializeError {
                    message: format!("cannot parse \"{}\" as f64", sv.raw),
                })?;
                visitor.visit_f64(f)
            }
            other => Err(DeserializeError {
                message: format!("expected f64, got {:?}", other),
            }),
        }
    }

    fn deserialize_seq<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Array(items) => visitor.visit_seq(HoconOwnedSeqAccess::new(items)),
            obj @ HoconValue::Object(_) => match numeric_object_to_array(&obj) {
                Some(items) => visitor.visit_seq(HoconOwnedSeqAccess::new(items)),
                None => Err(DeserializeError {
                    message: format!("expected array, got {:?}", obj),
                }),
            },
            other => Err(DeserializeError {
                message: format!("expected array, got {:?}", other),
            }),
        }
    }

    fn deserialize_option<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match &self.value {
            HoconValue::Scalar(sv) if sv.value_type == ScalarType::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Scalar(sv) if sv.value_type == ScalarType::Null => visitor.visit_unit(),
            other => Err(DeserializeError {
                message: format!("expected null/unit, got {:?}", other),
            }),
        }
    }

    fn deserialize_newtype_struct<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_tuple<V: ::serde::de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            HoconValue::Object(map) => visitor.visit_map(OwnedHoconMapAccess::new(map)),
            other => Err(DeserializeError {
                message: format!("expected object, got {:?}", other),
            }),
        }
    }

    fn deserialize_struct<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: ::serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        // Mirror HoconDeserializer::deserialize_enum (line ~345) so that converted
        // numeric-keyed-object → array elements can deserialize as string enum variants.
        match self.value {
            HoconValue::Scalar(sv) => visitor.visit_enum(sv.raw.as_str().into_deserializer()),
            other => Err(DeserializeError {
                message: format!("expected string for enum variant, got {:?}", other),
            }),
        }
    }

    fn deserialize_identifier<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V: ::serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_unit()
    }

    ::serde::forward_to_deserialize_any! {
        i8 i16 i32 u8 u16 u32 f32 char bytes byte_buf unit_struct
    }
}

// ---------------------------------------------------------------------------
// OwnedMapAccess — MapAccess that owns its IndexMap (used by OwnedHoconDeserializer)
// ---------------------------------------------------------------------------

pub(crate) struct OwnedHoconMapAccess {
    iter: indexmap::map::IntoIter<String, HoconValue>,
    current_value: Option<HoconValue>,
}

impl OwnedHoconMapAccess {
    fn new(map: IndexMap<String, HoconValue>) -> Self {
        Self {
            iter: map.into_iter(),
            current_value: None,
        }
    }
}

impl<'de> ::serde::de::MapAccess<'de> for OwnedHoconMapAccess {
    type Error = DeserializeError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: ::serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.current_value = Some(value);
                seed.deserialize(key.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: ::serde::de::DeserializeSeed<'de>,
    {
        let value = self.current_value.take().ok_or_else(|| DeserializeError {
            message: "next_value_seed called before next_key_seed".into(),
        })?;
        seed.deserialize(OwnedHoconDeserializer { value })
    }
}
