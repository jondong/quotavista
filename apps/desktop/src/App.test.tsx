import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// The overview and statistics screens subscribe to `qv://change`; jsdom has
// no Tauri event bridge, so the listener is stubbed out.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function envelope(command: string, data: unknown) {
  return {
    status: "ok",
    contract_version: 1,
    request_id: "fixture-request",
    data: { command, data },
  };
}

const CONNECTIONS_DATA = {
  connections: [
    {
      connection_id: "demo-kimi",
      provider: "kimi",
      alias: "Demo Kimi",
      lifecycle: "active",
      config_revision: "1",
      generation: "1",
      datasets: [],
      capabilities: [],
    },
    {
      connection_id: "demo-openrouter",
      provider: "openrouter",
      alias: "Demo OpenRouter",
      lifecycle: "active",
      config_revision: "1",
      generation: "1",
      datasets: [],
      capabilities: [],
    },
  ],
  data_revision: "1",
};

function metric(overrides: Record<string, unknown>) {
  return {
    metric_id: "tokens",
    scope_id: "synthetic-scope-kimi",
    product_id: "shared-pool",
    unit: { kind: "tokens" },
    value: null,
    availability: "missing",
    provenance: "synthetic",
    observed_at: "2026-10-21T09:00:00Z",
    source_as_of: null,
    freshness: "no_value",
    query_kind: "unvalidated",
    alert_eligible: false,
    period: null,
    reason: "no_data",
    ...overrides,
  };
}

const OVERVIEW_DATA = {
  metrics: [
    metric({
      value: "1234567890123",
      availability: "available",
      freshness: "current",
      source_as_of: "2026-10-21T09:00:00Z",
      reason: null,
    }),
    metric({
      metric_id: "quota_remaining",
      unit: { kind: "ratio" },
      value: "0.70",
      availability: "available",
      freshness: "current",
      source_as_of: "2026-10-21T09:00:00Z",
      reason: null,
    }),
    metric({
      metric_id: "spend",
      scope_id: "synthetic-scope-openrouter",
      product_id: "prepaid-credits",
      unit: { kind: "money", currency: "USD" },
      value: "42.50",
      availability: "available",
      freshness: "current",
      source_as_of: "2026-10-21T09:00:00Z",
      reason: null,
    }),
    metric({
      metric_id: "quota_remaining",
      scope_id: "synthetic-scope-openrouter",
      product_id: "key-budget",
      unit: { kind: "ratio" },
      availability: "unsupported",
      reason: "unsupported",
    }),
    metric({
      metric_id: "tokens",
      scope_id: "synthetic-scope-openrouter",
      product_id: "key-budget",
    }),
  ],
  summary: {
    requested_interval: { start: "2026-10-21T00:00:00Z", end: "2026-10-22T00:00:00Z" },
    effective_intervals: [{ start: "2026-10-21T00:00:00Z", end: "2026-10-22T00:00:00Z" }],
    reporting_zone: "UTC",
    grain: "day",
    as_of: "2026-10-21T09:00:00Z",
    provisional: true,
    coverage: [],
    excluded_scopes: [],
    totals: [
      metric({
        metric_id: "recorded_tokens",
        scope_id: "all-scopes",
        product_id: null,
        value: "3703703670369",
        availability: "available",
        freshness: "current",
        query_kind: "historical",
        provenance: "validated_derivation",
        reason: null,
      }),
    ],
    series: [],
    plans: [],
    data_revision: "1",
  },
};

function statisticsPoint(value: string | null, start: string, end: string) {
  return {
    interval: { start, end },
    zone: "UTC",
    metrics: [
      metric({
        metric_id: "recorded_tokens",
        scope_id: "all-scopes",
        product_id: null,
        value,
        availability: value === null ? "missing" : "available",
        freshness: value === null ? "no_value" : "current",
        query_kind: "historical",
        provenance: "validated_derivation",
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

const STATISTICS_DATA = {
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
      metric_id: "recorded_tokens",
      scope_id: "all-scopes",
      product_id: null,
      value: "2469135780247",
      availability: "available",
      freshness: "current",
      query_kind: "historical",
      provenance: "validated_derivation",
      reason: null,
    }),
  ],
  series: [
    statisticsPoint("1234567890123", "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z"),
    statisticsPoint("1234567890124", "2026-10-16T00:00:00Z", "2026-10-17T00:00:00Z"),
  ],
  plans: [],
  data_revision: "1",
};

const EMPTY_STATISTICS_DATA = {
  ...STATISTICS_DATA,
  totals: [
    metric({
      metric_id: "recorded_tokens",
      scope_id: "all-scopes",
      product_id: null,
      query_kind: "historical",
      provenance: "validated_derivation",
    }),
  ],
  series: [statisticsPoint(null, "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z")],
};

let statisticsFixture: unknown = STATISTICS_DATA;

function setNavigatorLanguages(languages: string[]) {
  Object.defineProperty(window.navigator, "languages", {
    value: languages,
    configurable: true,
  });
  Object.defineProperty(window.navigator, "language", {
    value: languages[0] ?? "en",
    configurable: true,
  });
}

function headerLanguageSelect(): HTMLSelectElement {
  // Resolved by id, not label text, so the helper works in any locale.
  return document.getElementById("header-language") as HTMLSelectElement;
}

beforeEach(async () => {
  window.localStorage.clear();
  statisticsFixture = STATISTICS_DATA;
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (_command: string, payload: unknown) => {
    const command = (payload as { request: { command: { command: string } } }).request.command.command;
    switch (command) {
      case "get_overview":
        return envelope("get_overview", OVERVIEW_DATA);
      case "query_statistics":
        return envelope("query_statistics", statisticsFixture);
      default:
        return envelope("list_connections", CONNECTIONS_DATA);
    }
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("bilingual shell", () => {
  it("renders English by default for a non-Chinese system locale", async () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Overview" })).not.toBeNull();
    // The synthetic-data banner marks all demo content as non-real.
    expect(screen.getByText(/Prototype demo/i).textContent).toContain("synthetic data only");
    expect(await screen.findByText("Recorded tokens in range")).not.toBeNull();
  });

  it("chooses zh-CN on first launch for an explicitly Simplified Chinese system locale", async () => {
    setNavigatorLanguages(["zh-CN"]);
    render(<App />);
    expect((await screen.findByRole("heading", { name: "概览" })).textContent).toContain("概览");
    expect(document.documentElement.lang).toBe("zh-CN");
    // Overview labels come from the zh-CN catalog, not the English source.
    expect(await screen.findAllByText("范围内记录的 token")).not.toHaveLength(0);
  });

  it("keeps English for a Traditional Chinese system locale", async () => {
    setNavigatorLanguages(["zh-TW"]);
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Overview" })).not.toBeNull();
  });

  it("prefers a stored user override over system detection", async () => {
    setNavigatorLanguages(["zh-CN"]);
    window.localStorage.setItem("quotavista.ui_locale", "en");
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Overview" })).not.toBeNull();
  });

  it("switching language preserves the current screen, filter, and nonsecret input", async () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);

    // Statistics: pick a filter, then switch language from the header control.
    fireEvent.click(screen.getByRole("button", { name: "Statistics" }));
    await screen.findByText("2,469,135,780,247");
    const range = screen.getByLabelText("Date range") as HTMLSelectElement;
    fireEvent.change(range, { target: { value: "last30" } });
    expect(range.value).toBe("last30");

    fireEvent.change(headerLanguageSelect(), { target: { value: "zh-CN" } });

    // Still on Statistics, filter preserved, labels localized.
    expect(screen.getByRole("heading", { name: "统计" })).not.toBeNull();
    const zhRange = screen.getByLabelText("日期范围") as HTMLSelectElement;
    expect(zhRange.value).toBe("last30");
    expect(screen.getByRole("img", { name: "每日记录的 token 柱状图" })).not.toBeNull();
    // The chart description is wired into the accessibility tree via
    // aria-describedby, not just visually rendered.
    const chart = screen.getByRole("img", { name: "每日记录的 token 柱状图" });
    const describedBy = chart.getAttribute("aria-describedby");
    expect(describedBy).toBe("statistics-chart-description-0");
    expect(document.getElementById("statistics-chart-description-0")!.textContent).toContain(
      "逐点数值",
    );
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(window.localStorage.getItem("quotavista.ui_locale")).toBe("zh-CN");
    // The shell-level live region announces the header switch.
    expect(screen.getByText("界面语言已切换为 简体中文")).not.toBeNull();

    // Connections: draft alias survives a language switch.
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    const alias = screen.getByLabelText("连接别名") as HTMLInputElement;
    fireEvent.change(alias, { target: { value: "Work account" } });
    fireEvent.change(headerLanguageSelect(), { target: { value: "en" } });
    expect((screen.getByLabelText("Connection alias") as HTMLInputElement).value).toBe(
      "Work account",
    );
    expect(screen.getByText("Interface language changed to English")).not.toBeNull();
  });

  it("renders unknown and unsupported values as text, never as zero", async () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    await screen.findByText("1,234,567,890,123");
    // The OpenRouter scope has no shared quota: explicit, not a zero.
    expect(screen.getAllByText("not supported by this source")).not.toHaveLength(0);
    // A missing current value is reported as unknown.
    expect(screen.getAllByText(/unknown/).length).toBeGreaterThan(0);
    // No quota value may collapse an unknown state to zero.
    expect(screen.queryByText(/^0(\.0+)?$/)).toBeNull();
  });

  it("shows the localized error foundation when the status read fails", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("backend unavailable"));
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Backend status unavailable.");
  });

  it("shows the localized empty-state foundation for a range without data", async () => {
    statisticsFixture = EMPTY_STATISTICS_DATA;
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Statistics" }));
    expect(
      await screen.findByText(/No recorded tokens in this range/i),
    ).not.toBeNull();
  });

  it("re-translates the overview labels after a language switch", async () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    expect(await screen.findByText("Recorded tokens in range")).not.toBeNull();
    fireEvent.change(headerLanguageSelect(), { target: { value: "zh-CN" } });
    expect(await screen.findByText("范围内记录的 token")).not.toBeNull();
    expect(screen.queryByText("Recorded tokens in range")).toBeNull();
  });

  it("falls back to system detection when the locale store read fails", async () => {
    const getItem = vi
      .spyOn(Storage.prototype, "getItem")
      .mockImplementation(() => { throw new Error("storage blocked"); });
    setNavigatorLanguages(["zh-CN"]);
    render(<App />);
    expect(await screen.findByRole("heading", { name: "概览" })).not.toBeNull();
    getItem.mockRestore();
  });

  it("still switches language in memory when the locale store write fails", async () => {
    const setItem = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(() => { throw new Error("quota exceeded"); });
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Overview" })).not.toBeNull();
    fireEvent.change(headerLanguageSelect(), { target: { value: "zh-CN" } });
    expect(screen.getByRole("heading", { name: "概览" })).not.toBeNull();
    setItem.mockRestore();
  });
});
