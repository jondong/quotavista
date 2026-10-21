import { useI18n, type MessageKey } from "../i18n";
import type { MetricDto } from "../generated/contracts";

/** Catalog key for each wire-level reason code. */
const REASON_KEYS: Record<string, MessageKey> = {
  permission_required: "statistics.reason.permissionRequired",
  not_yet_settled: "statistics.reason.notYetSettled",
  outside_lookback: "statistics.reason.outsideLookback",
  truncated: "statistics.reason.truncated",
  bucket_boundary_mismatch: "statistics.reason.bucketBoundaryMismatch",
  collection_gap: "statistics.reason.collectionGap",
  sync_pending: "statistics.reason.syncPending",
  unsupported: "statistics.reason.unsupported",
  no_data: "statistics.reason.noData",
  incompatible_unit: "statistics.reason.incompatibleUnit",
  unknown_scope: "statistics.reason.unknownScope",
};

/** Translated explanation for a backend reason code. */
export function ReasonText({ reason }: { reason: string }) {
  const { t } = useI18n();
  const key = REASON_KEYS[reason];
  return <span className="reason-text">{key ? t(key) : reason}</span>;
}

/**
 * One metric value with its unit. Availability states render as text, never
 * as a zero: a null value is "unknown" (with the backend reason when given),
 * an unsupported metric says so explicitly.
 */
export default function MetricValue({ metric }: { metric: MetricDto }) {
  const { t, formatTokens, formatDecimal, formatPercent } = useI18n();
  if (metric.availability === "unsupported") {
    return <span className="metric-state">{t("common.value.unsupported")}</span>;
  }
  const value = metric.value;
  if (value === null) {
    return (
      <span className="metric-state">
        {t("common.value.unknown")}
        {metric.reason ? (
          <>
            {" ("}
            <ReasonText reason={metric.reason} />)
          </>
        ) : null}
      </span>
    );
  }
  switch (metric.unit.kind) {
    case "tokens":
      return <>{formatTokens(value)}</>;
    case "money":
      return (
        <>
          {formatDecimal(value)} {metric.unit.currency}
        </>
      );
    case "ratio":
      return <>{formatPercent(value)}</>;
    case "percentage_points":
    case "native":
      return <>{formatDecimal(value)}</>;
  }
}
