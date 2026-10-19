use quotavista_contracts::*;
use quotavista_core::{
    calendar::{self, FakeClock},
    domain as d, number as n,
};
use serde_json::{json, Value};

fn id(s: &str) -> LocalId {
    s.to_owned().try_into().unwrap()
}
fn metric() -> d::Metric {
    let at = calendar::parse_instant("2024-06-03T12:00:00Z").unwrap();
    d::Metric {
        metric_id: "tokens".into(),
        scope_id: "synthetic-scope".into(),
        product_id: Some("demo-plan".into()),
        unit: d::Unit::Tokens,
        value: Some(d::Value::Tokens(n::TokenTotal(u128::MAX))),
        availability: d::Availability::Available,
        provenance: d::Provenance::Synthetic,
        observed_at: at,
        source_as_of: Some(at),
        freshness: d::Freshness::Stale,
        period: Some(d::Period {
            key: Some("synthetic-period".into()),
            interval: calendar::Interval::new(at, at + chrono_duration()).unwrap(),
            source_zone: "UTC".into(),
            expired: false,
        }),
        reason: None,
    }
}
// Build a duration without adding a second calendar dependency to this boundary.
fn chrono_duration() -> std::time::Duration {
    std::time::Duration::from_secs(3600)
}

#[test]
fn exact_json_round_trips_do_not_pass_through_js_numbers() {
    for count in [9_007_199_254_740_993, u64::MAX] {
        let dto = TokenCountDto::from(n::TokenCount(count));
        let text = serde_json::to_string(&dto).unwrap();
        assert_eq!(text, format!("\"{count}\""));
        let restored: TokenCountDto = serde_json::from_str(&text).unwrap();
        assert_eq!(n::TokenCount::from(restored), n::TokenCount(count));
    }
    let dto = TokenTotalDto::from(n::TokenTotal(u128::MAX));
    let restored: TokenTotalDto =
        serde_json::from_str(&serde_json::to_string(&dto).unwrap()).unwrap();
    assert_eq!(n::TokenTotal::from(restored), n::TokenTotal(u128::MAX));
    for invalid in [
        json!(9007199254740993_u64),
        json!("-1"),
        json!("01"),
        json!("18446744073709551616"),
    ] {
        assert!(serde_json::from_value::<TokenCountDto>(invalid).is_err());
    }
    assert!(serde_json::from_value::<TokenTotalDto>(json!(
        "340282366920938463463374607431768211456"
    ))
    .is_err());
    for invalid in [
        "1e2",
        "0.1234567890123456789",
        "12345678901234567890123456789",
        "1.00",
    ] {
        assert!(serde_json::from_value::<DecimalDto>(json!(invalid)).is_err());
    }
    assert_eq!(
        serde_json::to_value(DecimalDto::from(
            "0.123456789012345678".parse::<n::Decimal>().unwrap()
        ))
        .unwrap(),
        json!("0.123456789012345678")
    );
}

#[test]
fn mapping_preserves_null_zero_units_time_and_independent_failure() {
    let mut domain = metric();
    let mapped = MetricDto::from_domain(domain.clone(), true).unwrap();
    let value = serde_json::to_value(&mapped).unwrap();
    assert_eq!(value["value"], json!(u128::MAX.to_string()));
    assert_eq!(value["provenance"], "synthetic");
    assert_eq!(value["period"]["interval"]["start"], "2024-06-03T12:00:00Z");
    assert_eq!(value["period"]["source_zone"], "UTC");
    assert!(!mapped.alert_eligible);
    domain.value = Some(d::Value::Tokens(n::TokenTotal(0)));
    assert_eq!(
        MetricDto::from_domain(domain.clone(), false).unwrap().value,
        Some("0".into())
    );
    domain.value = None;
    domain.freshness = d::Freshness::NoValue;
    domain.availability = d::Availability::Unsupported;
    domain.reason = Some(d::Reason::Unsupported);
    let unavailable =
        serde_json::to_value(MetricDto::from_domain(domain.clone(), false).unwrap()).unwrap();
    assert_eq!(unavailable["value"], Value::Null);
    assert_eq!(unavailable["availability"], "unsupported");
    assert_eq!(unavailable["reason"], "unsupported");
    domain.availability = d::Availability::Missing;
    domain.reason = Some(d::Reason::SyncPending);
    assert_eq!(
        MetricDto::from_domain(domain, false).unwrap().availability,
        AvailabilityDto::Missing
    );
    let connection = ConnectionDto {
        connection_id: id("demo"),
        provider: Provider::Kimi,
        alias: "Demo".to_owned().try_into().unwrap(),
        lifecycle: LifecycleDto::Active,
        config_revision: "1".to_owned().try_into().unwrap(),
        generation: "3".to_owned().try_into().unwrap(),
        capabilities: vec![],
        datasets: vec![DatasetDto {
            dataset_id: id("history"),
            health: QueryHealthDto::LocalStorageError,
            last_success_at: Some(mapped.observed_at.clone()),
            error: Some(SafeError::new(ErrorCode::StorageError, None)),
            coverage: vec![CoverageDto {
                interval: mapped.period.clone().unwrap().interval,
                status: CoverageStatusDto::Partial,
                reason: Some(ReasonDto::Truncated),
            }],
        }],
    };
    let restored: ConnectionDto =
        serde_json::from_value(serde_json::to_value(&connection).unwrap()).unwrap();
    assert_eq!(restored, connection);
    assert_eq!(mapped.freshness, FreshnessDto::Stale);
    for (unit, amount) in [
        (
            d::Unit::Money("USD".parse().unwrap()),
            d::Value::Money(n::Money {
                amount: "12.34".parse().unwrap(),
                currency: "USD".parse().unwrap(),
            }),
        ),
        (d::Unit::Ratio, d::Value::Ratio("0.25".parse().unwrap())),
        (
            d::Unit::Native("requests".into()),
            d::Value::Native {
                amount: "4".parse().unwrap(),
                unit: "requests".into(),
            },
        ),
    ] {
        let mut m = metric();
        m.unit = unit;
        m.value = Some(amount);
        let dto = MetricDto::from_domain(m, false).unwrap();
        assert!(serde_json::from_value::<MetricDto>(serde_json::to_value(dto).unwrap()).is_ok());
    }
    let mut invalid = metric();
    invalid.unit = d::Unit::Ratio;
    assert!(MetricDto::from_domain(invalid, false).is_err());
}

fn args() -> Vec<(&'static str, Option<Value>)> {
    let range = json!({"start_date":"2024-06-01","end_date":"2024-06-03","reporting_zone":"America/New_York","grain":"week","scope_ids":null});
    vec![
        ("list_connections", None),
        (
            "connect_provider",
            Some(
                json!({"provider":"kimi","alias":"Demo","credential_kind":"session_token","secret":"synthetic-fixture-only","storage_mode":"session_only"}),
            ),
        ),
        (
            "update_connection",
            Some(json!({"connection_id":"demo","expected_config_revision":"1","alias":"Demo"})),
        ),
        (
            "replace_credentials",
            Some(
                json!({"connection_id":"demo","expected_config_revision":"1","credential_kind":"api_key","secret":"synthetic-fixture-only","storage_mode":"persistent"}),
            ),
        ),
        (
            "set_connection_enabled",
            Some(json!({"connection_id":"demo","expected_config_revision":"1","enabled":false})),
        ),
        (
            "remove_connection",
            Some(
                json!({"connection_id":"demo","expected_config_revision":"1","deletion_intent":"credentials_history_and_managed_backups"}),
            ),
        ),
        (
            "refresh_connections",
            Some(json!({"target":{"kind":"all"}})),
        ),
        ("get_overview", Some(json!({"range":range.clone()}))),
        ("query_statistics", Some(json!({"range":range}))),
        (
            "update_settings",
            Some(
                json!({"expected_settings_revision":"1","changes":{"ui_locale":"zh-CN","reporting_zone":"UTC","display_currency":"USD","polling":"disabled"}}),
            ),
        ),
        ("get_operation", Some(json!({"operation_id":"demo-job"}))),
    ]
}
fn request(name: &str, args: Option<Value>) -> Value {
    let mut command = json!({"command":name});
    if let Some(args) = args {
        command["args"] = args;
    }
    json!({"contract_version":CONTRACT_VERSION,"request_id":"synthetic-request","command":command})
}
#[test]
fn all_eleven_commands_decode_validate_and_have_result_contracts() {
    let clock = FakeClock(calendar::parse_instant("2024-06-03T12:00:00Z").unwrap());
    let generated = typescript();
    let result = generated
        .lines()
        .find(|line| line.starts_with("export type CommandResult ="))
        .unwrap();
    assert_eq!(args().len(), 11);
    for (name, arguments) in args() {
        let input: Request = serde_json::from_value(request(name, arguments)).unwrap();
        assert!(input.validate(&clock).is_ok(), "{name}");
        assert!(
            result.contains(&format!("\"{name}\"")),
            "missing result for {name}"
        );
    }
    assert!(generated.contains("\"zh-CN\""));
    assert!(generated.contains("export type TokenTotalDto = string;"));
    assert!(!generated.contains("bigint"));
}
#[test]
fn all_result_envelopes_round_trip_with_explicit_nulls_and_tags() {
    let op = json!({"operation_id":"demo-job","connection_id":"demo","kind":"setup","state":"pending","next_action":"none","retryable":false,"retry_at":null,"error":null});
    let connection = json!({"connection_id":"demo","provider":"kimi","alias":"Demo","lifecycle":"pending_setup","config_revision":"1","generation":"1","datasets":[],"capabilities":[]});
    let stats = json!({"requested_interval":{"start":"2024-06-03T00:00:00Z","end":"2024-06-04T00:00:00Z"},"effective_intervals":[],"reporting_zone":"UTC","grain":"day","as_of":"2024-06-03T00:00:00Z","provisional":true,"coverage":[],"excluded_scopes":[],"totals":[],"series":[],"plans":[],"data_revision":"1"});
    for (name, _) in args() {
        let data = match name {
            "list_connections" => json!({"connections":[connection.clone()],"data_revision":"1"}),
            "update_connection" | "set_connection_enabled" => connection.clone(),
            "refresh_connections" => json!({"operations":[op.clone()]}),
            "query_statistics" => stats.clone(),
            "get_overview" => json!({"metrics":[],"summary":stats.clone()}),
            "update_settings" => {
                json!({"settings_revision":"1","ui_locale":"zh-CN","reporting_zone":"UTC","display_currency":"USD","polling":"disabled"})
            }
            _ => op.clone(),
        };
        let wire = json!({"status":"ok","contract_version":CONTRACT_VERSION,"request_id":"synthetic-request","data":{"command":name,"data":data}});
        let response: Envelope = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(response).unwrap(), wire, "{name}");
    }
    let alias: Alias = serde_json::from_value(json!("  Demo  ")).unwrap();
    assert_eq!(alias.as_str(), "Demo");
    let empty_patch: SettingsPatch = serde_json::from_value(json!({})).unwrap();
    assert_eq!(
        serde_json::to_value(empty_patch).unwrap(),
        json!({"ui_locale":null,"reporting_zone":null,"display_currency":null,"polling":null})
    );
}

#[test]
fn request_validation_rejects_unknowns_and_bad_bounds_without_secret_echo() {
    let clock = FakeClock(calendar::parse_instant("2024-06-03T12:00:00Z").unwrap());
    let mut bad = request("list_connections", None);
    bad["unknown"] = json!(true);
    assert!(serde_json::from_value::<Request>(bad).is_err());
    assert!(serde_json::from_value::<Request>(request("arbitrary_sql", None)).is_err());
    let mut bad = request("list_connections", None);
    bad["contract_version"] = json!(2);
    assert_eq!(
        serde_json::from_value::<Request>(bad)
            .unwrap()
            .validate(&clock)
            .unwrap_err()
            .code,
        ErrorCode::Validation
    );
    for (name, args) in args().into_iter().filter(|(_, args)| args.is_some()) {
        let mut bad = request(name, args);
        bad["command"]["args"]["unknown"] = json!(true);
        assert!(serde_json::from_value::<Request>(bad).is_err(), "{name}");
    }
    for (field, value) in [
        ("provider", "unknown"),
        ("alias", ""),
        ("alias", &"a".repeat(81)),
        ("secret", "synthetic\ncredential"),
        ("secret", "  "),
        ("secret", &"x".repeat(65537)),
    ] {
        let mut bad = request("connect_provider", args()[1].1.clone());
        bad["command"]["args"][field] = json!(value);
        // serde's error for the secret conversion is static; it never echoes the input.
        assert!(serde_json::from_value::<Request>(bad).is_err());
    }
    let secret: SecretInput = serde_json::from_value(json!("  synthetic-fixture  ")).unwrap();
    assert_eq!(secret.expose_for_provider(), "  synthetic-fixture  ");
    let error = serde_json::from_value::<SecretInput>(json!("synthetic\ncredential"))
        .err()
        .unwrap()
        .to_string();
    assert!(!error.contains("synthetic"));
    for bad in [json!("../scope"), json!(""), json!("a b")] {
        assert!(serde_json::from_value::<LocalId>(bad).is_err());
    }
    assert!(serde_json::from_value::<Revision>(json!(1)).is_err());
    for (start, end) in [
        ("2024-06-03", "2024-06-02"),
        ("2024-06-04", "2024-06-04"),
        ("2023-01-01", "2024-06-03"),
    ] {
        let range = json!({"start_date":start,"end_date":end,"reporting_zone":"UTC","grain":"day","scope_ids":null});
        let bad: Request =
            serde_json::from_value(request("query_statistics", Some(json!({"range":range}))))
                .unwrap();
        assert_eq!(
            bad.validate(&clock).unwrap_err().code,
            ErrorCode::Validation
        );
    }
    let empty: Request = serde_json::from_value(request(
        "refresh_connections",
        Some(json!({"target":{"kind":"selected","connection_ids":[]}})),
    ))
    .unwrap();
    assert!(empty.validate(&clock).is_err());
}

#[test]
fn envelope_is_exclusive_safe_and_versioned() {
    let response = Envelope::Error {
        contract_version: CONTRACT_VERSION,
        request_id: id("synthetic-request"),
        error: SafeError::new(
            ErrorCode::RateLimited,
            Some("2024-06-03T12:01:00Z".to_owned().try_into().unwrap()),
        ),
    };
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["error"]["message"], "Wait before retrying.");
    assert_eq!(value["error"]["retryable"], true);
    assert!(value.get("data").is_none());
    assert_eq!(
        serde_json::from_value::<Envelope>(value.clone()).unwrap(),
        response
    );
    let mut mixed = value;
    mixed["data"] = json!({});
    assert!(serde_json::from_value::<Envelope>(mixed).is_err());
}

#[test]
fn generator_is_idempotent_and_check_detects_drift() {
    let dir = std::env::temp_dir().join(format!("quotavista-contract-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("contracts.ts");
    let run = |check| {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_generate-contracts"));
        if check {
            cmd.arg("--check");
        }
        cmd.arg(&path).output().unwrap()
    };
    assert!(run(false).status.success());
    let first = std::fs::read(&path).unwrap();
    assert!(run(false).status.success());
    assert_eq!(first, std::fs::read(&path).unwrap());
    assert!(run(true).status.success());
    std::fs::write(&path, "// deliberate drift in a throwaway fixture").unwrap();
    let drift = run(true);
    assert!(!drift.status.success());
    assert!(String::from_utf8_lossy(&drift.stderr).contains("contract drift"));
    std::fs::remove_file(&path).unwrap();
    assert!(
        !run(true).status.success(),
        "missing generated output must fail"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
