import { useState } from "react";
import { useI18n } from "../i18n";

type RangeFilter = "last7" | "last30" | "none";

const RANGE_KEYS: Record<RangeFilter, "statistics.filter.last7" | "statistics.filter.last30" | "statistics.filter.none"> = {
  last7: "statistics.filter.last7",
  last30: "statistics.filter.last30",
  none: "statistics.filter.none",
};

// Backend-prepared synthetic series; the chart only renders, never recalculates.
const DEMO_SERIES = [4200, 8100, 12000, 6300, 9400, 3000, 5600];
const MAX_VALUE = Math.max(...DEMO_SERIES);

export default function StatisticsScreen() {
  const { t, formatTokens } = useI18n();
  const [range, setRange] = useState<RangeFilter>("last7");
  const hasData = range !== "none";

  return (
    <section aria-labelledby="statistics-heading">
      <h2 id="statistics-heading">{t("statistics.heading")}</h2>

      <label htmlFor="statistics-range">{t("statistics.filter.range")}</label>
      <select
        id="statistics-range"
        value={range}
        onChange={(event) => setRange(event.target.value as RangeFilter)}
      >
        {(Object.keys(RANGE_KEYS) as RangeFilter[]).map((option) => (
          <option key={option} value={option}>
            {t(RANGE_KEYS[option])}
          </option>
        ))}
      </select>

      <h3>{t("statistics.series.heading")}</h3>
      {hasData ? (
        <figure className="chart">
          <svg
            viewBox="0 0 700 200"
            role="img"
            aria-label={t("statistics.chart.label")}
            aria-describedby="statistics-chart-description"
          >
            {DEMO_SERIES.map((value, index) => {
              const height = (value / MAX_VALUE) * 180;
              return (
                <rect
                  key={index}
                  x={20 + index * 100}
                  y={190 - height}
                  width={80}
                  height={height}
                  className="chart-bar"
                />
              );
            })}
          </svg>
          <figcaption id="statistics-chart-description">
            {t("statistics.chart.description")} ({t("statistics.legend.tokens")}:{" "}
            {DEMO_SERIES.map((value) => formatTokens(String(value))).join(", ")})
          </figcaption>
        </figure>
      ) : (
        <p className="empty">
          {t("common.empty")}. {t("statistics.chart.empty")}
        </p>
      )}
    </section>
  );
}
