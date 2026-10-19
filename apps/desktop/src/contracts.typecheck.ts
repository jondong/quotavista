import type { Command, Request, SettingsPatch, TokenTotalDto } from "./generated/contracts.js";

// Compile-only fixtures; not imported by the app. Rust checks the same wire shapes.
const range = {
  start_date: "2024-06-01", end_date: "2024-06-03", reporting_zone: "UTC",
  grain: "week", scope_ids: null,
} as const;
export const commands = [
  { command: "list_connections" },
  { command: "connect_provider", args: { provider: "kimi", alias: "Demo", credential_kind: "session_token", secret: "synthetic-fixture-only", storage_mode: "session_only" } },
  { command: "update_connection", args: { connection_id: "demo", expected_config_revision: "1", alias: "Demo" } },
  { command: "replace_credentials", args: { connection_id: "demo", expected_config_revision: "1", credential_kind: "api_key", secret: "synthetic-fixture-only", storage_mode: "persistent" } },
  { command: "set_connection_enabled", args: { connection_id: "demo", expected_config_revision: "1", enabled: false } },
  { command: "remove_connection", args: { connection_id: "demo", expected_config_revision: "1", deletion_intent: "credentials_history_and_managed_backups" } },
  { command: "refresh_connections", args: { target: { kind: "all" } } },
  { command: "get_overview", args: { range } },
  { command: "query_statistics", args: { range } },
  { command: "update_settings", args: { expected_settings_revision: "1", changes: { ui_locale: "zh-CN", reporting_zone: "UTC", display_currency: "USD", polling: "disabled" } } },
  { command: "get_operation", args: { operation_id: "demo-job" } },
] satisfies Command[];
export const request = { contract_version: 1, request_id: "synthetic-request", command: commands[0] } satisfies Request;
export const emptyPatch = { ui_locale: null, reporting_zone: null, display_currency: null, polling: null } satisfies SettingsPatch;
export const largestTotal = "340282366920938463463374607431768211455" satisfies TokenTotalDto;

// @ts-expect-error Exact counts must not become JavaScript numbers.
export const invalidTotal: TokenTotalDto = 1;
// @ts-expect-error Providers are a closed union.
export const invalidCommand: Command = { command: "connect_provider", args: { provider: "unknown" } };
// @ts-expect-error Generated types describe canonical senders: null, not omission.
export const omittedPatch: SettingsPatch = {};
