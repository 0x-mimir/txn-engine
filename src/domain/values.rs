use std::fmt;
use std::str::FromStr;

const UNITS_PER_WHOLE: i64 = 10_000;
const DECIMAL_PLACES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxId(pub u32);

/// A signed balance in whole ten-thousandths, so that arithmetic stays exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Funds(pub(crate) i64);

impl Funds {
    pub const ZERO: Self = Self(0);

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }
}

impl fmt::Display for Funds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        let whole = magnitude / UNITS_PER_WHOLE.unsigned_abs();
        let fraction = magnitude % UNITS_PER_WHOLE.unsigned_abs();
        write!(f, "{sign}{whole}.{fraction:0DECIMAL_PLACES$}")
    }
}

/// A strictly positive transaction amount, in the same whole units as [`Funds`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount(i64);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid amount {0:?}: expected a positive number with at most {DECIMAL_PLACES} decimal places"
)]
pub struct InvalidAmount(String);

impl Amount {
    pub fn credit(self) -> Funds {
        Funds(self.0)
    }

    pub fn debit(self) -> Funds {
        Funds(-self.0)
    }

    fn parse(value: &str) -> Option<Self> {
        let (whole, fraction) = value.split_once('.').unwrap_or((value, "0"));
        let digits_only = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !digits_only(whole) || !digits_only(fraction) || fraction.len() > DECIMAL_PLACES {
            return None;
        }
        let padding = 10_i64.pow((DECIMAL_PLACES - fraction.len()) as u32);
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
