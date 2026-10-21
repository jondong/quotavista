import { useEffect, useRef, useState } from "react";
import {
  ipc,
  isObsoleteResponse,
  revisionTracker,
  subscribeToChanges,
} from "../ipc";
import { useI18n, type MessageKey } from "../i18n";
import type {
  CoverageDto,
  Grain,

  StatisticsDto,
  StatisticsPoint,
} from "../generated/contracts";
import MetricValue, { REASON_KEYS, ReasonText } from "../components/MetricValue";

// The prototype reports in UTC until P1-10 wires the persisted reporting zone.
const REPORTING_ZONE = "UTC";

type Preset =
  | "today"
  | "yesterday"
  | "thisWeek"
  | "lastWeek"
  | "thisMonth"
  | "last7"
  | "last30"
  | "custom";

const PRESETS: Preset[] = [
  "today",
  "yesterday",
  "thisWeek",
  "lastWeek",
  "thisMonth",
  "last7",
  "last30",
  "custom",
];

const PRESET_LABELS: Record<Preset, MessageKey> = {
  today: "statistics.filter.today",
  yesterday: "statistics.filter.yesterday",
  thisWeek: "statistics.filter.thisWeek",
  lastWeek: "statistics.filter.lastWeek",
  thisMonth: "statistics.filter.thisMonth",
  last7: "statistics.filter.last7",
  last30: "statistics.filter.last30",
  custom: "statistics.filter.custom",
};

const PLAN_METRIC_LABELS: Record<string, MessageKey> = {
  recorded_tokens: "statistics.plans.recordedTokens",
  tokens: "statistics.plans.tokens",
  quota_remaining: "statistics.plans.quotaRemaining",
  quota_consumed: "statistics.plans.quotaConsumed",
  spend: "statistics.plans.spend",
};

const PLAN_METRIC_ORDER = [
  "recorded_tokens",
  "tokens",
  "quota_remaining",
  "quota_consumed",
  "spend",
];

function todayUtc(): string {
  return new Date().toISOString().slice(0, 10);
}

function addDays(date: string, days: number): string {
  const parsed = new Date(`${date}T00:00:00Z`);
  parsed.setUTCDate(parsed.getUTCDate() + days);
  return parsed.toISOString().slice(0, 10);
}

// Monday of the ISO week containing the UTC date.
function mondayOf(date: string): string {
  const parsed = new Date(`${date}T00:00:00Z`);
  const daysSinceMonday = (parsed.getUTCDay() + 6) % 7;
  return addDays(date, -daysSinceMonday);
}

function monthStart(date: string): string {
  return `${date.slice(0, 8)}01`;
}

function presetRange(preset: Preset, today: string): { start: string; end: string } | null {
  switch (preset) {
    case "today":
      return { start: today, end: today };
    case "yesterday":
      return { start: addDays(today, -1), end: addDays(today, -1) };
    case "thisWeek":
      return { start: mondayOf(today), end: today };
    case "lastWeek": {
      const monday = mondayOf(today);
      return { start: addDays(monday, -7), end: addDays(monday, -1) };
    }
    case "thisMonth":
      return { start: monthStart(today), end: today };
    case "last7":
      return { start: addDays(today, -6), end: today };
    case "last30":
      return { start: addDays(today, -29), end: today };
    case "custom":
      return null;
  }
}

function pointValue(point: StatisticsPoint): string | null {
  const metric = point.metrics.find((entry) => entry.metric_id === "recorded_tokens");
  return metric?.value ?? null;
}

const COVERAGE_KEYS: Record<string, MessageKey> = {
  complete: "statistics.coverage.complete",
  partial: "statistics.coverage.partial",
  missing: "statistics.coverage.missing",
  unsupported: "statistics.coverage.unsupported",
};

// First non-complete coverage row the backend attached to a point, if any.
function pointCoverage(point: StatisticsPoint): CoverageDto | undefined {
  return point.coverage.find((row) => row.status !== "complete");
}

// Calendar date that begins the bucket, in the bucket's own zone. A plain UTC
// slice would show the wrong date for zones east of UTC.
function pointLabel(point: StatisticsPoint): string {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: point.zone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(new Date(point.interval.start));
  const part = (type: string) => parts.find((entry) => entry.type === type)?.value ?? "";
  return `${part("year")}-${part("month")}-${part("day")}`;
}
function LaneChart({
  lane,
  laneIndex,
  grainLabel,
}: {
  lane: { zone: string; points: StatisticsPoint[] };
  laneIndex: number;
  grainLabel: string;
}) {
  const { t, formatTokens } = useI18n();
  const values = lane.points.map(pointValue);
  const known = values.filter((value): value is string => value !== null);
  const coverageOf = lane.points.map(pointCoverage);
  const descriptionId = `statistics-chart-description-${laneIndex}`;
  if (known.length === 0) {
    return (
      <p id={descriptionId} className="chart-empty">
        {t("statistics.chart.empty")}
      </p>
    );
  }
  const max = known.reduce((a, b) => (BigInt(a) > BigInt(b) ? a : b));
  const maxBig = BigInt(max);
  return (
    <figure className="statistics-chart">
      <svg
        role="img"
        aria-label={t("statistics.chart.label", { grain: grainLabel })}
        aria-describedby={descriptionId}
        viewBox={`0 0 ${lane.points.length * 100} 220`}
      >
        {lane.points.map((point, index) => {
          const value = values[index];
          const height = value === null ? 0 : Number((BigInt(value) * 180n) / maxBig);
          return (
            <g key={`${lane.zone}-${pointLabel(point)}`}>
              <rect
                x={index * 100 + 10}
                y={200 - height}
                width="80"
                height={height}
                className={
                  value === null ? "bar bar-unknown" : coverageOf[index] ? "bar bar-partial" : "bar"
                }
              />
              <text x={index * 100 + 50} y="216" textAnchor="middle" className="bar-label">
                {pointLabel(point)}
              </text>
            </g>
          );
        })}
      </svg>
      <figcaption id={descriptionId}>
        {t("statistics.chart.values")}
        {lane.points
          .map((point, index) => {
            const value = values[index];
            const valueText =
              value === null ? t("common.value.unknown") : formatTokens(value);
            const coverage = coverageOf[index];
            if (!coverage) return `${pointLabel(point)}: ${valueText}`;
            const statusKey = COVERAGE_KEYS[coverage.status];
            const status = statusKey ? t(statusKey) : coverage.status;
            const reasonKey = coverage.reason ? REASON_KEYS[coverage.reason] : undefined;
            const reason = coverage.reason
              ? reasonKey
                ? t(reasonKey)
                : coverage.reason
              : null;
            const detail = reason ? `${status} — ${reason}` : status;
            return `${pointLabel(point)}: ${valueText} (${detail})`;
          })
          .join("; ")}
      </figcaption>
    </figure>
  );
}

function PlanTable({ stats }: { stats: StatisticsDto }) {
  const { t } = useI18n();
  return (
    <table className="plans-table">
      <thead>
        <tr>
          <th scope="col">{t("statistics.plans.scope")}</th>
          <th scope="col">{t("statistics.plans.plan")}</th>
          {PLAN_METRIC_ORDER.map((metricId) => (
            <th scope="col" key={metricId}>
              {t(PLAN_METRIC_LABELS[metricId])}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {stats.plans.map((row) => {
          const byId = new Map(row.metrics.map((metric) => [metric.metric_id, metric]));
          return (
            <tr key={`${row.scope_id}-${row.product_id}`}>
              <td>{row.scope_id}</td>
              <td>{row.product_id}</td>
              {PLAN_METRIC_ORDER.map((metricId) => {
                const metric = byId.get(metricId);
                return (
                  <td key={metricId}>
                    {metric ? <MetricValue metric={metric} /> : t("common.value.unknown")}
                    {metric && metric.freshness === "stale" ? (
                      <span className="freshness-badge"> ({t("common.freshness.stale")})</span>
                    ) : null}
                  </td>
                );
              })}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

export default function StatisticsScreen() {
  const { t, formatDateTime } = useI18n();
  const [preset, setPreset] = useState<Preset>("last7");
  const [customStart, setCustomStart] = useState<string>(() => addDays(todayUtc(), -6));
  const [customEnd, setCustomEnd] = useState<string>(todayUtc);
  const [grain, setGrain] = useState<Grain>("day");
  const [stats, setStats] = useState<StatisticsDto | null>(null);
  const [failed, setFailed] = useState(false);
  const readGeneration = useRef(0);

  useEffect(() => {
    let alive = true;
    const load = async () => {
      const generation = ++readGeneration.current;
      const today = todayUtc();
      const range = presetRange(preset, today) ?? {
        start: customStart,
        end: customEnd,
      };
      if (!range.start || !range.end) return;
      try {
        const next = await ipc.queryStatistics({
          start_date: range.start,
          end_date: range.end,
          reporting_zone: REPORTING_ZONE,
          grain,
        });
        if (!alive || generation !== readGeneration.current) return;
        if (isObsoleteResponse(next.data_revision, revisionTracker.observed())) return;
        revisionTracker.observe(next.data_revision);
        setStats(next);
        setFailed(false);
      } catch {
        // Keep the last successful view; the note explains the failure.
        if (alive && generation === readGeneration.current) setFailed(true);
      }
    };
    let unsubscribe: (() => Promise<void>) | undefined;
    void subscribeToChanges(revisionTracker, () => {
      void load();
    }).then((release) => {
      unsubscribe = release;
    });
    void load();
    return () => {
      alive = false;
      void unsubscribe?.();
    };
  }, [preset, customStart, customEnd, grain]);

  const today = todayUtc();
  const range = presetRange(preset, today) ?? { start: customStart, end: customEnd };
  const grainLabel = t(
    grain === "day" ? "statistics.grain.day" : "statistics.grain.week",
  );

  return (
    <section className="panel">
      <h2>{t("nav.statistics")}</h2>
      <div className="statistics-controls">
        <label htmlFor="statistics-preset">{t("statistics.filter.range")}</label>
        <select
          id="statistics-preset"
          value={preset}
          onChange={(event) => setPreset(event.target.value as Preset)}
        >
          {PRESETS.map((key) => (
            <option key={key} value={key}>
              {t(PRESET_LABELS[key])}
            </option>
          ))}
        </select>
        {preset === "custom" ? (
          <>
            <label htmlFor="statistics-custom-start">{t("statistics.filter.customStart")}</label>
            <input
              id="statistics-custom-start"
              type="date"
              value={customStart}
              max={today}
              onChange={(event) => setCustomStart(event.target.value)}
            />
            <label htmlFor="statistics-custom-end">{t("statistics.filter.customEnd")}</label>
            <input
              id="statistics-custom-end"
              type="date"
              value={customEnd}
              max={today}
              onChange={(event) => setCustomEnd(event.target.value)}
            />
          </>
        ) : null}
        <label htmlFor="statistics-grain">{t("statistics.filter.grain")}</label>
        <select
          id="statistics-grain"
          value={grain}
          onChange={(event) => setGrain(event.target.value as Grain)}
        >
          <option value="day">{t("statistics.grain.day")}</option>
          <option value="week">{t("statistics.grain.week")}</option>
        </select>
      </div>

      {!stats ? (
        failed ? (
          <p role="alert">{t("statistics.error.queryFailed")}</p>
        ) : (
          <p>{t("common.loading")}</p>
        )
      ) : (
        <>
          {failed ? <p role="alert">{t("statistics.error.queryFailed")}</p> : null}
          <p>
            {t("statistics.range.summary", {
              start: range.start,
              end: range.end,
              zone: stats.reporting_zone,
              grain: grainLabel,
            })}
          </p>
          <p>
            {t("statistics.range.asOf", {
              instant: formatDateTime(new Date(stats.as_of), REPORTING_ZONE),
            })}
            {stats.provisional ? ` ${t("statistics.range.provisional")}` : ""}
          </p>
          <section>
            <h3>{t("statistics.total.recordedTokens")}</h3>
            <p className="statistics-total">
              {stats.totals
                .filter((metric) => metric.metric_id === "recorded_tokens")
                .map((metric) => <MetricValue key={metric.metric_id} metric={metric} />)}
            </p>
          </section>
          {[...stats.series.reduce<Map<string, StatisticsPoint[]>>((lanes, point) => {
            const lane = lanes.get(point.zone) ?? [];
            lane.push(point);
            lanes.set(point.zone, lane);
            return lanes;
          }, new Map())].map(([zone, points], index) => (
            <section key={zone} className="statistics-lane">
              <h3>{t("statistics.series.zoneLane", { zone })}</h3>
              <LaneChart lane={{ zone, points }} laneIndex={index} grainLabel={grainLabel} />
            </section>
          ))}
          {stats.excluded_scopes.length > 0 ? (
            <section>
              <h3>{t("statistics.excluded.heading")}</h3>
              <ul>
                {stats.excluded_scopes.map((excluded) => (
                  <li key={`${excluded.scope_id}-${excluded.metric_id}`}>
                    {t("statistics.excluded.entry", {
                      scope: excluded.scope_id,
                      metric: excluded.metric_id,
                    })}
                    {" — "}
                    <ReasonText reason={excluded.reason} />
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          <section>
            <h3>{t("statistics.plans.heading")}</h3>
            <PlanTable stats={stats} />
          </section>
        </>
      )}
    </section>
  );
}
