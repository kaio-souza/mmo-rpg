//! Fixed point number used by every game rule: `i32` in thousandths.
//!
//! All arithmetic saturates instead of wrapping or panicking, and rounds toward
//! zero, so the same inputs give the same result on every machine.

use std::fmt;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(i32);

impl Fixed {
    pub const SCALE: i32 = 1000;
    pub const ZERO: Fixed = Fixed(0);
    pub const ONE: Fixed = Fixed(Self::SCALE);
    pub const MAX: Fixed = Fixed(i32::MAX);
    pub const MIN: Fixed = Fixed(i32::MIN);

    pub const fn from_millis(millis: i32) -> Self {
        Self(millis)
    }

    pub const fn millis(self) -> i32 {
        self.0
    }

    pub const fn from_int(n: i32) -> Self {
        Self(n.saturating_mul(Self::SCALE))
    }

    /// `num / den`, rounded toward zero. `None` when `den` is zero.
    pub fn from_ratio(num: i64, den: i64) -> Option<Self> {
        if den == 0 {
            return None;
        }
        Some(Self::saturate(num.saturating_mul(i64::from(Self::SCALE)) / den))
    }

    /// Integer part, rounded toward zero.
    pub const fn trunc(self) -> i32 {
        self.0 / Self::SCALE
    }

    /// Nearest integer, halves away from zero.
    pub const fn round(self) -> i32 {
        let half = Self::SCALE / 2;
        if self.0 >= 0 {
            (self.0 / Self::SCALE) + ((self.0 % Self::SCALE >= half) as i32)
        } else {
            (self.0 / Self::SCALE) - ((self.0 % Self::SCALE <= -half) as i32)
        }
    }

    pub const fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    pub fn clamp(self, lo: Fixed, hi: Fixed) -> Self {
        Self(self.0.clamp(lo.0, hi.0))
    }

    /// `self / rhs`, rounded toward zero. `None` when `rhs` is zero.
    pub fn checked_div(self, rhs: Fixed) -> Option<Self> {
        if rhs.0 == 0 {
            return None;
        }
        Some(Self::saturate(
            i64::from(self.0) * i64::from(Self::SCALE) / i64::from(rhs.0),
        ))
    }

    fn saturate(v: i64) -> Self {
        Self(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }
}

impl Add for Fixed {
    type Output = Fixed;
    fn add(self, rhs: Fixed) -> Fixed {
        Fixed(self.0.saturating_add(rhs.0))
    }
}

impl AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Fixed) {
        *self = *self + rhs;
    }
}

impl Sub for Fixed {
    type Output = Fixed;
    fn sub(self, rhs: Fixed) -> Fixed {
        Fixed(self.0.saturating_sub(rhs.0))
    }
}

impl SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Fixed) {
        *self = *self - rhs;
    }
}

/// Rounded toward zero, saturating.
impl Mul for Fixed {
    type Output = Fixed;
    fn mul(self, rhs: Fixed) -> Fixed {
        Fixed::saturate(i64::from(self.0) * i64::from(rhs.0) / i64::from(Fixed::SCALE))
    }
}

/// Saturating.
impl Mul<i32> for Fixed {
    type Output = Fixed;
    fn mul(self, rhs: i32) -> Fixed {
        Fixed(self.0.saturating_mul(rhs))
    }
}

impl Neg for Fixed {
    type Output = Fixed;
    fn neg(self) -> Fixed {
        Fixed(self.0.saturating_neg())
    }
}

/// Shortest decimal form: `1.5`, `-0.25`, `3`.
impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = i64::from(self.0);
        let sign = if v < 0 { "-" } else { "" };
        let abs = v.abs();
        let int = abs / 1000;
        let mut frac = abs % 1000;
        if frac == 0 {
            return write!(f, "{sign}{int}");
        }
        let mut digits = 3;
        while frac % 10 == 0 {
            frac /= 10;
            digits -= 1;
        }
        write!(f, "{sign}{int}.{frac:0digits$}")
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed({self})")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFixedError;

impl fmt::Display for ParseFixedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid fixed point number (up to 3 decimals)")
    }
}

impl std::error::Error for ParseFixedError {}

/// Exact decimal parse, no floating point: `"0.75"`, `"-2"`, `"10.125"`.
impl FromStr for Fixed {
    type Err = ParseFixedError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let (int_part, frac_part) = body.split_once('.').unwrap_or((body, ""));
        let digits_ok = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if int_part.is_empty() || !digits_ok(int_part) || !digits_ok(frac_part) {
            return Err(ParseFixedError);
        }
        if frac_part.len() > 3 || (body.contains('.') && frac_part.is_empty()) {
            return Err(ParseFixedError);
        }
        let int: i64 = int_part.parse().map_err(|_| ParseFixedError)?;
        let mut frac: i64 = if frac_part.is_empty() {
            0
        } else {
            frac_part.parse().map_err(|_| ParseFixedError)?
        };
        for _ in frac_part.len()..3 {
            frac *= 10;
        }
        let millis = int
            .checked_mul(1000)
            .and_then(|v| v.checked_add(frac))
            .ok_or(ParseFixedError)?;
        let millis = if neg { -millis } else { millis };
        i32::try_from(millis).map(Fixed).map_err(|_| ParseFixedError)
    }
}

#[cfg(feature = "serde")]
mod serde_impl {
    use super::Fixed;
    use serde::de::{self, Visitor};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::fmt;

    /// Human readable formats (RON) use the decimal text; binary formats
    /// (postcard) use the raw thousandths.
    impl Serialize for Fixed {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if s.is_human_readable() {
                s.collect_str(self)
            } else {
                s.serialize_i32(self.0)
            }
        }
    }

    impl<'de> Deserialize<'de> for Fixed {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            if d.is_human_readable() {
                d.deserialize_any(FixedVisitor)
            } else {
                i32::deserialize(d).map(Fixed)
            }
        }
    }

    struct FixedVisitor;

    impl Visitor<'_> for FixedVisitor {
        type Value = Fixed;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a number with up to 3 decimals")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Fixed, E> {
            i32::try_from(v)
                .ok()
                .and_then(|n| n.checked_mul(Fixed::SCALE))
                .map(Fixed)
                .ok_or_else(|| E::custom("number out of range"))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Fixed, E> {
            let v = i64::try_from(v).map_err(|_| E::custom("number out of range"))?;
            self.visit_i64(v)
        }

        /// The float is only a carrier of the literal written in the file; it is
        /// printed back to its shortest decimal form and parsed exactly.
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Fixed, E> {
            self.visit_str(&v.to_string())
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Fixed, E> {
            v.parse().map_err(E::custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(s: &str) -> Fixed {
        s.parse().unwrap()
    }

    #[test]
    fn parse_and_display() {
        for s in ["0", "1", "-1", "0.5", "0.75", "-0.025", "10.125", "2147483.647"] {
            assert_eq!(f(s).to_string(), s);
        }
        assert_eq!(f("1.50").to_string(), "1.5");
        for bad in ["", "-", "1.", ".5", "1.2345", "a", "1.-2", "9999999"] {
            assert!(bad.parse::<Fixed>().is_err(), "{bad}");
        }
    }

    #[test]
    fn arithmetic() {
        assert_eq!(f("1.5") + f("0.25"), f("1.75"));
        assert_eq!(f("1.5") * f("1.5"), f("2.25"));
        assert_eq!(f("1.5") * 3, f("4.5"));
        assert_eq!(f("0.001") * f("0.5"), Fixed::ZERO); // toward zero
        assert_eq!(f("-0.001") * f("0.5"), Fixed::ZERO);
        assert_eq!(f("1").checked_div(f("3")), Some(f("0.333")));
        assert_eq!(f("1").checked_div(Fixed::ZERO), None);
        assert_eq!(Fixed::from_ratio(2, 3), Some(f("0.666")));
        assert_eq!(Fixed::MAX + Fixed::ONE, Fixed::MAX);
        assert_eq!(Fixed::MAX * Fixed::from_int(2), Fixed::MAX);
        assert_eq!(-Fixed::MIN, Fixed::MAX);
    }

    #[test]
    fn rounding() {
        assert_eq!(f("2.5").round(), 3);
        assert_eq!(f("2.499").round(), 2);
        assert_eq!(f("-2.5").round(), -3);
        assert_eq!(f("-2.499").round(), -2);
        assert_eq!(f("-2.7").trunc(), -2);
    }
}
