import { useEffect, useRef, useState } from "react";
import {
  ipc,
  isObsoleteResponse,
  revisionTracker,
  subscribeToChanges,
} from "../ipc";
import { useI18n, type MessageKey } from "../i18n";
import type { ConnectionList, MetricDto, OverviewDto } from "../generated/contracts";
import MetricValue from "../components/MetricValue";

// The prototype reports in UTC until P1-10 wires the persisted reporting zone.
const REPORTING_ZONE = "UTC";

// Current UTC calendar date; the overview queries the single current day.
function todayUtc(): string {
  return new Date().toISOString().slice(0, 10);
}

// Catalog key for each current-metric id the backend can return.
const METRIC_LABELS: Record<string, MessageKey> = {
  tokens: "overview.metric.tokens",
  recorded_tokens: "overview.metric.recordedTokens",
  quota_remaining: "overview.metric.quotaRemaining",
  quota_consumed: "overview.metric.quotaConsumed",
  spend: "overview.metric.spend",
};

function MetricTimestamps({ metric }: { metric: MetricDto }) {
  const { t, formatDateTime } = useI18n();
  return (
    <div className="metric-timestamps">
      <small>{t("overview.metric.observedAt", { at: formatDateTime(new Date(metric.observed_at), REPORTING_ZONE) })}</small>
      {metric.source_as_of ? (
        <small>{t("overview.metric.sourceAsOf", { at: formatDateTime(new Date(metric.source_as_of), REPORTING_ZONE) })}</small>
      ) : null}
      {metric.freshness === "stale" ? (
        <span className="freshness-badge">{t("common.freshness.stale")}</span>
      ) : null}
    </div>
  );
}

function ScopeSection({ metrics }: { metrics: MetricDto[] }) {
  const { t } = useI18n();
  const scopeId = metrics[0]?.scope_id ?? "";
  const plan = metrics[0]?.product_id ?? null;
  return (
    <section className="subsection">
      <h3>{t("overview.scope.heading", { scope: scopeId })}</h3>
      {plan ? <p>{t("overview.scope.plan", { plan })}</p> : null}
      <dl className="metric-list">
        {metrics.map((metric) => (
          <div key={metric.metric_id} className="metric-row">
            <dt>
              {METRIC_LABELS[metric.metric_id]
                ? t(METRIC_LABELS[metric.metric_id])
                : metric.metric_id}
              {metric.freshness === "stale" ? (
                <span className="freshness-badge"> ({t("common.freshness.stale")})</span>
              ) : null}
            </dt>
            <dd>
              <MetricValue metric={metric} />
              <MetricTimestamps metric={metric} />
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

export default function OverviewScreen() {
  const { t, formatDateTime } = useI18n();
  const [overview, setOverview] = useState<OverviewDto | null>(null);
  const [connections, setConnections] = useState<ConnectionList | null>(null);
  const [failed, setFailed] = useState(false);
  const readGeneration = useRef(0);

  useEffect(() => {
    let alive = true;
    const load = async () => {
      const generation = ++readGeneration.current;
      const today = todayUtc();
      try {
        const [connectionList, nextOverview] = await Promise.all([
          ipc.listConnections(),
          ipc.getOverview({
            start_date: today,
            end_date: today,
            reporting_zone: REPORTING_ZONE,
          }),
        ]);
        if (!alive || generation !== readGeneration.current) return;
        const observed = revisionTracker.observed();
        if (
          isObsoleteResponse(connectionList.data_revision, observed) ||
          isObsoleteResponse(nextOverview.summary.data_revision, observed)
        ) {
          return;
        }
        revisionTracker.observe(connectionList.data_revision);
        revisionTracker.observe(nextOverview.summary.data_revision);
        setConnections(connectionList);
        setOverview(nextOverview);
        setFailed(false);
      } catch {
        if (alive && generation === readGeneration.current) setFailed(true);
      }
    };
    let unsubscribe: (() => Promise<void>) | undefined;
    void subscribeToChanges(revisionTracker, () => {
      void load();
    })
      .then((release) => {
        unsubscribe = release;
      })
      .catch(() => {
        // The change channel is optional: without it the last read stays on
        // screen (the tests exercise this fallback explicitly).
      });
    void load();
    return () => {
      alive = false;
      void unsubscribe?.();
    };
  }, []);

  if (!overview || !connections) {
    return (
      <section className="panel">
        <h2>{t("nav.overview")}</h2>
        {failed ? (
          <p role="alert" className="error">
            {t("overview.error.statusUnavailable")}
          </p>
        ) : (
          <p className="note">{t("common.loading")}</p>
        )}
      </section>
    );
  }

  const summary = overview.summary;
  const total = summary.totals.find((metric) => metric.metric_id === "recorded_tokens");
  const asOf = formatDateTime(new Date(summary.as_of), REPORTING_ZONE);

  return (
    <section className="panel">
      <h2>{t("nav.overview")}</h2>
      <p>
        {t("overview.summary.range", {
          start: todayUtc(),
          end: todayUtc(),
          zone: summary.reporting_zone,
        })}
        {summary.provisional ? ` ${t("overview.summary.provisional", { asOf })}` : ""}
      </p>
      <section className="subsection">
        <h3>{t("overview.summary.heading")}</h3>
        <dl className="metric-list">
          {total ? (
            <div className="metric-row">
              <dt>{t("overview.metric.recordedTokens")}</dt>
              <dd>
                <MetricValue metric={total} />
              </dd>
            </div>
          ) : null}
          <div className="metric-row">
            <dt>{t("overview.status.dataRevision")}</dt>
            <dd>{summary.data_revision}</dd>
          </div>
        </dl>
      </section>
      {overview.metrics
        .reduce<MetricDto[][]>((groups, metric) => {
          const last = groups[groups.length - 1];
          if (last && last[0].scope_id === metric.scope_id) {
            last.push(metric);
          } else {
            groups.push([metric]);
          }
          return groups;
        }, [])
        .map((metrics) => (
          <ScopeSection key={metrics[0].scope_id} metrics={metrics} />
        ))}
    </section>
  );
}
