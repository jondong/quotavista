import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useI18n, type MessageKey } from "../i18n";

interface SyntheticStatus {
  provenance: string;
  providerCount: number;
  store: string;
  message: string;
}

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
  const [status, setStatus] = useState<SyntheticStatus | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    invoke<SyntheticStatus>("synthetic_status")
      .then(setStatus)
      .catch(() => setFailed(true));
  }, []);

  if (failed) {
    return <p role="alert" className="error">{t("overview.error.statusUnavailable")}</p>;
  }
  if (!status) {
    return <p role="status">{t("common.loading")}</p>;
  }

  return (
    <section aria-labelledby="overview-heading">
      <h2 id="overview-heading">{t("overview.heading")}</h2>

      <dl>
        <dt>{t("overview.status.provenance")}</dt>
        <dd>{status.provenance}</dd>
        <dt>{t("overview.status.providers")}</dt>
        <dd>{status.providerCount}</dd>
        <dt>{t("overview.status.store")}</dt>
        <dd>{status.store}</dd>
        <dt>{t("overview.status.message")}</dt>
        <dd>
          {STATUS_MESSAGES[status.message]
            ? t(STATUS_MESSAGES[status.message])
            : status.message}
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
