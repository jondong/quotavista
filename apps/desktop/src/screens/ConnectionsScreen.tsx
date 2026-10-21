import { useEffect, useRef, useState } from "react";
import {
  ipc,
  isIpcError,
  isObsoleteResponse,
  revisionTracker,
  subscribeToChanges,
} from "../ipc";
import { useI18n, type MessageKey } from "../i18n";
import type {
  ConnectionList,
  CredentialKind,
  ErrorCode,
  LifecycleDto,
  OperationDto,
  Provider,
  StorageMode,
} from "../generated/contracts";

// Official provider names are not translated; aliases keep user content.
const PROVIDER_LABELS: Record<Provider, MessageKey> = {
  kimi: "connections.provider.kimi",
  openrouter: "connections.provider.openrouter",
};

const LIFECYCLE_LABELS: Record<LifecycleDto, MessageKey> = {
  pending_setup: "connections.state.pending_setup",
  active: "connections.state.active",
  disabled: "connections.state.disabled",
  needs_credentials: "connections.state.needs_credentials",
  deletion_pending: "connections.state.deletion_pending",
};

const OPERATION_STATE_LABELS: Record<OperationDto["state"], MessageKey> = {
  pending: "connections.operation.state.pending",
  running: "connections.operation.state.running",
  succeeded: "connections.operation.state.succeeded",
  failed: "connections.operation.state.failed",
  cancelled: "connections.operation.state.cancelled",
};

const OPERATION_KIND_LABELS: Record<OperationDto["kind"], MessageKey> = {
  setup: "connections.operation.kind.setup",
  replace_credentials: "connections.operation.kind.replaceCredentials",
  delete: "connections.operation.kind.delete",
  refresh: "connections.operation.kind.refresh",
};

const NEXT_ACTION_HINTS: Partial<Record<OperationDto["next_action"], MessageKey>> = {
  retry: "connections.operation.nextAction.retry",
  unlock_secret_store: "connections.operation.nextAction.unlockSecretStore",
  provide_credentials: "connections.operation.nextAction.provideCredentials",
  replace_connection: "connections.operation.nextAction.replaceConnection",
};

const ERROR_MESSAGES: Partial<Record<ErrorCode, MessageKey>> = {
  validation: "connections.error.validation",
  conflict: "connections.error.conflict",
  needs_credentials: "connections.error.needsCredentials",
  secret_store_locked: "connections.error.secretStoreLocked",
  secret_store_unavailable: "connections.error.secretStoreUnavailable",
  storage_error: "connections.error.storageError",
  unsupported_in_phase_one: "connections.error.unsupported",
};

function errorMessage(error: unknown, t: (key: MessageKey) => string): string {
  if (isIpcError(error)) {
    const key = ERROR_MESSAGES[error.code as ErrorCode];
    return key ? t(key) : t("connections.error.fallback");
  }
  return t("connections.error.fallback");
}

interface CredentialDraft {
  readonly credential_kind: CredentialKind;
  readonly secret: string;
  readonly storage_mode: StorageMode;
}

/**
 * Credential inputs for add and rotate flows. The secret lives in component
 * state only: it is cleared after every submission — success or failure —
 * and when the interface language changes, and it never reaches browser
 * storage. Storage-mode choice is explicit, including session-only.
 */
function CredentialFields({
  idPrefix,
  draft,
  onChange,
}: {
  idPrefix: string;
  draft: CredentialDraft;
  onChange: (next: CredentialDraft) => void;
}) {
  const { t } = useI18n();
  // A language switch must never carry secret input forward.
  const previousLocale = useRef(useI18n().locale);
  const locale = useI18n().locale;
  useEffect(() => {
    if (previousLocale.current !== locale) {
      previousLocale.current = locale;
      onChange({ ...draft, secret: "" });
    }
    // draft is intentionally omitted: the reset writes the current draft.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [locale]);

  return (
    <>
      <div>
        <label htmlFor={`${idPrefix}-credential-kind`}>
          {t("connections.add.credentialKind")}
        </label>
        <select
          id={`${idPrefix}-credential-kind`}
          value={draft.credential_kind}
          onChange={(event) =>
            onChange({ ...draft, credential_kind: event.target.value as CredentialKind })
          }
        >
          <option value="api_key">{t("connections.add.credentialKind.apiKey")}</option>
          <option value="management_key">
            {t("connections.add.credentialKind.managementKey")}
          </option>
          <option value="session_token">
            {t("connections.add.credentialKind.sessionToken")}
          </option>
        </select>
      </div>
      <div>
        <label htmlFor={`${idPrefix}-secret`}>{t("connections.add.secret")}</label>
        <input
          id={`${idPrefix}-secret`}
          type="password"
          autoComplete="off"
          value={draft.secret}
          onChange={(event) => onChange({ ...draft, secret: event.target.value })}
        />
        <p className="note">{t("connections.add.secret.hint")}</p>
      </div>
      <div>
        <label htmlFor={`${idPrefix}-storage-mode`}>{t("connections.add.storageMode")}</label>
        <select
          id={`${idPrefix}-storage-mode`}
          value={draft.storage_mode}
          onChange={(event) =>
            onChange({ ...draft, storage_mode: event.target.value as StorageMode })
          }
        >
          <option value="persistent">{t("connections.add.storageMode.persistent")}</option>
          <option value="session_only">{t("connections.add.storageMode.sessionOnly")}</option>
        </select>
        <p className="note">{t("connections.add.storageMode.hint")}</p>
      </div>
    </>
  );
}

function OperationStatus({ operation }: { operation: OperationDto }) {
  const { t } = useI18n();
  const hint = NEXT_ACTION_HINTS[operation.next_action];
  return (
    <div>
      <h4>{t("connections.operation.heading")}</h4>
      <p>
        {t(OPERATION_KIND_LABELS[operation.kind])} —{" "}
        {t(OPERATION_STATE_LABELS[operation.state])}
      </p>
      {hint ? <p className="note">{t(hint)}</p> : null}
      {operation.state === "failed" && operation.next_action === "replace_connection" ? (
        <p className="note">{t("connections.replaceConnection.hint")}</p>
      ) : null}
    </div>
  );
}

const EMPTY_DRAFT: CredentialDraft = {
  credential_kind: "api_key",
  secret: "",
  storage_mode: "persistent",
};

export default function ConnectionsScreen() {
  const { t } = useI18n();
  const [connections, setConnections] = useState<ConnectionList | null>(null);
  const [failed, setFailed] = useState(false);
  // Generation of the newest read; older responses are obsolete.
  const generation = useRef(0);

  // Add-connection form state. The alias draft is non-secret and survives
  // language switches; the secret draft is cleared by CredentialFields.
  const [addProvider, setAddProvider] = useState<Provider>("kimi");
  const [addAlias, setAddAlias] = useState("");
  const [addDraft, setAddDraft] = useState<CredentialDraft>(EMPTY_DRAFT);
  const [addBusy, setAddBusy] = useState(false);

  // Rotate panel (one open at a time) and its draft.
  const [rotateFor, setRotateFor] = useState<string | null>(null);
  const [rotateDraft, setRotateDraft] = useState<CredentialDraft>(EMPTY_DRAFT);
  const [rotateBusy, setRotateBusy] = useState(false);

  // Inline rename editor.
  const [renameFor, setRenameFor] = useState<string | null>(null);
  const [renameAlias, setRenameAlias] = useState("");

  // Outcome surfaces: the last operation status and the last typed error.
  const [lastOperation, setLastOperation] = useState<OperationDto | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => Promise<void>) | undefined;

    const load = async (): Promise<void> => {
      const request = generation.current + 1;
      generation.current = request;
      try {
        const list = await ipc.listConnections();
        if (disposed) return;
        if (request !== generation.current) return;
        if (isObsoleteResponse(list.data_revision, revisionTracker.observed())) return;
        revisionTracker.observe(list.data_revision);
        setConnections(list);
        setFailed(false);
      } catch {
        if (disposed || request !== generation.current) return;
        setFailed(true);
      }
    };

    // The listener is installed before the first read so no commit lands
    // between them unnoticed.
    const start = async (): Promise<void> => {
      try {
        const dispose = await subscribeToChanges(revisionTracker, (_event, update) => {
          if (update.kind === "apply" || update.kind === "gap") void load();
        });
        if (disposed) {
          void dispose();
          return;
        }
        unsubscribe = dispose;
      } catch {
        // A live channel that cannot be installed keeps the last known view.
      }
      await load();
    };
    void start();

    return () => {
      disposed = true;
      void unsubscribe?.();
    };
  }, []);

  const runAction = async (action: () => Promise<unknown>): Promise<void> => {
    try {
      const result = await action();
      if (result && typeof result === "object" && "operation_id" in result) {
        setLastOperation(result as OperationDto);
      } else {
        setLastOperation(null);
      }
      setLastError(null);
      // The change event reloads the list; reload directly too so a missing
      // listener (tests) still shows the effect of the mutation.
      const request = generation.current + 1;
      generation.current = request;
      try {
        const list = await ipc.listConnections();
        if (request === generation.current) {
          if (!isObsoleteResponse(list.data_revision, revisionTracker.observed())) {
            revisionTracker.observe(list.data_revision);
          }
          setConnections(list);
          setFailed(false);
        }
      } catch {
        // keep the previous view; the event channel will refresh it
      }
    } catch (error) {
      setLastError(errorMessage(error, t));
    }
  };

  const submitAdd = async (): Promise<void> => {
    const alias = addAlias.trim();
    if (!alias || addBusy) return;
    setAddBusy(true);
    // The submitted value leaves the field at submission, before any await:
    // clearing is not tied to when (or whether) the response settles, and
    // input the user types afterwards is their next attempt, untouched here.
    const { secret } = addDraft;
    setAddDraft({ ...addDraft, secret: "" });
    try {
      await runAction(() =>
        ipc.connectProvider({
          provider: addProvider,
          alias,
          credential_kind: addDraft.credential_kind,
          secret,
          storage_mode: addDraft.storage_mode,
        }),
      );
    } finally {
      setAddBusy(false);
    }
  };

  const submitRotate = async (connectionId: string, revision: string): Promise<void> => {
    if (rotateBusy) return;
    setRotateBusy(true);
    // Same discipline as the add form: the secret leaves the field at
    // submission, before the request is even sent.
    const { secret } = rotateDraft;
    setRotateDraft({ ...rotateDraft, secret: "" });
    try {
      await runAction(() =>
        ipc.replaceCredentials({
          connection_id: connectionId,
          expected_config_revision: revision,
          credential_kind: rotateDraft.credential_kind,
          secret,
          storage_mode: rotateDraft.storage_mode,
        }),
      );
    } finally {
      setRotateBusy(false);
    }
  };

  if (failed) {
    return <p role="alert" className="error">{t("common.error")}</p>;
  }
  if (!connections) {
    return <p role="status">{t("common.loading")}</p>;
  }

  return (
    <section aria-labelledby="connections-heading">
      <h2 id="connections-heading">{t("connections.heading")}</h2>

      <h3>{t("connections.list.heading")}</h3>
      {connections.connections.length === 0 ? (
        <p>{t("connections.list.empty")}</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th scope="col">{t("connections.alias.label")}</th>
              <th scope="col">{t("connections.provider")}</th>
              <th scope="col">{t("connections.state")}</th>
              <th scope="col">{t("connections.revision")}</th>
              <th scope="col">{t("connections.actions.heading")}</th>
            </tr>
          </thead>
          <tbody>
            {connections.connections.map((connection) => (
              <tr key={connection.connection_id}>
                <td>
                  {renameFor === connection.connection_id ? (
                    <>
                      <label htmlFor={`rename-${connection.connection_id}`}>
                        {t("connections.alias.renameLabel")}
                      </label>
                      <input
                        id={`rename-${connection.connection_id}`}
                        type="text"
                        value={renameAlias}
                        onChange={(event) => setRenameAlias(event.target.value)}
                      />
                      <button
                        type="button"

                        onClick={() => {
                          const alias = renameAlias.trim();
                          if (!alias) return;
                          void runAction(() =>
                            ipc.updateConnection({
                              connection_id: connection.connection_id,
                              expected_config_revision: connection.config_revision,
                              alias,
                            }),
                          ).then(() => setRenameFor(null));
                        }}
                      >
                        {t("common.action.save")}
                      </button>
                      <button type="button" onClick={() => setRenameFor(null)}>
                        {t("common.action.cancel")}
                      </button>
                    </>
                  ) : (
                    <>
                      {connection.alias}
                      <button
                        type="button"
                        onClick={() => {
                          setRenameFor(connection.connection_id);
                          setRenameAlias(connection.alias);
                        }}
                      >
                        {t("connections.alias.rename")}
                      </button>
                    </>
                  )}
                </td>
                <td>{t(PROVIDER_LABELS[connection.provider])}</td>
                <td>
                  {t(LIFECYCLE_LABELS[connection.lifecycle])}
                  {connection.lifecycle === "needs_credentials" ? (
                    <p className="note">{t("connections.needsCredentials.note")}</p>
                  ) : null}
                  {connection.lifecycle === "deletion_pending" ? (
                    <p className="note">{t("connections.deletionPending.note")}</p>
                  ) : null}
                </td>
                <td>{connection.config_revision}</td>
                <td>
                  <button
                    type="button"
                    disabled={connection.lifecycle !== "active" && connection.lifecycle !== "needs_credentials"}
                    onClick={() =>
                      void runAction(() =>
                        ipc.setConnectionEnabled({
                          connection_id: connection.connection_id,
                          expected_config_revision: connection.config_revision,
                          enabled: false,
                        }),
                      )
                    }
                  >
                    {t("connections.action.disable")}
                  </button>
                  <button
                    type="button"
                    disabled={connection.lifecycle !== "disabled"}
                    onClick={() =>
                      void runAction(() =>
                        ipc.setConnectionEnabled({
                          connection_id: connection.connection_id,
                          expected_config_revision: connection.config_revision,
                          enabled: true,
                        }),
                      )
                    }
                  >
                    {t("connections.action.enable")}
                  </button>
                  <button
                    type="button"
                    disabled={connection.lifecycle === "pending_setup"}
                    onClick={() => {
                      setRotateFor(
                        rotateFor === connection.connection_id
                          ? null
                          : connection.connection_id,
                      );
                      setRotateDraft({ ...EMPTY_DRAFT, secret: "" });
                    }}
                  >
                    {t("connections.action.rotate")}
                  </button>
                  <button
                    type="button"
                    disabled={connection.lifecycle === "pending_setup"}
                    onClick={() =>
                      void runAction(() =>
                        ipc.removeConnection({
                          connection_id: connection.connection_id,
                          expected_config_revision: connection.config_revision,
                          deletion_intent: "credentials_history_and_managed_backups",
                        }),
                      )
                    }
                  >
                    {connection.lifecycle === "deletion_pending"
                      ? t("connections.action.remove.retry")
                      : t("connections.action.remove")}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {connections.connections
        .filter((connection) => connection.connection_id === rotateFor)
        .map((connection) => (
          <form
            key={connection.connection_id}
            onSubmit={(event) => {
              event.preventDefault();
              void submitRotate(connection.connection_id, connection.config_revision);
            }}
          >
            <h4>{t("connections.rotate.heading")}</h4>
            <CredentialFields
              idPrefix={`rotate-${connection.connection_id}`}
              draft={rotateDraft}
              onChange={setRotateDraft}
            />
            <button type="submit" disabled={rotateBusy}>
              {t("connections.rotate.submit")}
            </button>
            <p className="note">{t("connections.rotate.note")}</p>
          </form>
        ))}

      {lastError ? (
        <div role="alert">
          <h4>{t("connections.error.heading")}</h4>
          <p>{lastError}</p>
        </div>
      ) : null}

      {lastOperation ? <OperationStatus operation={lastOperation} /> : null}

      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submitAdd();
        }}
      >
        <h3>{t("connections.add.heading")}</h3>
        <div>
          <label htmlFor="add-provider">{t("connections.add.provider")}</label>
          <select
            id="add-provider"
            value={addProvider}
            onChange={(event) => setAddProvider(event.target.value as Provider)}
          >
            <option value="kimi">{t("connections.provider.kimi")}</option>
            <option value="openrouter">{t("connections.provider.openrouter")}</option>
          </select>
        </div>
        <div>
          <label htmlFor="add-alias">{t("connections.add.alias")}</label>
          <input
            id="add-alias"
            type="text"
            value={addAlias}
            placeholder={t("connections.alias.placeholder")}
            onChange={(event) => setAddAlias(event.target.value)}
          />
          <p className="note">{t("connections.alias.hint")}</p>
        </div>
        <CredentialFields idPrefix="add" draft={addDraft} onChange={setAddDraft} />
        <button type="submit" disabled={addBusy}>
          {t("connections.add.submit")}
        </button>
        <p className="note">{t("connections.add.note")}</p>
      </form>
    </section>
  );

}
