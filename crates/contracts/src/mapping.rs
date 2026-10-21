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
        let amount = value.value.map(|v| match v {
            d::Value::Tokens(n) => n.0.to_string(),
            d::Value::Money(m) => m.amount.to_string(),
            d::Value::Ratio(r) => r.to_string(),
            d::Value::PercentagePoints(p) => p.to_string(),
            d::Value::Native { amount, .. } => amount.to_string(),
        });
        let mut metric = Self {
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
            query_kind: query.into(),
            alert_eligible: false,
            period: value.period.map(TryInto::try_into).transpose()?,
            reason: value.reason.map(Into::into),
        };
        metric.validate_value().map_err(|_| invalid())?;
        metric.alert_eligible = metric.expected_alert_eligible();
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

// The wire enum is closed; map the journaled kinds exactly.
impl From<quotavista_core::journal::OperationKind> for OperationKind {
    fn from(value: quotavista_core::journal::OperationKind) -> Self {
        match value {
            quotavista_core::journal::OperationKind::Setup => Self::Setup,
            quotavista_core::journal::OperationKind::ReplaceCredentials => Self::ReplaceCredentials,
            quotavista_core::journal::OperationKind::Delete => Self::Delete,
        }
    }
}

// Helpers for the desktop composition layer; not part of the wire schema.
// Pure mapping: no storage lookup, no provider call, no I/O.
impl ConnectionDto {
    pub fn from_domain(
        connection: &quotavista_core::storage::Connection,
        datasets: Vec<DatasetDto>,
        capabilities: Vec<CapabilityDto>,
    ) -> Self {
        Self {
            connection_id: LocalId(connection.id.clone()),
            provider: provider_dto(&connection.provider),
            alias: Alias(connection.alias.clone()),
            lifecycle: connection.lifecycle.into(),
            config_revision: Revision(connection.generation.max(0).to_string()),
            generation: Revision(connection.generation.max(0).to_string()),
            datasets,
            capabilities,
        }
    }
}

impl SettingsDto {
    pub fn from_domain(settings: &quotavista_core::storage::Settings) -> Result<Self, SafeError> {
        Ok(Self {
            settings_revision: Revision(settings.revision.max(0).to_string()),
            ui_locale: parse_locale(&settings.locale)?,
            reporting_zone: Zone(settings.reporting_zone.to_string()),
            display_currency: CurrencyDto(settings.currency.to_string()),
            polling: polling_dto(settings.refresh_minutes),
        })
    }
}

impl OperationDto {
    pub fn from_record(record: &quotavista_core::journal::OperationRecord) -> Self {
        let (state, next_action, retry_at, error) = operation_outcome(record);
        Self {
            operation_id: LocalId(record.operation_id.clone()),
            connection_id: Some(LocalId(record.connection_id.clone())),
            kind: record.kind.into(),
            state,
            next_action,
            retryable: matches!(state, OperationState::Failed),
            retry_at,
            error,
        }
    }
}

fn operation_outcome(
    record: &quotavista_core::journal::OperationRecord,
) -> (
    OperationState,
    NextAction,
    Option<Instant>,
    Option<SafeError>,
) {
    use quotavista_core::journal::OperationPhase;
    let (state, next_action) = match record.phase {
        OperationPhase::Pending
        | OperationPhase::CandidateWritten
        | OperationPhase::CompensationPending => (OperationState::Running, NextAction::None),
        OperationPhase::Committed | OperationPhase::Complete => {
            (OperationState::Succeeded, NextAction::None)
        }
    };
    // The journal record carries neither error code nor retry timestamp;
    // those are projected by the composition layer and stay None here.
    (state, next_action, None, None)
}

fn provider_dto(id: &str) -> Provider {
    match id {
        "kimi" | "synthetic-kimi" => Provider::Kimi,
        "openrouter" | "synthetic-openrouter" => Provider::Openrouter,
        // Unknown providers still need a wire representation: pick Kimi as the
        // sentinel because the wire enum is closed and the caller is choosing
        // between the two declared providers.
        _ => Provider::Kimi,
    }
}

fn polling_dto(minutes: Option<u8>) -> Polling {
    match minutes {
        None => Polling::Disabled,
        Some(1) => Polling::OneMinute,
        Some(5) => Polling::FiveMinutes,
        Some(15) => Polling::FifteenMinutes,
        // Anything else (including corrupted values) collapses to Disabled so
        // a malformed settings row can never enable an unstated interval.
        _ => Polling::Disabled,
    }
}

fn parse_locale(value: &str) -> Result<Locale, SafeError> {
    match value {
        "en" => Ok(Locale::En),
        "zh-CN" => Ok(Locale::ZhCn),
        _ => Err(SafeError::new(ErrorCode::InternalError, None)),
    }
}
