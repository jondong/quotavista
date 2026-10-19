//! Explicit, fallible domain-to-wire mappings. No domain serialization shortcuts.
use super::*;
fn invalid() -> SafeError {
    SafeError::new(ErrorCode::InternalError, None)
}
impl From<n::TokenCount> for TokenCountDto {
    fn from(value: n::TokenCount) -> Self {
        Self(value.0.to_string())
    }
}
impl From<n::TokenTotal> for TokenTotalDto {
    fn from(value: n::TokenTotal) -> Self {
        Self(value.0.to_string())
    }
}
impl From<n::Decimal> for DecimalDto {
    fn from(value: n::Decimal) -> Self {
        Self(value.to_string())
    }
}
impl From<n::Ratio> for RatioDto {
    fn from(value: n::Ratio) -> Self {
        Self(value.to_string())
    }
}
impl From<TokenCountDto> for n::TokenCount {
    fn from(value: TokenCountDto) -> Self {
        Self(value.0.parse().expect("validated token count"))
    }
}
impl From<TokenTotalDto> for n::TokenTotal {
    fn from(value: TokenTotalDto) -> Self {
        Self(value.0.parse().expect("validated token total"))
    }
}
impl TryFrom<calendar::Interval> for IntervalDto {
    type Error = SafeError;
    fn try_from(value: calendar::Interval) -> Result<Self, Self::Error> {
        calendar::Interval::new(value.start, value.end).map_err(|_| invalid())?;
        Ok(Self {
            start: Instant(value.start.to_rfc3339().replace("+00:00", "Z")),
            end: Instant(value.end.to_rfc3339().replace("+00:00", "Z")),
        })
    }
}
impl TryFrom<d::Period> for PeriodDto {
    type Error = SafeError;
    fn try_from(value: d::Period) -> Result<Self, Self::Error> {
        Ok(Self {
            key: value
                .key
                .map(TryInto::try_into)
                .transpose()
                .map_err(|_| invalid())?,
            interval: value.interval.try_into()?,
            source_zone: value.source_zone.try_into().map_err(|_| invalid())?,
            expired: value.expired,
        })
    }
}
impl TryFrom<d::Coverage> for CoverageDto {
    type Error = SafeError;
    fn try_from(value: d::Coverage) -> Result<Self, Self::Error> {
        Ok(Self {
            interval: value.interval.try_into()?,
            status: value.status.into(),
            reason: value.reason.map(Into::into),
        })
    }
}
impl TryFrom<d::Unit> for UnitDto {
    type Error = SafeError;
    fn try_from(value: d::Unit) -> Result<Self, Self::Error> {
        Ok(match value {
            d::Unit::Tokens => Self::Tokens,
            d::Unit::Money(c) => Self::Money {
                currency: CurrencyDto(c.to_string()),
            },
            d::Unit::Ratio => Self::Ratio,
            d::Unit::PercentagePoints => Self::PercentagePoints,
            d::Unit::Native(name) => Self::Native {
                name: name.try_into().map_err(|_| invalid())?,
            },
        })
    }
}
impl MetricDto {
    pub fn from_domain(value: d::Metric, query: d::QueryKind) -> Result<Self, SafeError> {
        if !value.validate() {
            return Err(invalid());
        }
        let alert_eligible = value.alert_eligible(query);
        let amount = value.value.map(|v| match v {
            d::Value::Tokens(n) => n.0.to_string(),
            d::Value::Money(m) => m.amount.to_string(),
            d::Value::Ratio(r) => r.to_string(),
            d::Value::PercentagePoints(p) => p.to_string(),
            d::Value::Native { amount, .. } => amount.to_string(),
        });
        let metric = Self {
            metric_id: value.metric_id.try_into().map_err(|_| invalid())?,
            scope_id: value.scope_id.try_into().map_err(|_| invalid())?,
            product_id: value
                .product_id
                .map(TryInto::try_into)
                .transpose()
                .map_err(|_| invalid())?,
            unit: value.unit.try_into()?,
            value: amount,
            availability: value.availability.into(),
            provenance: value.provenance.into(),
            observed_at: Instant(value.observed_at.to_rfc3339().replace("+00:00", "Z")),
            source_as_of: value
                .source_as_of
                .map(|at| Instant(at.to_rfc3339().replace("+00:00", "Z"))),
            freshness: value.freshness.into(),
            alert_eligible,
            period: value.period.map(TryInto::try_into).transpose()?,
            reason: value.reason.map(Into::into),
        };
        metric.validate_value().map_err(|_| invalid())?;
        Ok(metric)
    }
}
impl From<n::SubcountRelation> for SubcountRelationDto {
    fn from(value: n::SubcountRelation) -> Self {
        match value {
            n::SubcountRelation::Included => Self::Included,
            n::SubcountRelation::Additional => Self::Additional,
            n::SubcountRelation::Unknown => Self::Unknown,
        }
    }
}
impl From<n::TokenFact> for TokenFactDto {
    fn from(value: n::TokenFact) -> Self {
        Self {
            input: value.input.map(Into::into),
            output: value.output.map(Into::into),
            cached: value.cached.map(Into::into),
            reasoning: value.reasoning.map(Into::into),
            cached_relation: value.cached_relation.into(),
            reasoning_relation: value.reasoning_relation.into(),
        }
    }
}
