//! Exact, nonnegative accounting values. Never round ingestion or arithmetic.
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericError {
    Invalid,
    Precision,
    Overflow,
    Incompatible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenCount(pub u64);
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenTotal(pub u128);
impl TokenTotal {
    pub fn checked_add(self, other: Self) -> Result<Self, NumericError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }
}
impl From<TokenCount> for TokenTotal {
    fn from(value: TokenCount) -> Self {
        Self(u128::from(value.0))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubcountRelation {
    Included,
    Additional,
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenFact {
    pub input: Option<TokenCount>,
    pub output: Option<TokenCount>,
    pub cached: Option<TokenCount>,
    pub reasoning: Option<TokenCount>,
    pub cached_relation: SubcountRelation,
    pub reasoning_relation: SubcountRelation,
}
impl TokenFact {
    /// Only sum compatible base counts. Detail subcounts never establish a total.
    pub fn base_total(&self) -> Result<Option<TokenTotal>, NumericError> {
        match (self.input, self.output) {
            (Some(a), Some(b)) => Ok(Some(TokenTotal::from(a).checked_add(b.into())?)),
            _ => Ok(None),
        }
    }
    /// Recorded total: base counts plus only the subcounts the source marks as
    /// additional. Included or unknown subcounts are never added again, so a
    /// recorded total never double counts cached or reasoning tokens.
    pub fn recorded_total(&self) -> Result<Option<TokenTotal>, NumericError> {
        let Some(mut total) = self.base_total()? else {
            return Ok(None);
        };
        for (count, relation) in [
            (self.cached, self.cached_relation),
            (self.reasoning, self.reasoning_relation),
        ] {
            if let (Some(count), SubcountRelation::Additional) = (count, relation) {
                total = total.checked_add(count.into())?;
            }
        }
        Ok(Some(total))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decimal {
    coefficient: u128,
    scale: u32,
}
impl FromStr for Decimal {
    type Err = NumericError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts = text.split('.');
        let whole = parts.next().ok_or(NumericError::Invalid)?;
        let fraction = parts.next();
        if parts.next().is_some()
            || whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(NumericError::Invalid);
        }
        let fraction = fraction.unwrap_or("");
        // Check submitted precision before normalization; never silently trim excess scale.
        if fraction.len() > 18 {
            return Err(NumericError::Precision);
        }
        let digits = format!("{whole}{fraction}");
        let significant = digits.trim_start_matches('0');
        if significant.len() > 28 {
            return Err(NumericError::Precision);
        }
        let coefficient = if significant.is_empty() {
            0
        } else {
            significant.parse().map_err(|_| NumericError::Overflow)?
        };
        Ok(Self::normalized(coefficient, fraction.len() as u32))
    }
}
impl Decimal {
    fn normalized(mut coefficient: u128, mut scale: u32) -> Self {
        while scale > 0 && coefficient.is_multiple_of(10) {
            coefficient /= 10;
            scale -= 1;
        }
        Self { coefficient, scale }
    }
    pub fn checked_add(self, other: Self) -> Result<Self, NumericError> {
        let scale = self.scale.max(other.scale);
        let align = |n: Self| {
            n.coefficient
                .checked_mul(10u128.pow(scale - n.scale))
                .ok_or(NumericError::Overflow)
        };
        let coefficient = align(self)?
            .checked_add(align(other)?)
            .ok_or(NumericError::Overflow)?;
        let result = Self::normalized(coefficient, scale);
        if result.coefficient.to_string().len() > 28 {
            return Err(NumericError::Precision);
        }
        Ok(result)
    }
}
impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.coefficient);
        }
        let digits = format!(
            "{:0width$}",
            self.coefficient,
            width = self.scale as usize + 1
        );
        let split = digits.len() - self.scale as usize;
        write!(f, "{}.{}", &digits[..split], &digits[split..])
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ratio(Decimal);
impl FromStr for Ratio {
    type Err = NumericError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let decimal: Decimal = value.parse()?;
        if decimal.coefficient > 10u128.pow(decimal.scale) {
            return Err(NumericError::Invalid);
        }
        Ok(Self(decimal))
    }
}
impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Currency(String);
impl FromStr for Currency {
    type Err = NumericError;
    fn from_str(code: &str) -> Result<Self, Self::Err> {
        if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(NumericError::Invalid);
        }
        Ok(Self(code.into()))
    }
}
impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}
impl Money {
    pub fn checked_add(&self, other: &Self) -> Result<Self, NumericError> {
        if self.currency != other.currency {
            return Err(NumericError::Incompatible);
        }
        Ok(Self {
            amount: self.amount.checked_add(other.amount)?,
            currency: self.currency.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_extremes_and_subcounts() {
        let mut fact = TokenFact {
            input: Some(TokenCount(u64::MAX)),
            output: Some(TokenCount(u64::MAX)),
            cached: Some(TokenCount(4)),
            reasoning: Some(TokenCount(3)),
            cached_relation: SubcountRelation::Included,
            reasoning_relation: SubcountRelation::Unknown,
        };
        assert_eq!(
            fact.base_total(),
            Ok(Some(TokenTotal(u128::from(u64::MAX) * 2)))
        );
        fact.input = None;
        assert_eq!(fact.base_total(), Ok(None));
        assert_eq!(
            TokenTotal(u128::MAX).checked_add(TokenTotal(1)),
            Err(NumericError::Overflow)
        );
    }
    #[test]
    fn recorded_total_adds_only_additional_subcounts() {
        let fact = |cached_relation, reasoning_relation| TokenFact {
            input: Some(TokenCount(1_000)),
            output: Some(TokenCount(2_000)),
            cached: Some(TokenCount(100)),
            reasoning: Some(TokenCount(50)),
            cached_relation,
            reasoning_relation,
        };
        assert_eq!(
            fact(SubcountRelation::Included, SubcountRelation::Included).recorded_total(),
            Ok(Some(TokenTotal(3_000)))
        );
        assert_eq!(
            fact(SubcountRelation::Unknown, SubcountRelation::Unknown).recorded_total(),
            Ok(Some(TokenTotal(3_000)))
        );
        assert_eq!(
            fact(SubcountRelation::Included, SubcountRelation::Additional).recorded_total(),
            Ok(Some(TokenTotal(3_050)))
        );
        assert_eq!(
            fact(SubcountRelation::Additional, SubcountRelation::Additional).recorded_total(),
            Ok(Some(TokenTotal(3_150)))
        );
        let mut incomplete = fact(SubcountRelation::Additional, SubcountRelation::Additional);
        incomplete.output = None;
        assert_eq!(incomplete.recorded_total(), Ok(None));
    }
    #[test]
    fn decimals_reject_without_rounding() {
        for invalid in [
            "", "-1", "+1", "1e2", "NaN", ".1", "1.", "1.2.3", " 1", "１",
        ] {
            assert!(invalid.parse::<Decimal>().is_err(), "{invalid}");
        }
        assert_eq!("0001.2300".parse::<Decimal>().unwrap().to_string(), "1.23");
        assert!("1234567890123456789012345678".parse::<Decimal>().is_ok());
        assert!("0.123456789012345678".parse::<Decimal>().is_ok());
        for excess in [
            "12345678901234567890123456789",
            "0.1234567890123456789",
            "1.0000000000000000000",
        ] {
            assert_eq!(excess.parse::<Decimal>(), Err(NumericError::Precision));
        }
        let a: Decimal = "0.1".parse().unwrap();
        assert_eq!(
            a.checked_add("0.2".parse().unwrap()).unwrap().to_string(),
            "0.3"
        );
        assert!("9999999999999999999999999999"
            .parse::<Decimal>()
            .unwrap()
            .checked_add("1".parse().unwrap())
            .is_err());
        assert!("9999999999999999999999999999"
            .parse::<Decimal>()
            .unwrap()
            .checked_add("0.000000000000000001".parse().unwrap())
            .is_err());
    }
    #[test]
    fn ratios_and_money_stay_unit_safe() {
        for valid in ["0", "1", "0.999999999999999999"] {
            assert!(valid.parse::<Ratio>().is_ok());
        }
        for invalid in ["-0.1", "1.000000000000000001", "100"] {
            assert!(invalid.parse::<Ratio>().is_err());
        }
        let money = |currency: &str| Money {
            amount: "1.2".parse().unwrap(),
            currency: currency.parse().unwrap(),
        };
        assert_eq!(
            money("USD").checked_add(&money("EUR")),
            Err(NumericError::Incompatible)
        );
        assert_eq!(
            money("USD")
                .checked_add(&money("USD"))
                .unwrap()
                .amount
                .to_string(),
            "2.4"
        );
        assert!("usd".parse::<Currency>().is_err());
    }
}
