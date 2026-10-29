//! OpenRouter Credits endpoint parsing and mapping.
//!
//! Wire shapes in the bundled fixtures are sanitized, invented, and
//! UNVALIDATED: no live Management Key was available while writing them, so
//! field spellings and casing still need confirmation during live
//! reconciliation. Parsed strictly anyway: a shape mismatch must surface as
//! `InvalidResponse`, never as a guessed balance.
//!
//! Amount exactness: amounts arrive either as JSON strings or as bare JSON
//! numbers. Both are captured as raw JSON text (`serde_json::raw::RawValue`)
//! and parsed with the core `Decimal` parser, so no floating-point value is
//! ever constructed on the accounting path.

use quotavista_core::domain::{Availability, Provenance, QueryKind, Reason, Value};
use quotavista_core::number::{Currency, Decimal, Money};
use quotavista_core::provider::{CurrentMetric, ProviderError, ProviderRequest};
use serde::Deserialize;
use serde_json::value::RawValue;

/// Account-level Credits endpoint of the OpenRouter API. Requires a
/// Management Key at runtime; queried read-only.
pub(crate) const CREDITS_PATH: &str = "/api/v1/credits";

/// Dataset identifier shared with the synthetic demo (`prepaid-credits`):
/// account-level prepaid balance, explicitly distinct from any per-key
/// budget.
pub(crate) const CREDITS_DATASET: &str = "prepaid-credits";

/// OpenRouter Credits are denominated in US dollars. TODO live validation:
/// confirm the account currency is always USD during reconciliation.
const CREDITS_CURRENCY: &str = "USD";

/// Maximum scale the core `Decimal` accepts; mirrors core limits so
/// precision rejections stay consistent after re-parsing.
const MAX_FRACTION_DIGITS: usize = 18;

#[derive(Deserialize)]
struct WireCredits<'a> {
    #[serde(borrow)]
    data: WireCreditsData<'a>,
}

#[derive(Deserialize)]
struct WireCreditsData<'a> {
    #[serde(borrow, default)]
    total_credits: Option<&'a RawValue>,
    #[serde(borrow, default)]
    total_usage: Option<&'a RawValue>,
}

/// Parse one raw amount literal into a `Decimal`. Accepts `"123.45"`,
/// `123.45`, and `123`; rejects anything else (signs, exponents, escaped
/// digits, objects) as `InvalidResponse`.
fn amount_decimal(raw: &RawValue) -> Result<Decimal, ProviderError> {
    let text = raw.get();
    let literal = if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        &text[1..text.len() - 1]
    } else {
        text
    };
    // A quoted amount containing backslashes would need JSON unescaping;
    // amounts are plain ASCII digit literals, so treat escapes as invalid
    // rather than guessing at a decode.
    if literal.bytes().any(|b| !b.is_ascii_digit() && b != b'.')
        || literal.split('.').count() > 2
        || literal.ends_with('.')
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

/// Render an integer coefficient at `scale` back into decimal notation.
fn render(coefficient: u128, scale: u32) -> String {
    if scale == 0 {
        return coefficient.to_string();
    }
    let digits = format!("{coefficient:0width$}", width = scale as usize + 1);
    let split = digits.len() - scale as usize;
    format!("{}.{}", &digits[..split], &digits[split..])
}

/// Exact `credits - usage`. Returns `Ok(None)` when the remainder is
/// negative: core `Decimal` is nonnegative, and a negative remainder must
/// surface as an explicitly unrepresentable metric, never as a clamped
/// zero. The final `Decimal` parse re-validates the 28-significant-digit
/// limit.
fn remaining(credits: Decimal, usage: Decimal) -> Result<Option<Decimal>, ProviderError> {
    let (credits_coefficient, credits_scale) = coefficient_scale(&credits);
    let (usage_coefficient, usage_scale) = coefficient_scale(&usage);
    let scale = credits_scale.max(usage_scale);
    let align = |coefficient: u128, own: u32| -> Result<u128, ProviderError> {
        coefficient
            .checked_mul(10u128.pow(scale - own))
            .ok_or(ProviderError::InvalidResponse)
    };
    let credits_aligned = align(credits_coefficient, credits_scale)?;
    let usage_aligned = align(usage_coefficient, usage_scale)?;
    if credits_aligned < usage_aligned {
        return Ok(None);
    }
    let difference = credits_aligned - usage_aligned;
    render(difference, scale)
        .parse()
        .map(Some)
        .map_err(|_| ProviderError::InvalidResponse)
}

/// Currency of Credits amounts. Panics never: the constant is a valid
/// three-letter code.
fn credits_currency() -> Currency {
    CREDITS_CURRENCY
        .parse()
        .expect("constant currency code is valid")
}

fn money_metric(metric: &str, value: Value, request: &ProviderRequest) -> CurrentMetric {
    CurrentMetric {
        dataset: CREDITS_DATASET.to_owned(),
        metric: metric.to_owned(),
        product_key: None,
        value: Some(value),
        availability: Availability::Available,
        reason: None,
        query_kind: QueryKind::Unvalidated,
        provenance: Provenance::Observation,
        observed_at: request.as_of,
        source_as_of: None,
    }
}

/// Parse a Credits response body into current metrics.
///
/// Emits `total_credits`, `total_usage`, and the derived `remaining`.
/// `remaining` is a plain arithmetic remainder of two directly observed
/// values, classified `Unvalidated` until live reconciliation validates the
/// wire shape. When usage exceeds credits the remainder is negative, which
/// values, classified `Unvalidated` until live reconciliation validates the
/// wire shape. When usage exceeds credits the remainder is negative, which
/// the nonnegative money domain cannot carry; the metric is then reported
/// `Missing`/`IncompatibleUnit` with no value rather than clamped to zero
/// (a unit-system limit, not a missing upstream capability). The Credits
/// envelope carries no server-side timestamp, so `observed_at` records the
/// local observation instant while `source_as_of` stays `None` until
/// reconciliation confirms a server-side field to carry
/// (TODO live validation).
pub(crate) fn credits_metrics(
    body: &str,
    request: &ProviderRequest,
) -> Result<Vec<CurrentMetric>, ProviderError> {
    let wire: WireCredits =
        serde_json::from_str(body).map_err(|_| ProviderError::InvalidResponse)?;
    let total_credits = wire
        .data
        .total_credits
        .ok_or(ProviderError::InvalidResponse)
        .and_then(amount_decimal)?;
    let total_usage = wire
        .data
        .total_usage
        .ok_or(ProviderError::InvalidResponse)
        .and_then(amount_decimal)?;
    let currency = credits_currency();
    let mut metrics = vec![
        money_metric(
            "total_credits",
            Value::Money(Money {
                amount: total_credits,
                currency: currency.clone(),
            }),
            request,
        ),
        money_metric(
            "total_usage",
            Value::Money(Money {
                amount: total_usage,
                currency,
            }),
            request,
        ),
    ];
    let remaining_metric = match remaining(total_credits, total_usage)? {
        Some(amount) => money_metric(
            "remaining",
            Value::Money(Money {
                amount,
                currency: credits_currency(),
            }),
            request,
        ),
        None => CurrentMetric {
            dataset: CREDITS_DATASET.to_owned(),
            metric: "remaining".to_owned(),
            product_key: None,
            value: None,
            availability: Availability::Missing,
            reason: Some(Reason::IncompatibleUnit),
            query_kind: QueryKind::Unvalidated,
            provenance: Provenance::Observation,
            observed_at: request.as_of,
            source_as_of: None,
        },
    };
    metrics.push(remaining_metric);
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ProviderRequest {
        ProviderRequest {
            connection_id: "openrouter-live-1".to_owned(),
            as_of: "2026-10-29T12:00:00Z".parse().unwrap(),
        }
    }

    fn money_value(metric: &CurrentMetric) -> String {
        match metric.value.as_ref().unwrap() {
            Value::Money(money) => money.amount.to_string(),
            other => panic!("expected money value, got {other:?}"),
        }
    }

    #[test]
    fn parses_string_amounts_from_fixture() {
        let body = include_str!("fixtures/credits_ok.json");
        let metrics = credits_metrics(body, &request()).unwrap();
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].metric, "total_credits");
        assert_eq!(money_value(&metrics[0]), "120.5");
        assert_eq!(money_value(&metrics[1]), "37.25");
        assert_eq!(money_value(&metrics[2]), "83.25");
        for metric in &metrics {
            metric.validate().unwrap();
            assert_eq!(metric.dataset, CREDITS_DATASET);
            assert_eq!(metric.provenance, Provenance::Observation);
            assert_eq!(metric.query_kind, QueryKind::Unvalidated);
            assert_eq!(metric.source_as_of, None);
        }
    }

    #[test]
    fn parses_bare_number_amounts_exactly() {
        let body = include_str!("fixtures/credits_number_amounts.json");
        let metrics = credits_metrics(body, &request()).unwrap();
        // 120.5 and 37.25 arrive as JSON numbers and must stay exact.
        // 120.055 and 37.257 arrive as bare JSON numbers and are NOT
        // representable in f64 exactly; only the raw-text path yields these
        // exact strings (a f64 detour would print 120.05500000000001-style
        // artifacts).
        assert_eq!(money_value(&metrics[0]), "120.055");
        assert_eq!(money_value(&metrics[2]), "82.798");
    }

    #[test]
    fn negative_remaining_is_reported_missing_not_clamped() {
        let body = include_str!("fixtures/credits_overdrawn.json");
        let metrics = credits_metrics(body, &request()).unwrap();
        let remainder = &metrics[2];
        assert_eq!(remainder.metric, "remaining");
        assert_eq!(remainder.availability, Availability::Missing);
        assert_eq!(remainder.reason, Some(Reason::IncompatibleUnit));
        assert!(remainder.value.is_none());
        remainder.validate().unwrap();
        // The directly observed totals stay available.
        assert_eq!(metrics[0].availability, Availability::Available);
        assert_eq!(metrics[1].availability, Availability::Available);
    }

    #[test]
    fn missing_amount_field_is_invalid() {
        let body = include_str!("fixtures/credits_missing_field.json");
        assert_eq!(
            credits_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn precision_violation_is_invalid() {
        let body = include_str!("fixtures/credits_precision_violation.json");
        assert_eq!(
            credits_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn malformed_body_is_invalid() {
        let body = include_str!("fixtures/credits_malformed.txt");
        assert_eq!(
            credits_metrics(body, &request()),
            Err(ProviderError::InvalidResponse)
        );
    }

    #[test]
    fn reject_signed_exponent_and_escaped_amounts() {
        for text in [
            r#"{"data":{"total_credits":"-1.5","total_usage":"0"}}"#,
            r#"{"data":{"total_credits":1e2,"total_usage":"0"}}"#,
            r#"{"data":{"total_credits":"1.2e3","total_usage":"0"}}"#,
            r#"{"data":{"total_credits":{},"total_usage":"0"}}"#,
            r#"{"data":{"total_credits":"1","total_usage":null}}"#,
        ] {
            assert_eq!(
                credits_metrics(text, &request()),
                Err(ProviderError::InvalidResponse),
                "body must be rejected: {text}"
            );
        }
    }

    #[test]
    fn subtraction_handles_scale_mismatch_exactly() {
        // 1.000000000000000001 - 0.999999999999999999 = 0.000000000000000002
        let credits: Decimal = "1.000000000000000001".parse().unwrap();
        let usage: Decimal = "0.999999999999999999".parse().unwrap();
        let difference = remaining(credits, usage).unwrap().unwrap();
        assert_eq!(difference.to_string(), "0.000000000000000002");
        // Mixed scales align exactly: 120.5 - 0.25 = 120.25
        let credits: Decimal = "120.5".parse().unwrap();
        let usage: Decimal = "0.25".parse().unwrap();
        assert_eq!(
            remaining(credits, usage).unwrap().unwrap().to_string(),
            "120.25"
        );
        // Overdraft reports None instead of fabricating a value.
        let credits: Decimal = "10.00".parse().unwrap();
        let usage: Decimal = "12.50".parse().unwrap();
        assert!(remaining(credits, usage).unwrap().is_none());
    }
}
