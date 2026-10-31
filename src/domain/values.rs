//! The value objects of the domain.

use std::fmt;
use std::str::FromStr;

/// The number of whole units in the value 1.0.
const UNITS_PER_WHOLE: i64 = 10_000;
/// The largest number of digits after the decimal point.
const DECIMAL_PLACES: usize = 4;

/// The ID of a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(pub u16);

/// The ID of a transaction. This ID is unique for all clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxId(pub u32);

/// A signed quantity of money. The value is in whole units of 0.0001, so that all calculations
/// are exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Funds(pub(crate) i64);

impl Funds {
    /// No funds.
    pub const ZERO: Self = Self(0);

    /// Adds `other`. Gives `None` if the result is out of range.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }
}

/// Writes the funds with 4 digits after the decimal point.
impl fmt::Display for Funds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        // Use `unsigned_abs`, because `i64::MIN` has no positive `i64` value.
        let magnitude = self.0.unsigned_abs();
        let whole = magnitude / UNITS_PER_WHOLE.unsigned_abs();
        let fraction = magnitude % UNITS_PER_WHOLE.unsigned_abs();
        write!(f, "{sign}{whole}.{fraction:0DECIMAL_PLACES$}")
    }
}

/// A quantity of money that is more than zero, in the same whole units as [`Funds`].
///
/// Only [`FromStr`] makes an `Amount`, so each `Amount` is in range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount(i64);

/// The text of an amount is not correct. The value is this text.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid amount {0:?}: expected a positive number with at most {DECIMAL_PLACES} decimal places"
)]
pub struct InvalidAmount(String);

impl Amount {
    /// Gives the funds that this amount adds to a balance.
    #[must_use]
    pub fn credit(self) -> Funds {
        Funds(self.0)
    }

    /// Gives the funds that this amount subtracts from a balance.
    #[must_use]
    pub fn debit(self) -> Funds {
        // An `Amount` is more than zero, so this negation cannot overflow.
        Funds(-self.0)
    }

    /// Changes text to whole units. Gives `None` if the text is not correct.
    fn parse(value: &str) -> Option<Self> {
        let (whole, fraction) = value.split_once('.').unwrap_or((value, "0"));
        // Check the digits here, because `parse::<i64>` also accepts a sign.
        let digits_only = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !digits_only(whole) || !digits_only(fraction) || fraction.len() > DECIMAL_PLACES {
            return None;
        }
        // Scale the fraction to whole units. For example, `.5` becomes 5000.
        let padding = 10_i64.pow(u32::try_from(DECIMAL_PLACES - fraction.len()).ok()?);
        let fraction = fraction.parse::<i64>().ok()? * padding;
        let units = whole
            .parse::<i64>()
            .ok()?
            .checked_mul(UNITS_PER_WHOLE)?
            .checked_add(fraction)?;
        (units > 0).then_some(Self(units))
    }
}

impl FromStr for Amount {
    type Err = InvalidAmount;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| InvalidAmount(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> Result<Amount, InvalidAmount> {
        value.parse()
    }

    #[test]
    fn amount_is_held_in_whole_ten_thousandths() {
        assert_eq!(parse("1").unwrap().credit(), Funds(1_0000));
        assert_eq!(parse("1.5").unwrap().credit(), Funds(1_5000));
        assert_eq!(parse("0.0001").unwrap().credit(), Funds(1));
        assert_eq!(parse("2.1234").unwrap().credit(), Funds(2_1234));
    }

    #[test]
    fn amount_debit_is_the_negated_credit() {
        assert_eq!(parse("1.5").unwrap().debit(), Funds(-1_5000));
    }

    #[test]
    fn amount_rejects_more_than_four_decimal_places() {
        assert!(parse("1.23456").is_err());
        assert!(parse("0.00001").is_err());
        assert!(parse("1.00000").is_err());
    }

    #[test]
    fn amount_must_be_positive() {
        assert!(parse("0").is_err());
        assert!(parse("0.0000").is_err());
        assert!(parse("-1.0").is_err());
    }

    #[test]
    fn amount_rejects_anything_but_plain_decimal_notation() {
        for malformed in [
            "", ".", ".5", "1.", "+1", "1e3", "1.2.3", "abc", " 1", "1.-5",
        ] {
            assert!(parse(malformed).is_err(), "accepted {malformed:?}");
        }
    }

    #[test]
    fn amount_tops_out_at_the_largest_representable_balance() {
        assert_eq!(
            parse("922337203685477.5807").unwrap().credit(),
            Funds(i64::MAX)
        );
        assert!(parse("922337203685477.5808").is_err());
        assert!(parse("99999999999999999999").is_err());
        assert!(parse("922337203685478").is_err());
        assert!(parse("1844674407370956").is_err());
    }

    #[test]
    fn funds_always_display_four_decimal_places() {
        assert_eq!(Funds(0).to_string(), "0.0000");
        assert_eq!(Funds(1_5000).to_string(), "1.5000");
        assert_eq!(Funds(1).to_string(), "0.0001");
    }

    #[test]
    fn negative_funds_keep_their_sign_below_one_unit() {
        assert_eq!(Funds(-5000).to_string(), "-0.5000");
        assert_eq!(Funds(-3_0000).to_string(), "-3.0000");
        assert_eq!(Funds(i64::MIN).to_string(), "-922337203685477.5808");
    }
}
