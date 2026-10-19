//! Contract definitions only; runtime command registration belongs to P1-07.
//! Domain types deliberately have no serde/TS derives. JSON integers are strings.
use quotavista_core::{calendar, domain as d, number as n};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub mod mapping;
pub const CONTRACT_VERSION: u16 = 1;

// All string scalars validate at deserialization, not only at UI entry points.
macro_rules! scalar {
    ($name:ident, $check:expr, $normalize:expr) => {
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl TryFrom<String> for $name {
            type Error = &'static str;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                let value = ($normalize)(value);
                if ($check)(&value) {
                    Ok(Self(value))
                } else {
                    Err("invalid contract scalar")
                }
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
    ($name:ident, $check:expr) => {
        scalar!($name, $check, |value| value);
    };
}
fn integer(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 39
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|b| b.is_ascii_digit())
}
scalar!(TokenCountDto, |s: &str| integer(s)
    && s.parse::<u64>().is_ok());
scalar!(TokenTotalDto, |s: &str| integer(s)
    && s.parse::<u128>().is_ok());
scalar!(Revision, |s: &str| integer(s) && s.parse::<u64>().is_ok());
scalar!(DecimalDto, |s: &str| s.len() <= 48
    && s.parse::<n::Decimal>().is_ok_and(|v| v.to_string() == s));
scalar!(RatioDto, |s: &str| s.len() <= 20
    && s.parse::<n::Ratio>().is_ok_and(|v| v.to_string() == s));
scalar!(CurrencyDto, |s: &str| s.parse::<n::Currency>().is_ok());
scalar!(LocalId, |s: &str| !s.is_empty()
    && s.len() <= 128
    && s.bytes().all(|b| b.is_ascii_alphanumeric()
        || b == b'-'
        || b == b'_'));
scalar!(Date, |s: &str| calendar::parse_date(s).is_ok());
scalar!(Instant, |s: &str| s.len() <= 40
    && s.ends_with('Z')
    && calendar::parse_instant(s).is_ok());
scalar!(Zone, |s: &str| s.len() <= 128 && calendar::valid_zone(s));
scalar!(
    Alias,
    |s: &str| !s.is_empty() && s.chars().count() <= 80 && !s.chars().any(char::is_control),
    |s: String| s.trim().to_owned()
);

macro_rules! wire_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
    };
}
macro_rules! domain_enum {
    ($name:ident, $domain:ident { $($variant:ident),+ $(,)? }) => {
        wire_enum!($name { $($variant),+ });
        impl From<d::$domain> for $name { fn from(value: d::$domain) -> Self { match value { $(d::$domain::$variant => Self::$variant),+ } } }
    };
}
domain_enum!(
    ProvenanceDto,
    Provenance {
        Synthetic,
        OfficialEvent,
        OfficialBucket,
        ValidatedDerivation,
        Observation
    }
);
domain_enum!(
    LifecycleDto,
    Lifecycle {
        PendingSetup,
        Active,
        Disabled,
        NeedsCredentials,
        DeletionPending
    }
);
domain_enum!(
    QueryHealthDto,
    QueryHealth {
        NotQueried,
        Running,
        Success,
        Offline,
        Timeout,
        RateLimited,
        AuthenticationRequired,
        PermissionRequired,
        InvalidResponse,
        LocalStorageError
    }
);
domain_enum!(
    FreshnessDto,
    Freshness {
        NoValue,
        Current,
        Stale
    }
);
domain_enum!(
    CoverageStatusDto,
    CoverageStatus {
        Complete,
        Partial,
        Missing,
        Unsupported
    }
);
domain_enum!(
    AvailabilityDto,
    Availability {
        Available,
        Unsupported,
        PermissionRequired,
        Unverified,
        Missing
    }
);
domain_enum!(
    ReasonDto,
    Reason {
        PermissionRequired,
        NotYetSettled,
        OutsideLookback,
        Truncated,
        BucketBoundaryMismatch,
        CollectionGap,
        SyncPending,
        Unsupported,
        NoData,
        IncompatibleUnit,
        UnknownScope
    }
);
wire_enum!(Provider { Kimi, Openrouter });
wire_enum!(StorageMode {
    Persistent,
    SessionOnly
});
wire_enum!(CredentialKind {
    ApiKey,
    ManagementKey,
    SessionToken
});
wire_enum!(Grain { Day, Week });
wire_enum!(Polling {
    Disabled,
    OneMinute,
    FiveMinutes,
    FifteenMinutes
});
wire_enum!(DeletionIntent {
    CredentialsHistoryAndManagedBackups
});
wire_enum!(ErrorCode {
    Validation,
    Conflict,
    NeedsCredentials,
    AuthenticationRequired,
    PermissionRequired,
    SecretStoreLocked,
    SecretStoreUnavailable,
    RateLimited,
    Offline,
    Timeout,
    InvalidResponse,
    ResourceLimit,
    StorageError,
    Cancelled
});
wire_enum!(OperationState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled
});
wire_enum!(OperationKind {
    Setup,
    ReplaceCredentials,
    Delete,
    Refresh
});
wire_enum!(NextAction {
    None,
    Retry,
    UnlockSecretStore,
    ProvideCredentials,
    ReplaceConnection
});
wire_enum!(SubcountRelationDto {
    Included,
    Additional,
    Unknown
});

macro_rules! dto {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}
// Explicit locale values preserve the product preference independently of zone.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, TS)]
pub enum Locale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

dto!(IntervalDto {
    start: Instant,
    end: Instant
});
dto!(PeriodDto { key: Option<String>, interval: IntervalDto, source_zone: Zone, expired: bool });
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UnitDto {
    Tokens,
    Money { currency: CurrencyDto },
    Ratio,
    PercentagePoints,
    Native { name: String },
}
dto!(TokenFactDto { input: Option<TokenCountDto>, output: Option<TokenCountDto>, cached: Option<TokenCountDto>, reasoning: Option<TokenCountDto>, cached_relation: SubcountRelationDto, reasoning_relation: SubcountRelationDto });
dto!(CoverageDto { interval: IntervalDto, status: CoverageStatusDto, reason: Option<ReasonDto> });
dto!(MetricDto {
    metric_id: LocalId, scope_id: LocalId, product_id: Option<LocalId>, unit: UnitDto,
    value: Option<String>, availability: AvailabilityDto, provenance: ProvenanceDto,
    observed_at: Instant, source_as_of: Option<Instant>, freshness: FreshnessDto,
    alert_eligible: bool, period: Option<PeriodDto>, reason: Option<ReasonDto>
});
dto!(ExcludedScope {
    scope_id: LocalId,
    metric_id: LocalId,
    reason: ReasonDto
});
dto!(StatisticsPoint { interval: IntervalDto, metrics: Vec<MetricDto>, coverage: Vec<CoverageDto> });
dto!(PlanRow { scope_id: LocalId, product_id: LocalId, period: Option<PeriodDto>, metrics: Vec<MetricDto> });
dto!(StatisticsDto {
    requested_interval: IntervalDto, effective_intervals: Vec<IntervalDto>, reporting_zone: Zone,
    grain: Grain, as_of: Instant, provisional: bool, coverage: Vec<CoverageDto>, excluded_scopes: Vec<ExcludedScope>,
    totals: Vec<MetricDto>, series: Vec<StatisticsPoint>, plans: Vec<PlanRow>, data_revision: Revision
});
dto!(OverviewDto { metrics: Vec<MetricDto>, summary: StatisticsDto });
dto!(CapabilityDto { metric_id: LocalId, availability: AvailabilityDto, authorization_category: String, source_grain: Option<Grain>, historical_interval: Option<IntervalDto>, reporting_zone: Option<Zone> });
dto!(DatasetDto { dataset_id: LocalId, health: QueryHealthDto, last_success_at: Option<Instant>, error: Option<SafeError>, coverage: Vec<CoverageDto> });
dto!(ConnectionDto { connection_id: LocalId, provider: Provider, alias: Alias, lifecycle: LifecycleDto, config_revision: Revision, generation: Revision, datasets: Vec<DatasetDto>, capabilities: Vec<CapabilityDto> });
dto!(ConnectionList { connections: Vec<ConnectionDto>, data_revision: Revision });
dto!(SettingsDto {
    settings_revision: Revision,
    ui_locale: Locale,
    reporting_zone: Zone,
    display_currency: CurrencyDto,
    polling: Polling
});
dto!(SettingsPatch { ui_locale: Option<Locale>, reporting_zone: Option<Zone>, display_currency: Option<CurrencyDto>, polling: Option<Polling> });
dto!(OperationDto { operation_id: LocalId, connection_id: Option<LocalId>, kind: OperationKind, state: OperationState, next_action: NextAction, retryable: bool, retry_at: Option<Instant>, error: Option<SafeError> });
dto!(OperationList { operations: Vec<OperationDto> });
dto!(ChangeEvent { data_revision: Revision, affected_connection_ids: Vec<LocalId>, affected_scope_ids: Vec<LocalId> });

// No caller-supplied prose: message is selected only from a closed taxonomy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SafeError {
    pub code: ErrorCode,
    pub retryable: bool,
    pub retry_at: Option<Instant>,
    message: String,
}
impl SafeError {
    pub fn new(code: ErrorCode, retry_at: Option<Instant>) -> Self {
        let retryable = matches!(
            code,
            ErrorCode::SecretStoreLocked
                | ErrorCode::SecretStoreUnavailable
                | ErrorCode::RateLimited
                | ErrorCode::Offline
                | ErrorCode::Timeout
                | ErrorCode::StorageError
        );
        Self {
            code,
            retryable,
            retry_at,
            message: (match code {
                ErrorCode::Validation => "Invalid request.",
                ErrorCode::Conflict => "Configuration changed; reload and retry.",
                ErrorCode::NeedsCredentials => "Credentials are required.",
                ErrorCode::AuthenticationRequired => "Authentication is required.",
                ErrorCode::PermissionRequired => "Additional permission is required.",
                ErrorCode::SecretStoreLocked => "Unlock the credential store.",
                ErrorCode::SecretStoreUnavailable => "The credential store is unavailable.",
                ErrorCode::RateLimited => "Wait before retrying.",
                ErrorCode::Offline => "The provider is unreachable.",
                ErrorCode::Timeout => "The request timed out.",
                ErrorCode::InvalidResponse => "The provider response is invalid.",
                ErrorCode::ResourceLimit => "The request exceeds a resource limit.",
                ErrorCode::StorageError => "Local storage failed.",
                ErrorCode::Cancelled => "The operation was cancelled.",
            })
            .to_owned(),
        }
    }
}

/// Input-only secret: neither Serialize nor Debug. Never included in results/events.
#[derive(Deserialize, TS)]
#[serde(try_from = "String")]
pub struct SecretInput(String);
impl TryFrom<String> for SecretInput {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() || value.len() > 65536 || value.contains(['\r', '\n']) {
            return Err("invalid credential input");
        }
        Ok(Self(value))
    }
}
impl SecretInput {
    pub fn expose_for_provider(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    content = "connection_ids",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RefreshTarget {
    All,
    Selected(Vec<LocalId>),
}
dto!(RangeInput { start_date: Date, end_date: Date, reporting_zone: Zone, grain: Grain, scope_ids: Option<Vec<LocalId>> });
#[derive(Deserialize, TS)]
#[serde(
    tag = "command",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Command {
    ListConnections,
    ConnectProvider {
        provider: Provider,
        alias: Alias,
        credential_kind: CredentialKind,
        secret: SecretInput,
        storage_mode: StorageMode,
    },
    UpdateConnection {
        connection_id: LocalId,
        expected_config_revision: Revision,
        alias: Alias,
    },
    ReplaceCredentials {
        connection_id: LocalId,
        expected_config_revision: Revision,
        credential_kind: CredentialKind,
        secret: SecretInput,
        storage_mode: StorageMode,
    },
    SetConnectionEnabled {
        connection_id: LocalId,
        expected_config_revision: Revision,
        enabled: bool,
    },
    RemoveConnection {
        connection_id: LocalId,
        expected_config_revision: Revision,
        deletion_intent: DeletionIntent,
    },
    RefreshConnections {
        target: RefreshTarget,
    },
    GetOverview {
        range: RangeInput,
    },
    QueryStatistics {
        range: RangeInput,
    },
    UpdateSettings {
        expected_settings_revision: Revision,
        changes: SettingsPatch,
    },
    GetOperation {
        operation_id: LocalId,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "command",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CommandResult {
    ListConnections(ConnectionList),
    ConnectProvider(OperationDto),
    UpdateConnection(ConnectionDto),
    ReplaceCredentials(OperationDto),
    SetConnectionEnabled(ConnectionDto),
    RemoveConnection(OperationDto),
    RefreshConnections(OperationList),
    GetOverview(OverviewDto),
    QueryStatistics(StatisticsDto),
    UpdateSettings(SettingsDto),
    GetOperation(OperationDto),
}
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub contract_version: u16,
    pub request_id: LocalId,
    pub command: Command,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Envelope {
    Ok {
        contract_version: u16,
        request_id: LocalId,
        data: Box<CommandResult>,
    },
    Error {
        contract_version: u16,
        request_id: LocalId,
        error: SafeError,
    },
}
impl Request {
    /// Pure validation before any network/storage side effect. ID existence and revision
    /// conflicts still require application lookups in P1-07.
    pub fn validate(&self, clock: &impl calendar::Clock) -> Result<(), SafeError> {
        let invalid = || SafeError::new(ErrorCode::Validation, None);
        if self.contract_version != CONTRACT_VERSION {
            return Err(invalid());
        }
        match &self.command {
            Command::GetOverview { range } | Command::QueryStatistics { range } => {
                calendar::calendar_range(
                    range.start_date.as_str(),
                    range.end_date.as_str(),
                    range.reporting_zone.as_str(),
                    clock,
                )
                .map_err(|_| invalid())?;
                if range.scope_ids.as_ref().is_some_and(|ids| ids.len() > 100) {
                    return Err(invalid());
                }
            }
            Command::RefreshConnections {
                target: RefreshTarget::Selected(ids),
            } if ids.is_empty() || ids.len() > 100 => return Err(invalid()),
            _ => {}
        }
        Ok(())
    }
}

/// One deterministic registry; includes request-only scalars and all response types.
pub fn typescript() -> String {
    let cfg = ts_rs::Config::default();
    let mut out = String::from("// Generated by quotavista-contracts; do not edit.\n");
    macro_rules! emit { ($($ty:ty),+ $(,)?) => { $(out.push_str("export "); out.push_str(&<$ty>::decl(&cfg)); out.push('\n');)+ }; }
    emit!(
        TokenCountDto,
        TokenTotalDto,
        Revision,
        DecimalDto,
        RatioDto,
        CurrencyDto,
        LocalId,
        Date,
        Instant,
        Zone,
        Alias,
        ProvenanceDto,
        LifecycleDto,
        QueryHealthDto,
        FreshnessDto,
        CoverageStatusDto,
        AvailabilityDto,
        ReasonDto,
        Provider,
        Locale,
        StorageMode,
        CredentialKind,
        Grain,
        Polling,
        DeletionIntent,
        ErrorCode,
        OperationState,
        OperationKind,
        NextAction,
        SubcountRelationDto,
        IntervalDto,
        PeriodDto,
        UnitDto,
        TokenFactDto,
        CoverageDto,
        MetricDto,
        ExcludedScope,
        StatisticsPoint,
        PlanRow,
        StatisticsDto,
        OverviewDto,
        CapabilityDto,
        DatasetDto,
        ConnectionDto,
        ConnectionList,
        SettingsDto,
        SettingsPatch,
        OperationDto,
        OperationList,
        ChangeEvent,
        SafeError,
        SecretInput,
        RefreshTarget,
        RangeInput,
        Command,
        CommandResult,
        Request,
        Envelope
    );
    out
}
