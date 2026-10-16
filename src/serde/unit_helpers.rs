//! Serde helpers for HOCON unit values — duration (S19), byte size (S21) and
//! period (S20) — for use with `#[serde(with = "...")]`.
//!
//! The helpers read a value with the same rules as the getters
//! ([`Config::get_duration`](crate::Config::get_duration),
//! [`get_bytes`](crate::Config::get_bytes),
//! [`get_period`](crate::Config::get_period)): both go through one reader, so
//! the forms accepted, the default unit of a bare number and the rejected
//! values are the same. This module only gets the scalar out of the
//! deserializer and reports failures.
//!
//! # Getting the source text
//!
//! A getter reads a scalar's source text, and the text matters for numbers:
//! `0.99999999999999999` ms is exactly 999999 ns, while the nearest double
//! is 1.0 (1 ms). A helper only sees a `Deserializer`, so it asks
//! for the text with a private `deserialize_newtype_struct` name that the HOCON
//! deserializers answer with the scalar's text and type; the result is then
//! exactly the getter's.
//!
//! Where serde buffers the input first (`#[serde(flatten)]`, untagged and
//! internally tagged enums), and with any other self-describing deserializer
//! (serde_json, …), the request falls back to the value: a string is read as
//! it is, an integer as an integer and a float as a float (`1.0`, `1e21`), so
//! the helpers accept and reject the same values as the getter. A number
//! reads by its value, not its spelling (E20), so a buffered number reads as
//! the getter reads it. The exception is a number literal that is not an
//! i64 integer and has more than 15 significant digits: it is buffered as
//! the nearest double and reads as that — slightly differently, and near the
//! top of the range the getter can accept what the buffered helper rejects.
//! Likewise a negative number that underflows the double (`-1e-400`) is
//! buffered as `-0.0`, which a duration reads as zero; the getter reports a
//! negative duration.
//!
//! Formats that are not self-describing (bincode, postcard) cannot use the
//! helpers, and serde_json built with `arbitrary_precision` hands over floats
//! in a form they do not read (integers and strings still work).
//!
//! # Requiring the unit
//!
//! Each helper has a `require_unit` sibling (`duration::require_unit`, …)
//! that reads the same values but refuses one that does not name its unit: a
//! number (`ttl = 30`) or a string of the number alone (`"30"`, what an
//! environment variable gives), which would otherwise read in the default
//! unit. The value is read first, so one that does not read keeps its own
//! error; whether a unit is named is decided by the readers' own split
//! ([`crate::units`]). The buffered paths hand over a number as a number and
//! a string as a string, so the check is the same there.

use crate::units::{
    names_magnitude_unit, names_period_unit, read_bytes, read_duration, read_period, UnitError,
};
use crate::value::{ScalarType, ScalarValue};
use ::serde::de::{self, Deserializer, SeqAccess, Visitor};
use std::fmt;
use std::marker::PhantomData;

/// The `deserialize_newtype_struct` name by which the helpers ask a HOCON
/// deserializer for a scalar's source text and type instead of its value.
pub(crate) const RAW_SCALAR: &str = "$hocon::private::RawScalar";

// Type tags of the RAW_SCALAR answer. Namespaced so that a sequence some
// other deserializer hands over is not mistaken for the answer.
const TAG_STRING: &str = "$hocon::private::String";
const TAG_NUMBER: &str = "$hocon::private::Number";
const TAG_BOOLEAN: &str = "$hocon::private::Boolean";
const TAG_NULL: &str = "$hocon::private::Null";

/// Answer a [`RAW_SCALAR`] request: the scalar as a two-element sequence,
/// `[type tag, source text]`.
pub(crate) fn visit_raw_scalar<'de, V, E>(sv: &ScalarValue, visitor: V) -> Result<V::Value, E>
where
    V: Visitor<'de>,
    E: de::Error,
{
    let tag = match sv.value_type {
        ScalarType::String => TAG_STRING,
        ScalarType::Number => TAG_NUMBER,
        ScalarType::Boolean => TAG_BOOLEAN,
        ScalarType::Null => TAG_NULL,
    };
    visitor.visit_seq(de::value::SeqDeserializer::<_, E>::new(
        [tag, sv.raw.as_str()].into_iter(),
    ))
}

/// A scalar as the readers take it: source text plus type.
struct Scalar {
    raw: String,
    ty: ScalarType,
}

struct ScalarVisitor {
    expecting: &'static str,
    /// Whether a sequence may be the RAW_SCALAR answer. The visitor handed to
    /// `deserialize_newtype_struct` accepts one (a deserializer that forwards
    /// newtype structs to `deserialize_any` reaches it here too, which the
    /// namespaced tags guard against); once the request falls back to the
    /// value, a sequence is an ordinary (wrong-typed) array —
    /// `v = ["number", "5"]` must not read as the scalar `5`.
    raw_answer: bool,
}

impl ScalarVisitor {
    fn number(raw: String) -> Scalar {
        Scalar {
            raw,
            ty: ScalarType::Number,
        }
    }
}

impl<'de> Visitor<'de> for ScalarVisitor {
    type Value = Scalar;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a HOCON {}", self.expecting)
    }

    // A HOCON deserializer answers the RAW_SCALAR request here.
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Scalar, A::Error> {
        if !self.raw_answer {
            return Err(de::Error::invalid_type(de::Unexpected::Seq, &self));
        }
        let tag: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let raw: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let ty = match tag.as_str() {
            TAG_STRING => ScalarType::String,
            TAG_NUMBER => ScalarType::Number,
            TAG_BOOLEAN => ScalarType::Boolean,
            TAG_NULL => ScalarType::Null,
            _ => return Err(de::Error::invalid_value(de::Unexpected::Seq, &self)),
        };
        Ok(Scalar { raw, ty })
    }

    // Any other deserializer: read the value it hands over.
    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Scalar, D::Error> {
        d.deserialize_any(ScalarVisitor {
            raw_answer: false,
            ..self
        })
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Scalar, E> {
        Ok(Scalar {
            raw: v.to_string(),
            ty: ScalarType::String,
        })
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Scalar, E> {
        Ok(Scalar {
            raw: v,
            ty: ScalarType::String,
        })
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Scalar, E> {
        Ok(Self::number(v.to_string()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Scalar, E> {
        Ok(Self::number(v.to_string()))
    }

    // `{:?}` keeps a float looking like a float (`1.0`, `1000.0`, `1e21`) and
    // round-trips exactly, so the reader treats it as the fractional number
    // it is: a byte count or period rejects `1.0` here as the getter does.
    // `Display` would print `1`, which they would accept.
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Scalar, E> {
        Ok(Self::number(format!("{:?}", v)))
    }
}

/// One unit family: what it reads into and which reader it goes through.
/// `read` returns the failure as a message; the path is added by the
/// deserializer's map / seq access.
trait Unit {
    type Value;
    const EXPECTING: &'static str;
    fn read(raw: &str, ty: ScalarType) -> Result<Self::Value, String>;
    /// Whether a scalar that `read` accepts names its unit.
    fn names_unit(raw: &str, ty: ScalarType) -> bool;
}

/// `U`, refusing a value that does not name its unit (the `require_unit`
/// modules). The reader runs first, so a value that does not read keeps its
/// own error. The message names the problem, not the value: a value can be a
/// secret set under the wrong key.
struct RequireUnit<U>(PhantomData<U>);

impl<U: Unit> Unit for RequireUnit<U> {
    type Value = U::Value;
    const EXPECTING: &'static str = U::EXPECTING;
    fn read(raw: &str, ty: ScalarType) -> Result<Self::Value, String> {
        let value = U::read(raw, ty)?;
        if !U::names_unit(raw, ty) {
            return Err(format!("{} must name its unit", U::EXPECTING));
        }
        Ok(value)
    }
    fn names_unit(raw: &str, ty: ScalarType) -> bool {
        U::names_unit(raw, ty)
    }
}

/// The helpers' wording: `invalid duration: "5 parsecs"` (the path is
/// carried apart, in `DeserializeError::path`).
fn unit_message(raw: &str) -> impl FnOnce(UnitError) -> String + '_ {
    move |e| format!("{}: {:?}", e, raw)
}

fn deserialize_unit<'de, U: Unit, D: Deserializer<'de>>(d: D) -> Result<U::Value, D::Error> {
    let s = d.deserialize_newtype_struct(
        RAW_SCALAR,
        ScalarVisitor {
            expecting: U::EXPECTING,
            raw_answer: true,
        },
    )?;
    U::read(&s.raw, s.ty).map_err(de::Error::custom)
}

struct OptionVisitor<U>(PhantomData<U>);

impl<'de, U: Unit> Visitor<'de> for OptionVisitor<U> {
    type Value = Option<U::Value>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a HOCON {} or null", U::EXPECTING)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        deserialize_unit::<U, D>(d).map(Some)
    }
}

fn deserialize_option<'de, U: Unit, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<U::Value>, D::Error> {
    d.deserialize_option(OptionVisitor::<U>(PhantomData))
}

struct DurationUnit;

impl Unit for DurationUnit {
    type Value = std::time::Duration;
    const EXPECTING: &'static str = "duration";
    fn read(raw: &str, ty: ScalarType) -> Result<Self::Value, String> {
        read_duration(raw, ty).map_err(unit_message(raw))
    }
    fn names_unit(raw: &str, ty: ScalarType) -> bool {
        names_magnitude_unit(raw, ty)
    }
}

struct BytesUnit<T>(PhantomData<T>);

impl<T> Unit for BytesUnit<T>
where
    T: TryFrom<i64>,
{
    type Value = T;
    const EXPECTING: &'static str = "byte size";
    fn read(raw: &str, ty: ScalarType) -> Result<T, String> {
        let n = read_bytes(raw, ty).map_err(unit_message(raw))?;
        // The range is the field type's constraint, not a unit rule.
        T::try_from(n).map_err(|_| {
            format!(
                "byte size {:?} ({} bytes) is out of range for {}",
                raw,
                n,
                std::any::type_name::<T>()
            )
        })
    }
    fn names_unit(raw: &str, ty: ScalarType) -> bool {
        names_magnitude_unit(raw, ty)
    }
}

struct PeriodUnit;

impl Unit for PeriodUnit {
    type Value = crate::Period;
    const EXPECTING: &'static str = "period";
    fn read(raw: &str, ty: ScalarType) -> Result<Self::Value, String> {
        read_period(raw, ty).map_err(unit_message(raw))
    }
    fn names_unit(raw: &str, ty: ScalarType) -> bool {
        names_period_unit(raw, ty)
    }
}

/// Read a HOCON duration (`30m`, `100 ms`, `2 hours`) into a
/// [`std::time::Duration`], with the rules of
/// [`Config::get_duration`](crate::Config::get_duration): a bare number is
/// milliseconds, unit names are lowercase, negative durations are errors.
///
/// ```
/// use serde::Deserialize;
/// use std::time::Duration;
///
/// #[derive(Deserialize)]
/// struct Session {
///     #[serde(with = "hocon::serde::duration")]
///     lifetime: Duration,
///     // `default` makes a missing key `None`; `null` is `None` too.
///     #[serde(default, with = "hocon::serde::duration::option")]
///     idle_timeout: Option<Duration>,
/// }
///
/// let s: Session = hocon::from_str("lifetime = 30m")?;
/// assert_eq!(s.lifetime, Duration::from_secs(1800));
/// assert_eq!(s.idle_timeout, None);
/// # Ok::<(), hocon::HoconError>(())
/// ```
pub mod duration {
    use super::{deserialize_option, deserialize_unit, DurationUnit};
    use ::serde::Deserializer;
    use std::time::Duration;

    /// Deserialize a HOCON duration. See the [module docs](self).
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        deserialize_unit::<DurationUnit, D>(deserializer)
    }

    /// `Option<Duration>`: `null` reads as `None`. Add `#[serde(default)]` to
    /// also accept a missing key.
    pub mod option {
        use super::*;

        /// Deserialize an optional HOCON duration.
        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<Duration>, D::Error> {
            deserialize_option::<DurationUnit, D>(deserializer)
        }
    }

    /// [`duration`](crate::serde::duration), refusing a value that does not
    /// name its unit: a number (`ttl = 30`) or a string of the number alone
    /// (`"30"`, `"1e3"`) is an error (`duration must name its unit`) instead of
    /// milliseconds. `30s`, `"30 s"` and `"1e3ms"` read as before; a value that
    /// is not a duration keeps its own error.
    ///
    /// ```
    /// use serde::Deserialize;
    /// use std::time::Duration;
    ///
    /// #[derive(Deserialize)]
    /// struct Token {
    ///     #[serde(with = "hocon::serde::duration::require_unit")]
    ///     ttl: Duration,
    ///     #[serde(default, with = "hocon::serde::duration::require_unit::option")]
    ///     refresh: Option<Duration>,
    /// }
    ///
    /// let t: Token = hocon::from_str("ttl = 30s")?;
    /// assert_eq!(t.ttl, Duration::from_secs(30));
    /// assert_eq!(t.refresh, None);
    ///
    /// let Err(hocon::HoconError::Config(e)) = hocon::from_str::<Token>("ttl = 30") else {
    ///     panic!("a bare number must be refused");
    /// };
    /// assert_eq!((e.path.as_str(), e.message.as_str()), ("ttl", "duration must name its unit"));
    /// # Ok::<(), hocon::HoconError>(())
    /// ```
    pub mod require_unit {
        use super::super::{deserialize_option, deserialize_unit, DurationUnit, RequireUnit};
        use ::serde::Deserializer;
        use std::time::Duration;

        /// Deserialize a HOCON duration that names its unit. See the
        /// [module docs](self).
        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Duration, D::Error> {
            deserialize_unit::<RequireUnit<DurationUnit>, D>(deserializer)
        }

        /// `Option<Duration>` that names its unit: `null` reads as `None`.
        /// Add `#[serde(default)]` to also accept a missing key.
        pub mod option {
            use super::*;

            /// Deserialize an optional HOCON duration that names its unit.
            pub fn deserialize<'de, D: Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<Duration>, D::Error> {
                deserialize_option::<RequireUnit<DurationUnit>, D>(deserializer)
            }
        }
    }
}

/// Read a HOCON byte size (`64KiB`, `512 MB`, `1.5G`) into any integer type
/// `T: TryFrom<i64>`, with the rules of
/// [`Config::get_bytes`](crate::Config::get_bytes): a bare number is bytes,
/// fractions truncate toward zero, byte sizes are non-negative. The field's type sets
/// the range — a size that does not fit `T` is an error.
///
/// ```
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Limits {
///     #[serde(with = "hocon::serde::bytes")]
///     max_body: u64,
///     #[serde(with = "hocon::serde::bytes")]
///     buffer: u32,
///     #[serde(default, with = "hocon::serde::bytes::option")]
///     cache: Option<usize>,
/// }
///
/// let l: Limits = hocon::from_str("max_body = 64KiB, buffer = 4096")?;
/// assert_eq!(l.max_body, 65_536);
/// assert_eq!(l.buffer, 4096);
/// assert_eq!(l.cache, None);
/// # Ok::<(), hocon::HoconError>(())
/// ```
pub mod bytes {
    use super::{deserialize_option, deserialize_unit, BytesUnit};
    use ::serde::Deserializer;

    /// Deserialize a HOCON byte size. See the [module docs](self).
    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: TryFrom<i64>,
    {
        deserialize_unit::<BytesUnit<T>, D>(deserializer)
    }

    /// `Option<T>`: `null` reads as `None`. Add `#[serde(default)]` to also
    /// accept a missing key.
    pub mod option {
        use super::*;

        /// Deserialize an optional HOCON byte size.
        pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
        where
            D: Deserializer<'de>,
            T: TryFrom<i64>,
        {
            deserialize_option::<BytesUnit<T>, D>(deserializer)
        }
    }

    /// [`bytes`](crate::serde::bytes), refusing a value that does not name its
    /// unit: a number (`buffer = 4096`) or a string of the number alone
    /// (`"4096"`, `"1e3"`) is an error (`byte size must name its unit`) instead
    /// of bytes. `64KiB`, `"512 MB"` and `4096B` read as before; a value that
    /// is not a byte size, or does not fit `T`, keeps its own error.
    ///
    /// ```
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Limits {
    ///     #[serde(with = "hocon::serde::bytes::require_unit")]
    ///     max_body: u64,
    ///     #[serde(default, with = "hocon::serde::bytes::require_unit::option")]
    ///     cache: Option<usize>,
    /// }
    ///
    /// let l: Limits = hocon::from_str("max_body = 64KiB")?;
    /// assert_eq!(l.max_body, 65_536);
    /// assert!(hocon::from_str::<Limits>("max_body = 65536").is_err());
    /// # Ok::<(), hocon::HoconError>(())
    /// ```
    pub mod require_unit {
        use super::super::{deserialize_option, deserialize_unit, BytesUnit, RequireUnit};
        use ::serde::Deserializer;

        /// Deserialize a HOCON byte size that names its unit. See the
        /// [module docs](self).
        pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
        where
            D: Deserializer<'de>,
            T: TryFrom<i64>,
        {
            deserialize_unit::<RequireUnit<BytesUnit<T>>, D>(deserializer)
        }

        /// `Option<T>` that names its unit: `null` reads as `None`. Add
        /// `#[serde(default)]` to also accept a missing key.
        pub mod option {
            use super::*;

            /// Deserialize an optional HOCON byte size that names its unit.
            pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
            where
                D: Deserializer<'de>,
                T: TryFrom<i64>,
            {
                deserialize_option::<RequireUnit<BytesUnit<T>>, D>(deserializer)
            }
        }
    }
}

/// Read a HOCON period (`7d`, `2w`, `3m`, `1y`) into a [`Period`](crate::Period),
/// with the rules of [`Config::get_period`](crate::Config::get_period): a bare
/// number is days, the amount is an integer, negative periods are allowed.
///
/// ```
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Retention {
///     #[serde(with = "hocon::serde::period")]
///     keep: hocon::Period,
///     #[serde(default, with = "hocon::serde::period::option")]
///     archive: Option<hocon::Period>,
/// }
///
/// let r: Retention = hocon::from_str("keep = 2w")?;
/// assert_eq!(r.keep, hocon::Period::new(0, 0, 14));
/// assert_eq!(r.archive, None);
/// # Ok::<(), hocon::HoconError>(())
/// ```
pub mod period {
    use super::{deserialize_option, deserialize_unit, PeriodUnit};
    use crate::Period;
    use ::serde::Deserializer;

    /// Deserialize a HOCON period. See the [module docs](self).
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Period, D::Error> {
        deserialize_unit::<PeriodUnit, D>(deserializer)
    }

    /// `Option<Period>`: `null` reads as `None`. Add `#[serde(default)]` to
    /// also accept a missing key.
    pub mod option {
        use super::*;

        /// Deserialize an optional HOCON period.
        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<Period>, D::Error> {
            deserialize_option::<PeriodUnit, D>(deserializer)
        }
    }

    /// [`period`](crate::serde::period), refusing a value that does not name
    /// its unit: a number (`keep = 14`) or a string of the integer alone
    /// (`"14"`) is an error (`period must name its unit`) instead of days.
    /// `14d`, `"2 w"` and `1y` read as before; a value that is not a period
    /// keeps its own error.
    ///
    /// ```
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Retention {
    ///     #[serde(with = "hocon::serde::period::require_unit")]
    ///     keep: hocon::Period,
    ///     #[serde(default, with = "hocon::serde::period::require_unit::option")]
    ///     archive: Option<hocon::Period>,
    /// }
    ///
    /// let r: Retention = hocon::from_str("keep = 2w")?;
    /// assert_eq!(r.keep, hocon::Period::new(0, 0, 14));
    /// assert!(hocon::from_str::<Retention>("keep = 14").is_err());
    /// # Ok::<(), hocon::HoconError>(())
    /// ```
    pub mod require_unit {
        use super::super::{deserialize_option, deserialize_unit, PeriodUnit, RequireUnit};
        use crate::Period;
        use ::serde::Deserializer;

        /// Deserialize a HOCON period that names its unit. See the
        /// [module docs](self).
        pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Period, D::Error> {
            deserialize_unit::<RequireUnit<PeriodUnit>, D>(deserializer)
        }

        /// `Option<Period>` that names its unit: `null` reads as `None`. Add
        /// `#[serde(default)]` to also accept a missing key.
        pub mod option {
            use super::*;

            /// Deserialize an optional HOCON period that names its unit.
            pub fn deserialize<'de, D: Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<Period>, D::Error> {
                deserialize_option::<RequireUnit<PeriodUnit>, D>(deserializer)
            }
        }
    }
}
