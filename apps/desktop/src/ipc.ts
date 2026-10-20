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
import { listen, type Event } from "@tauri-apps/api/event";
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
export function envelopeToError(
  requestId: LocalId,
  envelope: Extract<Envelope, { status: "error" }>,
): IpcError {
  void requestId;
  return makeIpcError(
    envelope.error.message,
    envelope.error.code,
    envelope.error.retryable,
    envelope.error.retry_at,
  );
}

/**
 * Compare two revisions as decimals. Returns -1 / 0 / 1 so callers can sort
 * or branch on a strict ordering.
 */
export function compareRevisions(a: string, b: string): -1 | 0 | 1 {
  const ai = BigInt(a);
  const bi = BigInt(b);
  if (ai < bi) return -1;
  if (ai > bi) return 1;
  return 0;
}

/**
 * Returns true when `responseRevision` is at least as new as `observedAt`,
 * i.e. the response was generated from data no older than what the renderer
 * last saw. A response older than the latest observation is "obsolete".
 */
export function isObsoleteResponse(responseRevision: string, observedAt: string): boolean {
  return compareRevisions(responseRevision, observedAt) < 0;
}

/**
 * Returns true when `seen` and `incoming` form a gap: the new revision is
 * not exactly one greater than the previous one. The renderer treats gaps
 * as "must reload" because we cannot trust a partial update.
 */
export function isRevisionGap(seen: string, incoming: string): boolean {
  // We accept the very first event (seen === "0") regardless of value, since
  // an empty storage always returns revision 0.
  if (seen === "0") return false;
  return BigInt(incoming) - BigInt(seen) !== 1n;
}

/**
 * Revision-only state shared by every screen. The renderer stores the latest
 * revision it has observed; every response whose `data_revision` is older
 * than this value is treated as obsolete and discarded.
 */
export class RevisionTracker {
  private latest: string;
  private readonly subscribers = new Set<(event: ChangeEvent) => void>();

  constructor(initial: string = "0") {
    this.latest = initial;
  }

  /** Current best-known revision. The renderer treats responses older than this as obsolete. */
  observed(): string {
    return this.latest;
  }

  /**
   * Apply a `qv://change` event. Returns one of:
   * - `{"kind":"apply"}`: revision is the next expected value; subscribers fired.
   * - `{"kind":"gap"}`: revision skipped one or more steps; subscribers fired with `gap=true`
   *   so screens can trigger a full reload.
   */
  apply(event: ChangeEvent): { kind: "apply" | "gap"; gap: boolean } {
    const gap = isRevisionGap(this.latest, event.data_revision);
    this.latest = event.data_revision;
    for (const subscriber of this.subscribers) {
      subscriber(event);
    }
    return { kind: gap ? "gap" : "apply", gap };
  }

  subscribe(handler: (event: ChangeEvent) => void): () => void {
    this.subscribers.add(handler);
    return () => {
      this.subscribers.delete(handler);
    };
  }

  /** Test/debug access: replace the current revision. */
  reset(value: string): void {
    this.latest = value;
  }
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
  refreshConnections(
    target: { all: true } | { selected: ReadonlyArray<string> },
  ): Promise<OperationList> {
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
 * Wire the `qv://change` channel to a revision tracker. Returns a disposer
 * that unbinds both the listener and every subscriber registered through the
 * tracker. The renderer should treat the returned tracker as the single
 * source of truth for "what revision am I looking at".
 */
export async function subscribeToChanges(
  tracker: RevisionTracker,
  handler: (event: ChangeEvent) => void,
): Promise<() => Promise<void>> {
  const unsubTracker = tracker.subscribe(handler);
  const unlisten = await listen<ChangeEvent>(
    "qv://change",
    (event: Event<ChangeEvent>) => {
      tracker.apply(event.payload);
    },
  );
  return async () => {
    unsubTracker();
    unlisten();
  };
}
