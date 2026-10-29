//! Kimi shared-membership pool parsing and mapping.
//!
//! Wire shapes in the bundled fixtures are sanitized, invented, and
//! UNVALIDATED: no authorized session was available while writing them, so
//! field spellings and casing still need confirmation during live
//! reconciliation against the official membership page. Parsed strictly
//! anyway: a shape mismatch must surface as `InvalidResponse`, never as a
//! guessed quota.
//!
//! Scope rule (A02): only the shared membership pool is mapped. A response
//! that carries API-wallet or independent Code-limit sections but no shared
//! pool is an [`ProviderError::InvalidResponse`] for the `shared-pool`
//! dataset — those sections are never substituted for membership quota.
//!
//! Unit rule: consumption stays percentage points and remaining stays a
//! ratio. Token totals are never invented from percentages or prices, so the
//! dataset's `tokens` metric is always reported `Missing`/`NoData`.

use quotavista_core::domain::{Availability, Provenance, QueryKind, Reason, Value};
use quotavista_core::number::{Decimal, Ratio};
use quotavista_core::provider::{CurrentMetric, ProviderError, ProviderRequest};
use serde::Deserialize;

/// Membership endpoint of the Kimi web application. UNVALIDATED placeholder:
/// the exact path must be confirmed against the official membership page
/// during live reconciliation (TODO live validation). Requires an authorized
/// session token at runtime; queried read-only.
pub(crate) const MEMBERSHIP_PATH: &str = "/api/membership/shared-pool";

/// Dataset identifier shared with the synthetic demo (`shared-pool`): the
/// shared membership pool, explicitly distinct from the API wallet and from
/// independent Code limits.
pub(crate) const SHARED_POOL_DATASET: &str = "shared-pool";

/// Maximum fractional digits accepted for a percentage literal. The core
/// `Decimal` allows 18; the cap is 16 here so the derived remaining ratio
/// (percentage points shifted two places left) always fits the 18-digit
/// decimal domain exactly, with no rounding.
const MAX_FRACTION_DIGITS: usize = 16;

#[derive(Deserialize)]
struct WireMembership<'a> {
    #[serde(borrow)]
    data: WireMembershipData<'a>,
}

#[derive(Deserialize)]
struct WireMembershipData<'a> {
    /// The shared membership pool section. Absent means the response cannot
    /// state membership quota; sibling sections (API wallet, Code limits)
    /// are deliberately not read as substitutes.
    #[serde(borrow, default)]
    shared_pool: Option<WireSharedPool<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSharedPool<'a> {
    /// Consumed percentage points of the shared pool, `"0"`–`"100"`, as a
    /// string or bare JSON number.
    #[serde(borrow)]
    consumed_percent: &'a serde_json::value::RawValue,
    /// Next refresh instant of the shared pool, RFC 3339, when the source
    /// exposes it.
    #[serde(default)]
    refreshes_at: Option<String>,
}

/// Parse one raw percentage literal into a `Decimal`. Accepts `"30"`,
/// `"33.5"`, `30`, and `33.5`; rejects anything else (signs, exponents,
/// escaped digits, objects) as `InvalidResponse`. Mirrors the OpenRouter
/// amount parser: no floating-point value is ever constructed.
fn percent_decimal(raw: &serde_json::value::RawValue) -> Result<Decimal, ProviderError> {
    let text = raw.get();
    let literal = if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        &text[1..text.len() - 1]
    } else {
        text
    };
    if literal.bytes().any(|b| !b.is_ascii_digit() && b != b'.')
        || literal.split('.').count() > 2
        || literal.ends_with('.')
        || literal.starts_with('.')
        || (literal.contains('.')
            && literal
                .split('.')
                .nth(1)
                .is_some_and(|f| f.len() > MAX_FRACTION_DIGITS))
    {
        return Err(ProviderError::InvalidResponse);
    }
    literal.parse().map_err(|_| ProviderError::InvalidResponse)
}

/// Split the normalized `Display` form of a `Decimal` back into an integer
/// coefficient and its scale, using only public core APIs.
fn coefficient_scale(value: &Decimal) -> (u128, u32) {
    let text = value.to_string();
    match text.split_once('.') {
        Some((whole, fraction)) => (
            format!("{whole}{fraction}").parse().expect("digits only"),
            fraction.len() as u32,
        ),
        None => (text.parse().expect("digits only"), 0),
    }
}

/// Exact `1 - consumed / 100` as a [`Ratio`]. `consumed` is percentage points
/// in `[0, 100]`; anything above 100 is a malformed response, never a clamped
/// value. Arithmetic is manual coefficient alignment because core `Decimal`
/// is nonnegative-only and has no subtraction operator.
fn remaining_ratio(consumed: Decimal) -> Result<Ratio, ProviderError> {
    let (coefficient, scale) = coefficient_scale(&consumed);
    let hundred = 100u128
        .checked_mul(10u128.pow(scale))
        .ok_or(ProviderError::InvalidResponse)?;
    if coefficient > hundred {
        return Err(ProviderError::InvalidResponse);
    }
    let remaining = hundred - coefficient;
    // Convert remaining percentage points to a ratio by shifting the decimal
    // point two places left: 70 -> "0.70", 33.5 -> "0.335", 0.005 -> "0.00005".
    let digits = format!("{remaining:0width$}", width = scale as usize + 1);
    let split = digits.len() - scale as usize;
    let (whole, fraction) = (&digits[..split], &digits[split..]);
    let ratio_text = if whole == "100" {
        debug_assert!(fraction.bytes().all(|b| b == b'0'));
        "1".to_owned()
    } else {
        format!("0.{whole:0>2}{fraction}")
    };
    ratio_text
        .parse()
        .map_err(|_| ProviderError::InvalidResponse)
}

fn pool_metric(
    metric: &str,
    value: Option<Value>,
    availability: Availability,
    reason: Option<Reason>,
    request: &ProviderRequest,
) -> CurrentMetric {
    CurrentMetric {
        dataset: SHARED_POOL_DATASET.to_owned(),
        metric: metric.to_owned(),
        product_key: None,
        value,
        availability,
        reason,
        query_kind: QueryKind::Unvalidated,
        provenance: Provenance::Observation,
        observed_at: request.as_of,
        source_as_of: None,
    }
}

/// Parse a membership response body into shared-pool current metrics.
///
/// Emits `quota_consumed` (percentage points, directly observed) and
/// `quota_remaining` (ratio, the exact arithmetic complement of the observed
/// consumption), plus `quota_refreshes_at` when the source exposes the next
/// refresh instant. The refresh instant is carried as
/// `Value::Native { unit: "unix_seconds" }` — the domain has no timestamp
/// value shape, and the metric stays `Unvalidated` until live reconciliation
/// confirms the field. The dataset's `tokens` metric is always
/// `Missing`/`NoData`: percentage points cannot yield token totals and none
/// are invented. The membership envelope carries no server-side observation
/// timestamp, so `source_as_of` stays `None` until reconciliation confirms
/// one (TODO live validation).
pub(crate) fn membership_metrics(
    body: &str,
    request: &ProviderRequest,
) -> Result<Vec<CurrentMetric>, ProviderError> {
    let wire: WireMembership =
        serde_json::from_str(body).map_err(|_| ProviderError::InvalidResponse)?;
    let pool = wire
        .data
        .shared_pool
        .ok_or(ProviderError::InvalidResponse)?;
    let consumed = percent_decimal(pool.consumed_percent)?;
    let remaining = remaining_ratio(consumed)?;
    let mut metrics = vec![
        pool_metric(
            "tokens",
            None,
            Availability::Missing,
            Some(Reason::NoData),
            request,
        ),
        pool_metric(
            "quota_consumed",
            Some(Value::PercentagePoints(consumed)),
            Availability::Available,
            None,
            request,
        ),
        pool_metric(
            "quota_remaining",
            Some(Value::Ratio(remaining)),
            Availability::Available,
            None,
            request,
        ),
    ];
    if let Some(refreshes_at) = pool.refreshes_at {
        let instant = chrono::DateTime::parse_from_rfc3339(&refreshes_at)
            .map_err(|_| ProviderError::InvalidResponse)?;
        let unix_seconds = instant.timestamp();
        if unix_seconds < 0 {
            return Err(ProviderError::InvalidResponse);
        }
        metrics.push(pool_metric(
            "quota_refreshes_at",
            Some(Value::Native {
                amount: unix_seconds
                    .to_string()
                    .parse()
                    .map_err(|_| ProviderError::InvalidResponse)?,
                unit: "unix_seconds".to_owned(),
            }),
            Availability::Available,
            None,
            request,
        ));
    }
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ProviderRequest {
        ProviderRequest {
            connection_id: "kimi-live-1".to_owned(),
            as_of: "2026-10-29T12:00:00Z".parse().unwrap(),
        }
    }

    fn metric<'a>(metrics: &'a [CurrentMetric], name: &str) -> &'a CurrentMetric {
        metrics
            .iter()
            .find(|m| m.metric == name)
            .unwrap_or_else(|| panic!("metric {name} present"))
    }

    #[test]
    fn parses_shared_pool_from_fixture() {
        let body = include_str!("fixtures/membership_ok.json");
        let metrics = membership_metrics(body, &request()).unwrap();
        assert_eq!(metrics.len(), 4);
        let consumed = metric(&metrics, "quota_consumed");
        assert_eq!(
            consumed.value,
            Some(Value::PercentagePoints("30".parse().unwrap()))
        );
        let remaining = metric(&metrics, "quota_remaining");
        assert_eq!(remaining.value, Some(Value::Ratio("0.7".parse().unwrap())));
        let refresh = metric(&metrics, "quota_refreshes_at");
        let Some(Value::Native { amount, unit }) = refresh.value.as_ref() else {
            panic!("refresh metric must carry a native value");
        };
        assert_eq!(unit, "unix_seconds");
        // 2026-11-01T00:00:00Z in unix seconds.
        assert_eq!(amount.to_string(), "1793491200");
        for m in &metrics {
            m.validate().unwrap();
            assert_eq!(m.dataset, SHARED_POOL_DATASET);
            assert_eq!(m.provenance, Provenance::Observation);
            assert_eq!(m.query_kind, QueryKind::Unvalidated);
            assert_eq!(m.source_as_of, None);
        }
    }

    #[test]
    fn tokens_metric_is_missing_never_invented() {
        let body = include_str!("fixtures/membership_ok.json");
        let metrics = membership_metrics(body, &request()).unwrap();
        let tokens = metric(&metrics, "tokens");
        assert_eq!(tokens.availability, Availability::Missing);
        assert_eq!(tokens.reason, Some(Reason::NoData));
        assert!(tokens.value.is_none());
        tokens.validate().unwrap();
    }

    #[test]
    fn missing_refresh_time_omits_the_metric() {
        let body = include_str!("fixtures/membership_no_refresh.json");
        let metrics = membership_metrics(body, &request()).unwrap();
        assert_eq!(metrics.len(), 3);
        assert!(metrics.iter().all(|m| m.metric != "quota_refreshes_at"));
        assert!(metrics.iter().all(|m| m.validate().is_ok()));
    }

    #[test]
    fn wallet_only_payload_is_invalid_never_substituted() {
        // The envelope answers API-wallet and Code-limit sections but no
        // shared pool: membership quota stays unknown, nothing substitutes.
        let body = include_str!("fixtures/membership_wallet_only.json");
        assert_eq!(
            membership_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn missing_consumed_percent_is_invalid() {
        let body = include_str!("fixtures/membership_missing_field.json");
        assert_eq!(
            membership_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn overconsumption_is_invalid_not_clamped() {
        let body = include_str!("fixtures/membership_overconsumption.json");
        assert_eq!(
            membership_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn precision_violation_is_invalid() {
        let body = include_str!("fixtures/membership_precision_violation.json");
        assert_eq!(
            membership_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn malformed_body_is_invalid() {
        let body = include_str!("fixtures/membership_malformed.txt");
        assert_eq!(
            membership_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn bad_refresh_timestamp_is_invalid() {
        let body =
            r#"{"data":{"shared_pool":{"consumed_percent":"30","refreshes_at":"not-a-date"}}}"#;
        assert_eq!(
            membership_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn rejects_signed_exponent_and_escaped_percents() {
        for text in [
            r#"{"data":{"shared_pool":{"consumed_percent":"-1.5"}}}"#,
            r#"{"data":{"shared_pool":{"consumed_percent":1e2}}}"#,
            r#"{"data":{"shared_pool":{"consumed_percent":"1.2e3"}}}"#,
            r#"{"data":{"shared_pool":{"consumed_percent":{}}}}"#,
            r#"{"data":{"shared_pool":{"consumed_percent":null}}}"#,
            r#"{"data":{"shared_pool":{"consumed_percent":".5"}}}"#,
        ] {
            assert_eq!(
                membership_metrics(text, &request()),
                Err(ProviderError::InvalidResponse),
                "body must be rejected: {text}"
            );
        }
    }

    #[test]
    fn remaining_ratio_is_exact_across_scales() {
        // 30 consumed -> 0.7 remaining.
        let ratio = remaining_ratio("30".parse().unwrap()).unwrap();
        assert_eq!(ratio.to_string(), "0.7");
        // 33.5 consumed -> 0.665 remaining.
        let ratio = remaining_ratio("33.5".parse().unwrap()).unwrap();
        assert_eq!(ratio.to_string(), "0.665");
        // 99.995 consumed -> 0.005 percentage points remaining -> ratio 0.00005.
        let ratio = remaining_ratio("99.995".parse().unwrap()).unwrap();
        assert_eq!(ratio.to_string(), "0.00005");
        // Full consumption yields exactly zero; zero consumption yields 1.
        let ratio = remaining_ratio("100".parse().unwrap()).unwrap();
        assert_eq!(ratio.to_string(), "0");
        let ratio = remaining_ratio("0".parse().unwrap()).unwrap();
        assert_eq!(ratio.to_string(), "1");
        // 16-digit fractional precision is preserved exactly.
        let ratio = remaining_ratio("0.0000000000000001".parse().unwrap()).unwrap();
        assert_eq!(ratio.to_string(), "0.999999999999999999");
        // Above 100 is malformed, never clamped.
        assert!(remaining_ratio("100.000000000000000001".parse().unwrap()).is_err());
        assert!(remaining_ratio("101".parse().unwrap()).is_err());
    }
}
