/**
 * Single typed entry point for the renderer to talk to the backend.
 *
 * The renderer MUST go through this module. The capabilities allowlist only
 * permits one command (`dispatch_command`), and every command-level operation
 * lives behind it. The renderer never sees a SecretInput value as a string —
 * the only APIs that accept credentials are `connectProvider` and
 * `replaceCredentials`, and they return the operation result immediately,
 * never the secret.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ChangeEvent,
  Command,
  CommandResult,
  ConnectionDto,
  ConnectionList,
  Envelope,
  LocalId,
  OperationDto,
  OperationList,
  OverviewDto,
  Request,
  SettingsDto,
  SettingsPatch,
  StatisticsDto,
} from "./generated/contracts";

export const CONTRACT_VERSION = 1 as const;

export type { ChangeEvent, ConnectionDto, ConnectionList, OperationDto, OperationList, OverviewDto, SettingsDto, SettingsPatch, StatisticsDto };

/** SafeError shape lifted off the envelope. `code` is the only stable branch. */
export interface IpcError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly retryAt: string | null;
}

function makeIpcError(message: string, code: string, retryable: boolean, retryAt: string | null): IpcError {
  const error = new Error(message) as Error & {
    code?: string;
    retryable?: boolean;
    retryAt?: string | null;
  };
  error.name = "QuotaVistaIpcError";
  error.code = code;
  error.retryable = retryable;
  error.retryAt = retryAt;
  return error as IpcError;
}

/** Map a non-`ok` envelope into a JS `Error` that callers can branch on. */
export function envelopeToError(requestId: LocalId, envelope: Extract<Envelope, { status: "error" }>): IpcError {
  void requestId;
  return makeIpcError(
    envelope.error.message,
    envelope.error.code,
    envelope.error.retryable,
    envelope.error.retry_at,
  );
}

/**
 * Build a per-request id from a monotonic counter. The id is opaque to the
 * backend but unique within a renderer session, which is enough to associate
 * logs from the composition root.
 */
let requestCounter = 0;
function nextRequestId(): LocalId {
  requestCounter += 1;
  return `req-${requestCounter.toString(36)}`;
}

interface DispatchOptions {
  readonly requestId?: LocalId;
}

type ResultOf<C extends Command["command"]> = Extract<CommandResult, { command: C }>["data"];

async function dispatch<C extends Command["command"]>(
  command: Extract<Command, { command: C }>,
  options: DispatchOptions = {},
): Promise<ResultOf<C>> {
  const request: Request = {
    contract_version: CONTRACT_VERSION,
    request_id: options.requestId ?? nextRequestId(),
    command,
  };
  const envelope = await invoke<Envelope>("dispatch_command", { request });
  if (envelope.status === "ok") {
    if (envelope.data.command !== command.command) {
      throw new Error(
        `IPC envelope command mismatch: requested ${command.command}, received ${envelope.data.command}`,
      );
    }
    return envelope.data.data as ResultOf<C>;
  }
  throw envelopeToError(envelope.request_id, envelope);
}

export const ipc = {
  listConnections(): Promise<ConnectionList> {
    return dispatch({ command: "list_connections" });
  },
  connectProvider(args: {
    provider: "kimi" | "openrouter";
    alias: string;
    credential_kind: "api_key" | "management_key" | "session_token";
    secret: string;
    storage_mode: "persistent" | "session_only";
  }): Promise<OperationDto> {
    return dispatch({ command: "connect_provider", args });
  },
  updateConnection(args: {
    connection_id: string;
    expected_config_revision: string;
    alias: string;
  }): Promise<ConnectionDto> {
    return dispatch({ command: "update_connection", args });
  },
  replaceCredentials(args: {
    connection_id: string;
    expected_config_revision: string;
    credential_kind: "api_key" | "management_key" | "session_token";
    secret: string;
    storage_mode: "persistent" | "session_only";
  }): Promise<OperationDto> {
    return dispatch({ command: "replace_credentials", args });
  },
  setConnectionEnabled(args: {
    connection_id: string;
    expected_config_revision: string;
    enabled: boolean;
  }): Promise<ConnectionDto> {
    return dispatch({ command: "set_connection_enabled", args });
  },
  removeConnection(args: {
    connection_id: string;
    expected_config_revision: string;
    deletion_intent: "credentials_history_and_managed_backups";
  }): Promise<OperationDto> {
    return dispatch({ command: "remove_connection", args });
  },
  refreshConnections(target: { all: true } | { selected: ReadonlyArray<string> }): Promise<OperationList> {
    if ("all" in target) {
      return dispatch({ command: "refresh_connections", args: { target: { kind: "all" } } });
    }
    return dispatch({
      command: "refresh_connections",
      args: { target: { kind: "selected", connection_ids: [...target.selected] } },
    });
  },
  getOverview(range: {
    start_date: string;
    end_date: string;
    reporting_zone: string;
    scope_ids?: ReadonlyArray<string> | null;
  }): Promise<OverviewDto> {
    return dispatch({
      command: "get_overview",
      args: { range: { ...range, scope_ids: range.scope_ids ? [...range.scope_ids] : null } },
    });
  },
  queryStatistics(range: {
    start_date: string;
    end_date: string;
    reporting_zone: string;
    grain: "day" | "week";
    scope_ids?: ReadonlyArray<string> | null;
  }): Promise<StatisticsDto> {
    return dispatch({
      command: "query_statistics",
      args: { range: { ...range, scope_ids: range.scope_ids ? [...range.scope_ids] : null } },
    });
  },
  updateSettings(args: {
    expected_settings_revision: string;
    changes: SettingsPatch;
  }): Promise<SettingsDto> {
    return dispatch({ command: "update_settings", args });
  },
  getOperation(operation_id: string): Promise<OperationDto> {
    return dispatch({ command: "get_operation", args: { operation_id } });
  },
} as const;

/**
 * Subscribe to revision-change events. Each call wires a fresh listener; the
 * returned `dispose` removes it. Events are invalidation hints: the receiver
 * decides what to refetch.
 */
export async function subscribeToChanges(
  handler: (event: ChangeEvent) => void,
): Promise<UnlistenFn> {
  return await listen<ChangeEvent>("qv://change", (event: Event<ChangeEvent>) => handler(event.payload));
}
