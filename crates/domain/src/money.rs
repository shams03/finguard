//! Money is integer minor units + an explicit currency. Never floating point.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Php,
}

impl Currency {
    pub fn code(self) -> &'static str {
        match self {
            Currency::Php => "PHP",
        }
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Currency::Php => "₱",
        }
    }
    /// Digits after the decimal point (PHP centavos = 2).
    pub fn exponent(self) -> u32 {
        2
    }
}

impl FromStr for Currency {
    type Err = MoneyError;
    fn from_str(s: &str) -> Result<Self, MoneyError> {
        match s.trim().to_ascii_uppercase().as_str() {
            "PHP" | "₱" | "PESO" | "PESOS" => Ok(Currency::Php),
            other => Err(MoneyError::UnsupportedCurrency(other.to_owned())),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("currency mismatch: {0} vs {1}")]
    CurrencyMismatch(&'static str, &'static str),
    #[error("arithmetic overflow")]
    Overflow,
    #[error("unsupported currency: {0}")]
    UnsupportedCurrency(String),
    #[error("invalid amount: {0}")]
    InvalidAmount(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: Currency,
}

impl Money {
    pub const fn new(amount_minor: i64, currency: Currency) -> Self {
        Self {
            amount_minor,
            currency,
        }
    }
    pub const fn php(amount_minor: i64) -> Self {
        Self::new(amount_minor, Currency::Php)
    }
    /// Whole pesos convenience for fixtures: `Money::php_major(2_000)` = ₱2,000.00
    pub const fn php_major(pesos: i64) -> Self {
        Self::php(pesos * 100)
    }
    pub fn is_positive(&self) -> bool {
        self.amount_minor > 0
    }
    fn same_currency(&self, other: &Money) -> Result<(), MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch(
                self.currency.code(),
                other.currency.code(),
            ));
        }
        Ok(())
    }
    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(&other)?;
        self.amount_minor
            .checked_add(other.amount_minor)
            .map(|a| Money::new(a, self.currency))
            .ok_or(MoneyError::Overflow)
    }
    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(&other)?;
        self.amount_minor
            .checked_sub(other.amount_minor)
            .map(|a| Money::new(a, self.currency))
            .ok_or(MoneyError::Overflow)
    }
    /// Compare amounts of the same currency.
    pub fn checked_cmp(&self, other: &Money) -> Result<std::cmp::Ordering, MoneyError> {
        self.same_currency(other)?;
        Ok(self.amount_minor.cmp(&other.amount_minor))
    }

    /// Parse a human amount such as `2000`, `2,000.50`, `₱2,000`. At most two
    /// decimals, never negative. Integer arithmetic only.
    pub fn parse_major(input: &str, currency: Currency) -> Result<Money, MoneyError> {
        let cleaned: String = input
            .trim()
            .trim_start_matches(currency.symbol())
            .trim_start_matches(currency.code())
            .trim()
            .chars()
            .filter(|c| *c != ',' && *c != '_' && !c.is_whitespace())
            .collect();
        if cleaned.is_empty() {
            return Err(MoneyError::InvalidAmount("empty"));
        }
        let (whole, frac) = match cleaned.split_once('.') {
            Some((w, f)) => (w, f),
            None => (cleaned.as_str(), ""),
        };
        if whole.is_empty() && frac.is_empty() {
            return Err(MoneyError::InvalidAmount("no digits"));
        }
        if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err(MoneyError::InvalidAmount("not a plain positive number"));
        }
        if frac.len() > currency.exponent() as usize {
            return Err(MoneyError::InvalidAmount("too many decimal places"));
        }
        let whole_n: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| MoneyError::Overflow)?
        };
        let mut frac_padded = frac.to_owned();
        while frac_padded.len() < currency.exponent() as usize {
            frac_padded.push('0');
        }
        let frac_n: i64 = if frac_padded.is_empty() {
            0
        } else {
            frac_padded.parse().map_err(|_| MoneyError::Overflow)?
        };
        let minor = whole_n
            .checked_mul(10_i64.pow(currency.exponent()))
            .and_then(|w| w.checked_add(frac_n))
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, currency))
    }

    /// `PHP 2000.00` — the machine-friendly API form.
    pub fn to_api_string(&self) -> String {
        format!("{} {}", self.currency.code(), self.plain_major())
    }

    /// `2000.00`
    pub fn plain_major(&self) -> String {
        let div = 10_i64.pow(self.currency.exponent());
        let sign = if self.amount_minor < 0 { "-" } else { "" };
        let abs = self.amount_minor.unsigned_abs();
        format!("{sign}{}.{:02}", abs / div as u64, abs % div as u64)
    }

    /// `₱2,000.00` — the human-friendly UI form.
    pub fn pretty(&self) -> String {
        let div = 10_i64.pow(self.currency.exponent()) as u64;
        let abs = self.amount_minor.unsigned_abs();
        let whole = (abs / div).to_string();
        let mut grouped = String::new();
        for (i, ch) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        let sign = if self.amount_minor < 0 { "-" } else { "" };
        format!(
            "{sign}{}{}.{:02}",
            self.currency.symbol(),
            grouped,
            abs % div
        )
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_api_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_2000_is_200000_minor() {
        assert_eq!(Money::php_major(2_000).amount_minor, 200_000);
        assert_eq!(Money::php_major(2_000).to_api_string(), "PHP 2000.00");
        assert_eq!(Money::php_major(2_000).pretty(), "₱2,000.00");
        assert_eq!(Money::php(123_456_789).pretty(), "₱1,234,567.89");
        assert_eq!(Money::php(5).pretty(), "₱0.05");
    }

    #[test]
    fn parse_variants() {
        let p = |s| Money::parse_major(s, Currency::Php);
        assert_eq!(p("2000").unwrap().amount_minor, 200_000);
        assert_eq!(p("2,000.50").unwrap().amount_minor, 200_050);
        assert_eq!(p("₱75,000").unwrap().amount_minor, 7_500_000);
        assert_eq!(p("0.5").unwrap().amount_minor, 50);
        assert_eq!(p(".25").unwrap().amount_minor, 25);
        assert!(p("1.234").is_err());
        assert!(p("-5").is_err());
        assert!(p("abc").is_err());
        assert!(p("").is_err());
        assert_eq!(p("99999999999999999999"), Err(MoneyError::Overflow));
    }

    #[test]
    fn arithmetic_is_checked() {
        let a = Money::php(i64::MAX);
        assert_eq!(a.checked_add(Money::php(1)), Err(MoneyError::Overflow));
        assert_eq!(
            Money::php(10).checked_sub(Money::php(3)).unwrap(),
            Money::php(7)
        );
    }

    #[test]
    fn no_float_drift() {
        // 0.1 + 0.2 style bug cannot happen with minor units
        let s = Money::parse_major("0.10", Currency::Php)
            .unwrap()
            .checked_add(Money::parse_major("0.20", Currency::Php).unwrap())
            .unwrap();
        assert_eq!(s.amount_minor, 30);
    }

    #[test]
    fn serde_shape() {
        let v = serde_json::to_value(Money::php(200_000)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"amount_minor":200000,"currency":"PHP"})
        );
    }
}
