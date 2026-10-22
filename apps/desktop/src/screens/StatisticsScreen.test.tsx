import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n";
import { revisionTracker } from "../ipc";
import StatisticsScreen from "./StatisticsScreen";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

function metric(overrides: Record<string, unknown>) {
  return {
    metric_id: "recorded_tokens",
    scope_id: "all-scopes",
    product_id: null,
    unit: { kind: "tokens" },
    value: null,
    availability: "missing",
    provenance: "validated_derivation",
    observed_at: "2026-10-21T09:00:00Z",
    source_as_of: null,
    freshness: "no_value",
    query_kind: "historical",
    alert_eligible: false,
    period: null,
    reason: "collection_gap",
    ...overrides,
  };
}

function point(value: string | null, start: string, end: string) {
  return {
    interval: { start, end },
    zone: "UTC",
    metrics: [
      metric({
        value,
        availability: value === null ? "missing" : "available",
        freshness: value === null ? "no_value" : "current",
        reason: value === null ? "collection_gap" : null,
      }),
    ],
    coverage: [
      {
        interval: { start, end },
        status: value === null ? "missing" : "complete",
        reason: value === null ? "collection_gap" : null,
      },
    ],
  };
}

function statisticsData(overrides: Record<string, unknown> = {}) {
  return {
    requested_interval: { start: "2026-10-15T00:00:00Z", end: "2026-10-22T00:00:00Z" },
    effective_intervals: [{ start: "2026-10-15T00:00:00Z", end: "2026-10-22T00:00:00Z" }],
    reporting_zone: "UTC",
    grain: "day",
    as_of: "2026-10-21T09:00:00Z",
    provisional: true,
    coverage: [],
    excluded_scopes: [],
    totals: [
      metric({
        value: "2469135780247",
        availability: "available",
        freshness: "current",
        reason: null,
      }),
    ],
    series: [
      point("1234567890123", "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z"),
      point("1234567890124", "2026-10-16T00:00:00Z", "2026-10-17T00:00:00Z"),
    ],
    plans: [
      {
        scope_id: "synthetic-scope-kimi",
        product_id: "shared-pool",
        period: null,
        metrics: [
          metric({
            scope_id: "synthetic-scope-kimi",
            product_id: "shared-pool",
            value: "1234567890123",
            availability: "available",
            freshness: "current",
            reason: null,
          }),
          metric({
            metric_id: "quota_remaining",
            scope_id: "synthetic-scope-kimi",
            product_id: "shared-pool",
            unit: { kind: "ratio" },
            value: "0.70",
            availability: "available",
            freshness: "current",
            query_kind: "unvalidated",
            provenance: "synthetic",
            source_as_of: "2026-10-21T09:00:00Z",
            reason: null,
          }),
        ],
      },
    ],
    data_revision: "1",
    ...overrides,
  };
}

function envelope(data: unknown) {
  return {
    status: "ok",
    contract_version: 1,
    request_id: "fixture-request",
    data: { command: "query_statistics", data },
  };
}

let changeHandler: ((event: { payload: unknown }) => void) | undefined;
let fixture: unknown = statisticsData();

beforeEach(async () => {
  window.localStorage.setItem("quotavista.ui_locale", "en");
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
  changeHandler = undefined;
  fixture = statisticsData();
  vi.mocked(invoke).mockImplementation(async () => envelope(fixture));
  vi.mocked(listen).mockImplementation((async (_event: string, handler: unknown) => {
    changeHandler = handler as (event: { payload: unknown }) => void;
    return vi.fn();
  }) as never);
  revisionTracker.reset("0");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
});

function renderScreen() {
  return render(
    <I18nProvider>
      <StatisticsScreen />
    </I18nProvider>,
  );
}

async function announce(revision: string): Promise<void> {
  await waitFor(() => expect(changeHandler).toBeDefined());
  await act(async () => {
    changeHandler?.({
      payload: {
        data_revision: revision,
        affected_connection_ids: [],
        affected_scope_ids: [],
      },
    });
  });
}

describe("StatisticsScreen", () => {
  it("renders the exact backend totals and per-point values", async () => {
    renderScreen();
    // 1,234,567,890,123 + 1,234,567,890,124 = 2,469,135,780,247.
    expect(await screen.findByText("2,469,135,780,247")).not.toBeNull();
    const description = document.getElementById("statistics-chart-description-0");
    expect(description?.textContent).toContain("2026-10-15: 1,234,567,890,123");
    expect(description?.textContent).toContain("2026-10-16: 1,234,567,890,124");
  });

  it("shows the explicit range dates, zone, and provisional marker", async () => {
    // last7 is today-6 through today. Freeze UTC so this assertion does not
    // follow the wall clock; Date only, so waitFor still uses real timers.
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-10-21T12:00:00Z"));
    renderScreen();
    expect(
      await screen.findByText("Range: 2026-10-15 – 2026-10-21 · zone UTC · Day"),
    ).not.toBeNull();
    expect(screen.getByText(/Data as of/)).not.toBeNull();
    expect(screen.getByText(/Provisional/)).not.toBeNull();
  });
  it("shows unavailable buckets as unknown, never as zero", async () => {
    fixture = statisticsData({
      series: [
        point("1234567890123", "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z"),
        point(null, "2026-10-16T00:00:00Z", "2026-10-17T00:00:00Z"),
      ],
    });
    renderScreen();
    await screen.findByText("2,469,135,780,247");
    const description = document.getElementById("statistics-chart-description-0");
    expect(description?.textContent).toContain("2026-10-16: unknown");
    expect(screen.queryByText(/^0(\.0+)?$/)).toBeNull();
  });

  it("shows the empty explanation instead of a zero line when nothing was recorded", async () => {
    fixture = statisticsData({
      totals: [metric({})],
      series: [point(null, "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z")],
    });
    renderScreen();
    expect(await screen.findByText(/No recorded tokens in this range/i)).not.toBeNull();
    expect(screen.getAllByText(/unknown/).length).toBeGreaterThan(0);
  });

  it("lists excluded scopes with their reason", async () => {
    fixture = statisticsData({
      excluded_scopes: [
        { scope_id: "synthetic-scope-kimi", metric_id: "tokens", reason: "outside_lookback" },
      ],
    });
    renderScreen();
    await screen.findByText("2,469,135,780,247");
    expect(screen.getByText(/synthetic-scope-kimi excluded from tokens/)).not.toBeNull();
    expect(screen.getByText("outside the recorded lookback")).not.toBeNull();
  });

  it("renders per-plan rows with native units and percentages", async () => {
    renderScreen();
    await screen.findByText("2,469,135,780,247");
    expect(screen.getByText("shared-pool")).not.toBeNull();
    // The 0.70 ratio renders as an exact percentage, not a rounded Number.
    expect(screen.getByText("70%")).not.toBeNull();
    // The range total is rendered per plan, exact, never rounded.
    expect(screen.getAllByText("1,234,567,890,123").length).toBeGreaterThan(0);
  });

  it("marks partial buckets instead of presenting them as final", async () => {
    fixture = statisticsData({
      totals: [
        metric({
          value: "1234567890123",
          availability: "available",
          freshness: "current",
          reason: null,
        }),
      ],
      series: [
        {
          ...point("1234567890123", "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z"),
          coverage: [
            {
              interval: { start: "2026-10-15T00:00:00Z", end: "2026-10-16T00:00:00Z" },
              status: "partial",
              reason: "not_yet_settled",
            },
          ],
        },
      ],
    });
    renderScreen();
    await screen.findAllByText("1,234,567,890,123");
    const description = document.getElementById("statistics-chart-description-0");
    expect(description?.textContent).toContain(
      "2026-10-15: 1,234,567,890,123 (partial — not yet settled)",
    );
    expect(document.querySelector(".bar-partial")).not.toBeNull();
  });

  it("keeps the previous view when a reload fails", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    renderScreen();
    await screen.findByText("2,469,135,780,247");

    vi.mocked(invoke).mockRejectedValue(new Error("store locked"));
    await announce("2");

    // The failure note appears but the last good view stays on screen.
    expect(await screen.findByRole("alert")).not.toBeNull();
    expect(screen.getByText("2,469,135,780,247")).not.toBeNull();
  });

  it("reveals explicit custom date inputs with a future-date ceiling", async () => {
    renderScreen();
    await screen.findByText("2,469,135,780,247");
    fireEvent.change(screen.getByLabelText("Date range"), { target: { value: "custom" } });
    const start = screen.getByLabelText(/Custom range start date/) as HTMLInputElement;
    const end = screen.getByLabelText(/Custom range end date/) as HTMLInputElement;
    const today = new Date().toISOString().slice(0, 10);
    expect(start.max).toBe(today);
    expect(end.max).toBe(today);
    fireEvent.change(start, { target: { value: "2026-10-01" } });
    fireEvent.change(end, { target: { value: "2026-10-05" } });
    expect(start.value).toBe("2026-10-01");
    expect(end.value).toBe("2026-10-05");
  });
});
