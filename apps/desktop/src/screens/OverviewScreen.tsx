import { useEffect, useRef, useState } from "react";
import {
  ipc,
  isObsoleteResponse,
  revisionTracker,
  subscribeToChanges,
} from "../ipc";
import { useI18n, type MessageKey } from "../i18n";
import type { ConnectionList } from "../generated/contracts";

// Synthetic demo snapshot. Unknown stays unknown (never rendered as zero);
// exact strings and the independent reporting zone flow through formatters.
// Application-owned prose from the backend maps to catalog keys so both
// locales read naturally; identifiers (provenance, store) stay untranslated.
const STATUS_MESSAGES: Record<string, MessageKey> = {
  "Development scaffold running without provider credentials.":
    "overview.status.scaffoldMessage",
};
const DEMO_REPORTING_ZONE = "UTC";
const DEMO_SNAPSHOT = {
  quotaRemaining: null,
  quotaUsed: "123456789012345678",
  balance: "12.50",
  observedAt: "2026-10-18T12:00:00Z",
};

export default function OverviewScreen() {
  const { t, formatTokens, formatDecimal, formatDateTime } = useI18n();
  const [connections, setConnections] = useState<ConnectionList | null>(null);
  const [failed, setFailed] = useState(false);
  // Generation of the newest read. A response or an error that belongs to an
  // older read is obsolete and must not replace the newer view.
  const generation = useRef(0);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => Promise<void>) | undefined;

    // A response older than the observed revision is obsolete: rendering it
    // would overwrite newer data, so it is discarded.
    const load = async (): Promise<void> => {
      const request = generation.current + 1;
      generation.current = request;
      try {
        const list = await ipc.listConnections();
        if (disposed) return;
        // A response from an older read is obsolete: rendering it would
        // overwrite newer data.
        if (request !== generation.current) return;
        if (isObsoleteResponse(list.data_revision, revisionTracker.observed())) return;
        revisionTracker.observe(list.data_revision);
        setConnections(list);
        setFailed(false);
      } catch {
        // An error from an older read is obsolete as well: it must not replace
        // the newer view with the error state.
        if (disposed || request !== generation.current) return;
        setFailed(true);
      }
    };

    // The listener is installed before the first read. A commit that lands
    // between the read and the registration would otherwise be missed.
    const start = async (): Promise<void> => {
      try {
        const dispose = await subscribeToChanges(revisionTracker, (_event, update) => {
          // A gap means a revision was skipped, so the local view cannot be
          // patched and is reloaded instead. A stale event carries nothing
          // newer than the rendered revision, so it is ignored.
          if (update.kind === "apply" || update.kind === "gap") void load();
        });
        if (disposed) {
          void dispose();
          return;
        }
        unsubscribe = dispose;
      } catch {
        // A live channel that cannot be installed leaves the screen on its
        // last known state instead of an empty view.
      }
      await load();
    };
    void start();

    return () => {
      disposed = true;
      void unsubscribe?.();
    };
  }, []);

  if (failed) {
    return <p role="alert" className="error">{t("overview.error.statusUnavailable")}</p>;
  }
  if (!connections) {
    return <p role="status">{t("common.loading")}</p>;
  }

  const provenance = "synthetic";
  const providerCount = connections.connections.length;
  const dataRevision = connections.data_revision;
  const message = "Development scaffold running without provider credentials.";

  return (
    <section aria-labelledby="overview-heading">
      <h2 id="overview-heading">{t("overview.heading")}</h2>

      <dl>
        <dt>{t("overview.status.provenance")}</dt>
        <dd>{provenance}</dd>
        <dt>{t("overview.status.providers")}</dt>
        <dd>{providerCount}</dd>
        <dt>{t("overview.status.dataRevision")}</dt>
        <dd>{dataRevision}</dd>
        <dt>{t("overview.status.message")}</dt>
        <dd>
          {STATUS_MESSAGES[message]
            ? t(STATUS_MESSAGES[message])
            : message}
        </dd>
      </dl>

      <h3>{t("overview.metrics.heading")}</h3>
      <dl>
        <dt>{t("overview.metric.quotaRemaining")}</dt>
        <dd>{t("common.value.unknown")}</dd>
        <dt>{t("overview.metric.quotaUsed")}</dt>
        <dd>{formatTokens(DEMO_SNAPSHOT.quotaUsed)}</dd>
        <dt>{t("overview.metric.balance")}</dt>
        <dd>{formatDecimal(DEMO_SNAPSHOT.balance)}</dd>
        <dt>{t("overview.metric.observedAt")}</dt>
        <dd>
          <time dateTime={DEMO_SNAPSHOT.observedAt}>
            {formatDateTime(new Date(DEMO_SNAPSHOT.observedAt), DEMO_REPORTING_ZONE)}
          </time>
        </dd>
      </dl>
      <p className="note">{t("overview.metric.note")}</p>
    </section>
  );
}
