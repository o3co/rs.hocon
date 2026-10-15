//! Exact decimal arithmetic for unit values (E20).
//!
//! A unit value's number is read from its source text as an exact decimal and
//! multiplied by the unit's integer factor exactly; the result is truncated
//! toward zero. Binary floating point is never involved, so `1.001s` is
//! exactly 1001 ms and `1.001kB` exactly 1001 bytes.
//!
//! The magnitude is held as base-10^9 limbs, which keeps parsing, scaling by
//! powers of ten and truncating division by powers of ten simple. Values are
//! small in practice; the size guard in [`Decimal::mul_trunc`] keeps the work
//! bounded for pathological inputs (hundreds of digits, huge exponents).

/// An exact decimal: `(-1)^negative × digits × 10^exp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Decimal {
    negative: bool,
    /// Significant digits, most significant first, without leading zeros.
    /// Empty for zero.
    digits: Vec<u8>,
    exp: i64,
}

/// Exponent arithmetic saturates at ± this bound. No input can carry that
/// many digits, so the fraction length is always subtracted exactly and a
/// saturated exponent still leaves the value far outside every unit range
/// (or truncating to zero).
const EXP_LIMIT: i64 = i64::MAX / 4;

impl Decimal {
    /// Parse a number in Java's `double` grammar, as Lightbend reads the
    /// number part of a unit value: `[+-]? (digits [. digits?] | . digits)`
    /// with an optional `[eE] [+-]? digits` exponent (`1e+5`, `1.e3`, `.5`).
    /// Returns `None` for anything else (`1e3.5`, `1e`, `inf`).
    pub(super) fn parse(s: &str) -> Option<Decimal> {
        let (negative, rest) = match s.as_bytes().first() {
            Some(b'-') => (true, &s[1..]),
            Some(b'+') => (false, &s[1..]),
            _ => (false, s),
        };
        let (mantissa, exp_part) = match rest.find(['e', 'E']) {
            Some(i) => (&rest[..i], Some(&rest[i + 1..])),
            None => (rest, None),
        };
        let (int_part, frac_part) = match mantissa.find('.') {
            Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
            None => (mantissa, ""),
        };
        let all_digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if int_part.len() + frac_part.len() == 0 || !all_digits(int_part) || !all_digits(frac_part)
        {
            return None;
        }
        let mut exp: i64 = match exp_part {
            None => 0,
            Some(e) => parse_exponent(e)?,
        };
        exp = exp.saturating_sub(i64::try_from(frac_part.len()).unwrap_or(EXP_LIMIT));
        let mut digits: Vec<u8> = int_part
            .bytes()
            .chain(frac_part.bytes())
            .map(|b| b - b'0')
            .skip_while(|&d| d == 0)
            .collect();
        // Trailing zeros only scale; fold them into the exponent.
        while digits.last() == Some(&0) {
            digits.pop();
            exp = exp.saturating_add(1);
        }
        Some(Decimal {
            negative,
            digits,
            exp,
        })
    }

    pub(super) fn is_negative(&self) -> bool {
        self.negative && !self.is_zero()
    }

    pub(super) fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// `|self| × factor`, truncated toward zero, or `None` when the result
    /// does not fit in a `u128`.
    pub(super) fn mul_trunc(&self, factor: u128) -> Option<u128> {
        if self.is_zero() || factor == 0 {
            return Some(0);
        }
        // |self| ≥ 10^(len + exp − 1) and factor ≥ 1: past 10^39 the result
        // cannot fit a u128 (≈ 3.4 × 10^38).
        if (self.digits.len() as i64).saturating_add(self.exp) > 40 {
            return None;
        }
        let mut n = Limbs::from_digits(&self.digits);
        n.mul_small(factor);
        if self.exp >= 0 {
            n.mul_pow10(self.exp as u32);
        } else {
            n.div_pow10_trunc(self.exp.unsigned_abs());
        }
        n.to_u128()
    }
}

fn parse_exponent(e: &str) -> Option<i64> {
    let (neg, digits) = match e.as_bytes().first() {
        Some(b'-') => (true, &e[1..]),
        Some(b'+') => (false, &e[1..]),
        _ => (false, e),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut v: i64 = 0;
    for b in digits.bytes() {
        v = v
            .saturating_mul(10)
            .saturating_add(i64::from(b - b'0'))
            .min(EXP_LIMIT);
    }
    Some(if neg { -v } else { v })
}

const BASE: u128 = 1_000_000_000;

/// An unsigned integer in base-10^9 limbs, least significant first.
struct Limbs(Vec<u128>);

impl Limbs {
    fn from_digits(digits: &[u8]) -> Limbs {
        let mut limbs = Vec::with_capacity(digits.len() / 9 + 1);
        let mut end = digits.len();
        while end > 0 {
            let start = end.saturating_sub(9);
            let limb = digits[start..end]
                .iter()
                .fold(0u128, |acc, &d| acc * 10 + u128::from(d));
            limbs.push(limb);
            end = start;
        }
        Limbs(limbs)
    }

    /// `self *= m`. A limb is < 10^9 < 2^30 and every unit factor is < 2^81,
    /// so a limb product plus carry stays below 2^112.
    fn mul_small(&mut self, m: u128) {
        let mut carry = 0u128;
        for limb in &mut self.0 {
            let v = *limb * m + carry;
            *limb = v % BASE;
            carry = v / BASE;
        }
        while carry > 0 {
            self.0.push(carry % BASE);
            carry /= BASE;
        }
    }

    fn mul_pow10(&mut self, k: u32) {
        let whole = (k / 9) as usize;
        self.0.splice(0..0, std::iter::repeat_n(0, whole));
        self.mul_small(10u128.pow(k % 9));
    }

    /// `self = floor(self / 10^k)`.
    fn div_pow10_trunc(&mut self, k: u64) {
        let whole = usize::try_from(k / 9).unwrap_or(usize::MAX);
        if whole >= self.0.len() {
            self.0.clear();
            return;
        }
        self.0.drain(..whole);
        let d = 10u128.pow((k % 9) as u32);
        let mut rem = 0u128;
        for limb in self.0.iter_mut().rev() {
            let v = rem * BASE + *limb;
            *limb = v / d;
            rem = v % d;
        }
    }

    fn to_u128(&self) -> Option<u128> {
        self.0
            .iter()
            .rev()
            .try_fold(0u128, |acc, &limb| acc.checked_mul(BASE)?.checked_add(limb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn parse_forms() {
        for ok in [
            "0", "-0", "+5", "5.", ".5", "1.001", "007", "1e3", "1E-3", "2.05e+0",
        ] {
            assert!(Decimal::parse(ok).is_some(), "{}", ok);
        }
        for bad in [
            "", ".", "-", "1.2.3", "1-2", "e3", "1e", "1e+", "abc", "inf", "NaN",
        ] {
            assert!(Decimal::parse(bad).is_none(), "{}", bad);
        }
        assert!(Decimal::parse("1e3.5").is_none());
    }

    #[test]
    fn normalizes_zeros() {
        assert_eq!(d("007.500"), d("7.5"));
        assert_eq!(d("1e3"), d("1000"));
        assert!(d("-0.000").is_zero());
        assert!(!d("-0.0").is_negative());
        assert!(d("-0.1").is_negative());
    }

    #[test]
    fn mul_trunc_is_exact() {
        assert_eq!(d("1.001").mul_trunc(1_000_000_000), Some(1_001_000_000));
        assert_eq!(
            d("2.3").mul_trunc(3_600_000_000_000),
            Some(8_280_000_000_000)
        );
        assert_eq!(d("1.0009").mul_trunc(1000), Some(1000)); // truncates
        assert_eq!(d("1.001").mul_trunc(1000), Some(1001));
        assert_eq!(d("0.0000006").mul_trunc(1_000_000), Some(0));
        assert_eq!(d("6e-7").mul_trunc(1_000_000), Some(0));
        assert_eq!(d("1.5").mul_trunc(1), Some(1));
        assert_eq!(d("-2.5").mul_trunc(2), Some(5)); // magnitude
    }

    #[test]
    fn mul_trunc_large_and_small() {
        assert_eq!(d("1").mul_trunc(1u128 << 80), Some(1u128 << 80));
        assert_eq!(
            d("340282366920938463463374607431768211455").mul_trunc(1),
            Some(u128::MAX)
        );
        assert_eq!(
            d("340282366920938463463374607431768211456").mul_trunc(1),
            None
        );
        assert_eq!(d(&format!("1{}", "0".repeat(400))).mul_trunc(1), None);
        assert_eq!(d(&format!("1{}.5", "0".repeat(400))).mul_trunc(1), None);
        assert_eq!(d("1e-1000000000").mul_trunc(1u128 << 80), Some(0));
        assert_eq!(d("1e1000000000").mul_trunc(1), None);
        let long_fraction = format!("0.{}1", "0".repeat(300));
        assert_eq!(d(&long_fraction).mul_trunc(1u128 << 80), Some(0));
    }

    #[test]
    fn exponent_cancels_a_long_fraction_exactly() {
        // The fraction length is subtracted before any bound applies:
        // 0.<10^6 zeros>1 × 10^1000001 is exactly 1.
        let one = format!("0.{}1e1000001", "0".repeat(1_000_000));
        assert_eq!(d(&one).mul_trunc(1), Some(1));
        let huge_exp = format!("1e{}", "9".repeat(40));
        assert_eq!(d(&huge_exp).mul_trunc(1), None);
        let tiny_exp = format!("1e-{}", "9".repeat(40));
        assert_eq!(d(&tiny_exp).mul_trunc(1u128 << 80), Some(0));
    }
}
