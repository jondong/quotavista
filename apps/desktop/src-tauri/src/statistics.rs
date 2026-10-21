//! Overview and statistics query handlers (P1-09).
//!
//! The handlers are thin: recompute the validated calendar range, resolve the
//! selected scopes, read committed snapshots and published partitions through
//! the storage port, and hand everything to the pure aggregation engine in
//! `quotavista_core::statistics`. The renderer receives display-ready objects;
//! no accounting happens here beyond what the engine already did.

use std::collections::BTreeSet;

use quotavista_contracts::{
    ErrorCode, Grain as WireGrain, LocalId, OverviewDto, OverviewRangeInput, RangeInput, SafeError,
    StatisticsDto,
};
use quotavista_core::calendar::{self, CalendarRange};
use quotavista_core::domain::Grain;
use quotavista_core::statistics::{self, ScopeInput};
use quotavista_core::storage::Repository;
use quotavista_providers::simulated::synthetic_capability;

use crate::{safe, storage_error, AppState, UtcClock};

/// Overview: per-scope current values (quota, spend, tokens with source
/// timestamps) plus a day-grain summary of the requested range.
pub async fn get_overview(
    state: &AppState,
    input: OverviewRangeInput,
) -> Result<OverviewDto, SafeError> {
    let range = calendar_range(&input)?;
    let scopes = scope_inputs(state, input.scope_ids.as_deref()).await?;
    let scopes = read_partitions(state, &range, scopes).await?;
    let metrics = scopes
        .iter()
        .flat_map(|scope| statistics::current_metrics(scope, range.as_of))
        .collect();
    let summary = run_statistics(state, &range, Grain::Day, &scopes).await?;
    OverviewDto::from_domain(metrics, summary)
}

/// Statistics: range totals, day/week series, UTC-only lanes, coverage,
/// exclusions, and per-plan native-unit rows.
pub async fn query_statistics(
    state: &AppState,
    input: RangeInput,
) -> Result<StatisticsDto, SafeError> {
    let range = calendar_range(&input)?;
    let scopes = scope_inputs(state, input.scope_ids.as_deref()).await?;
    let scopes = read_partitions(state, &range, scopes).await?;
    let grain = match input.grain {
        WireGrain::Day => Grain::Day,
        WireGrain::Week => Grain::Week,
    };
    run_statistics(state, &range, grain, &scopes).await
}

trait CalendarInput {
    fn start(&self) -> &str;
    fn end(&self) -> &str;
    fn zone(&self) -> &str;
}

impl CalendarInput for OverviewRangeInput {
    fn start(&self) -> &str {
        self.start_date.as_str()
    }
    fn end(&self) -> &str {
        self.end_date.as_str()
    }
    fn zone(&self) -> &str {
        self.reporting_zone.as_str()
    }
}

impl CalendarInput for RangeInput {
    fn start(&self) -> &str {
        self.start_date.as_str()
    }
    fn end(&self) -> &str {
        self.end_date.as_str()
    }
    fn zone(&self) -> &str {
        self.reporting_zone.as_str()
    }
}

/// The request was already validated at the decode boundary; recomputing here
/// pins the calendar boundaries to the one as-of this handler captures.
fn calendar_range(input: &impl CalendarInput) -> Result<CalendarRange, SafeError> {
    calendar::calendar_range(input.start(), input.end(), input.zone(), &UtcClock)
        .map_err(|_| safe(ErrorCode::Validation))
}

async fn run_statistics(
    state: &AppState,
    range: &CalendarRange,
    grain: Grain,
    scopes: &[ScopeInput],
) -> Result<StatisticsDto, SafeError> {
    let statistics =
        statistics::aggregate(range, grain, scopes).map_err(|_| safe(ErrorCode::InternalError))?;
    let revision = state
        .repository
        .data_revision()
        .await
        .map_err(storage_error)?
        .max(0) as u64;
    StatisticsDto::from_domain(statistics, revision)
}

/// Resolve the scope filter against the committed store. Unknown selected ids
/// are rejected instead of silently narrowing the query.
async fn scope_inputs(
    state: &AppState,
    selected: Option<&[LocalId]>,
) -> Result<Vec<ScopeInput>, SafeError> {
    let selected: Option<BTreeSet<&str>> =
        selected.map(|ids| ids.iter().map(|id| id.as_str()).collect());
    let connections = state
        .repository
        .connections()
        .await
        .map_err(storage_error)?;
    let mut matched = BTreeSet::new();
    let mut scopes = Vec::new();
    for connection in &connections {
        let (token_dataset, quota_supported) = synthetic_capability(&connection.provider);
        for scope_id in state
            .repository
            .scope_ids(&connection.id)
            .await
            .map_err(storage_error)?
        {
            if let Some(selected) = &selected {
                if !selected.contains(scope_id.as_str()) {
                    continue;
                }
            }
            matched.insert(scope_id.clone());
            let snapshots = state
                .repository
                .snapshots(std::slice::from_ref(&scope_id))
                .await
                .map_err(storage_error)?;
            scopes.push(ScopeInput {
                scope_id,
                token_dataset: token_dataset.to_string(),
                quota_supported,
                snapshots,
                partitions: Vec::new(),
            });
        }
    }
    if let Some(selected) = &selected {
        if matched.len() != selected.len() {
            return Err(safe(ErrorCode::Validation));
        }
    }
    Ok(scopes)
}

/// Fill in the published partitions of every selected scope. A fully future
/// range has no effective interval, so no fact read happens at all.
async fn read_partitions(
    state: &AppState,
    range: &CalendarRange,
    scopes: Vec<ScopeInput>,
) -> Result<Vec<ScopeInput>, SafeError> {
    let mut out = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let partitions = match &range.effective {
            Some(effective) => state
                .repository
                .published_partitions(std::slice::from_ref(&scope.scope_id), effective)
                .await
                .map_err(storage_error)?,
            None => Vec::new(),
        };
        out.push(ScopeInput {
            partitions,
            ..scope
        });
    }
    Ok(out)
}
