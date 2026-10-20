import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const PROTOTYPE_ENVELOPE = {
  status: "ok",
  contract_version: 1,
  request_id: "fixture-request",
  data: {
    command: "list_connections",
    data: {
      connections: [
        { provider: "kimi" },
        { provider: "openrouter" },
      ],
      data_revision: "synthetic-memory",
    },
  },
} as const;

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
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(PROTOTYPE_ENVELOPE);
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
    await waitFor(() => {
      expect(screen.getByText("Development scaffold running without provider credentials.")).not
        .toBeNull();
    });
  });

  it("chooses zh-CN on first launch for an explicitly Simplified Chinese system locale", async () => {
    setNavigatorLanguages(["zh-CN"]);
    render(<App />);
    expect((await screen.findByRole("heading", { name: "概览" })).textContent).toContain("概览");
    expect(document.documentElement.lang).toBe("zh-CN");
    // Backend status prose maps to a catalog key: the known scaffold sentence
    // renders in Simplified Chinese, not the English source string.
    expect(
      await screen.findByText("开发脚手架正在运行，未配置任何服务商凭据。"),
    ).not.toBeNull();
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

  it("switching language preserves the current screen, filter, and nonsecret input", () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);

    // Statistics: pick a filter, then switch language from the header control.
    fireEvent.click(screen.getByRole("button", { name: "Statistics" }));
    const range = screen.getByLabelText("Date range") as HTMLSelectElement;
    fireEvent.change(range, { target: { value: "last30" } });
    expect(range.value).toBe("last30");

    fireEvent.change(headerLanguageSelect(), { target: { value: "zh-CN" } });

    // Still on Statistics, filter preserved, labels localized.
    expect(screen.getByRole("heading", { name: "统计" })).not.toBeNull();
    const zhRange = screen.getByLabelText("日期范围") as HTMLSelectElement;
    expect(zhRange.value).toBe("last30");
    expect(screen.getByRole("img", { name: "合成每日 token 消耗柱状图" })).not.toBeNull();
    // The chart description is wired into the accessibility tree via
    // aria-describedby, not just visually rendered.
    const chart = screen.getByRole("img", { name: "合成每日 token 消耗柱状图" });
    const describedBy = chart.getAttribute("aria-describedby");
    expect(describedBy).toBe("statistics-chart-description");
    expect(document.getElementById("statistics-chart-description")!.textContent).toContain(
      "七个合成日合计",
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

  it("renders unknown values as unknown, never as zero", async () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    await screen.findByText("Development scaffold running without provider credentials.");
    expect(screen.getByText("123,456,789,012,345,678")).not.toBeNull();
    expect(screen.getByText("unknown")).not.toBeNull();
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

  it("shows the localized empty-state foundation for a range without data", () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Statistics" }));
    fireEvent.change(screen.getByLabelText("Date range"), {
      target: { value: "none" },
    });
    expect(screen.getByText(/No consumption recorded for this range/i)).not.toBeNull();
  });

  it("re-translates the backend status message after a language switch", async () => {
    setNavigatorLanguages(["en-US"]);
    render(<App />);
    expect(
      await screen.findByText("Development scaffold running without provider credentials."),
    ).not.toBeNull();
    fireEvent.change(headerLanguageSelect(), { target: { value: "zh-CN" } });
    expect(screen.getByText("开发脚手架正在运行，未配置任何服务商凭据。")).not.toBeNull();
    expect(
      screen.queryByText("Development scaffold running without provider credentials."),
    ).toBeNull();
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
