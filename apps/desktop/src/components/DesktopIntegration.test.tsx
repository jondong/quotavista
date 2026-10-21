import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import DesktopIntegration from "./DesktopIntegration";
import { I18nProvider } from "../i18n";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// The prototype's desktop region talks to Rust only through `dispatch_command`,
// so every fixture here is a sealed envelope rather than a component prop. That
// keeps the test honest about the wire shape the renderer really receives.
function okEnvelope(command: string, data: unknown) {
  return {
    status: "ok",
    contract_version: 1,
    request_id: "fixture-request",
    data: { command, data },
  } as const;
}

function errorEnvelope(code: string) {
  return {
    status: "error",
    contract_version: 1,
    request_id: "fixture-request",
    error: { code, retryable: false, retry_at: null, message: "synthetic failure" },
  } as const;
}

const CAPABILITIES = { tray: "available", notification: "available" } as const;
const NO_TRAY_CAPABILITIES = { tray: "unavailable", notification: "unsupported" } as const;

function notice(capability: string, delivered: boolean, locale: string) {
  return {
    template_key: "synthetic_probe",
    locale,
    capability,
    delivered,
    title: locale === "zh-CN" ? "QuotaVista 测试通知" : "QuotaVista test notification",
    body:
      locale === "zh-CN"
        ? "来自 QuotaVista 原型程序的合成消息，不包含任何账号数据。"
        : "Synthetic message from the QuotaVista prototype. It carries no account data.",
  } as const;
}

/** Route `dispatch_command` by the requested command name. */
async function mockDispatch(routes: Record<string, unknown>) {
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockImplementation((command: string, args?: unknown) => {
    expect(command).toBe("dispatch_command");
    const request = (args as { request: { command: { command: string } } }).request;
    const response = routes[request.command.command];
    if (response === undefined) {
      return Promise.reject(new Error(`unexpected command ${request.command.command}`));
    }
    return Promise.resolve(response);
  });
}

function renderRegion() {
  return render(
    <I18nProvider>
      <DesktopIntegration />
    </I18nProvider>,
  );
}

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

beforeEach(async () => {
  window.localStorage.clear();
  // A previous test may have switched the reported system language; jsdom
  // keeps that override on `window.navigator`, so restore the default here.
  setNavigatorLanguages(["en"]);
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("desktop integration region", () => {
  it("reads nothing until the user asks, then reports both capabilities", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    await mockDispatch({
      get_platform_capabilities: okEnvelope("get_platform_capabilities", CAPABILITIES),
    });
    renderRegion();

    // Mounting the region is not an IPC call: opening the application never
    // contacts the composition by itself.
    expect(vi.mocked(invoke)).not.toHaveBeenCalled();
    expect(screen.getByText("System tray")).toBeDefined();
    expect(screen.getAllByText("Not checked yet").length).toBe(2);

    fireEvent.click(screen.getByRole("button", { name: "Check desktop capabilities" }));

    await waitFor(() => {
      expect(screen.getByText(/keeps QuotaVista in the tray/)).toBeDefined();
      expect(screen.getByText(/^Available$/)).toBeDefined();
    });
  });

  it("explains that closing the window exits when no tray was detected", async () => {
    await mockDispatch({
      get_platform_capabilities: okEnvelope("get_platform_capabilities", NO_TRAY_CAPABILITIES),
    });
    renderRegion();

    fireEvent.click(screen.getByRole("button", { name: "Check desktop capabilities" }));

    await waitFor(() => {
      expect(
        screen.getByText(/exits QuotaVista instead of leaving a hidden collector/),
      ).toBeDefined();
      expect(screen.getByText("Not supported on this platform")).toBeDefined();
    });
  });

  it("keeps an unread capability as not checked instead of claiming a value", async () => {
    await mockDispatch({ get_platform_capabilities: errorEnvelope("internal_error") });
    renderRegion();

    fireEvent.click(screen.getByRole("button", { name: "Check desktop capabilities" }));

    await waitFor(() => {
      expect(screen.getByText("Desktop capabilities could not be read.")).toBeDefined();
    });
    // A failed read leaves both rows unclaimed rather than reporting a value the
    // platform never gave.
    expect(screen.getAllByText("Not checked yet").length).toBe(2);
    // The region stays usable: the user can still try the explicit action.
    const action = screen.getByRole("button", { name: "Send a test notification" });
    expect((action as HTMLButtonElement).disabled).toBe(false);
  });

  it("sends the synthetic notification in the active locale and reports acceptance", async () => {
    await mockDispatch({
      get_platform_capabilities: okEnvelope("get_platform_capabilities", CAPABILITIES),
      preview_system_notification: okEnvelope("preview_system_notification", notice("available", true, "en")),
    });
    renderRegion();

    fireEvent.click(await screen.findByRole("button", { name: "Send a test notification" }));

    await waitFor(() => {
      expect(screen.getByText("The platform accepted the notification.")).toBeDefined();
    });
    expect(screen.getByText("QuotaVista test notification")).toBeDefined();
    expect(
      screen.getByText(/Synthetic message from the QuotaVista prototype/),
    ).toBeDefined();
  });

  it("asks for the notification in the locale the user selected", async () => {
    setNavigatorLanguages(["zh-CN"]);
    const { invoke } = await import("@tauri-apps/api/core");
    await mockDispatch({
      get_platform_capabilities: okEnvelope("get_platform_capabilities", CAPABILITIES),
      preview_system_notification: okEnvelope(
        "preview_system_notification",
        notice("available", true, "zh-CN"),
      ),
    });
    renderRegion();

    fireEvent.click(await screen.findByRole("button", { name: "发送测试通知" }));

    await waitFor(() => {
      expect(screen.getByText("平台已接受该通知。")).toBeDefined();
    });
    expect(screen.getByText("QuotaVista 测试通知")).toBeDefined();
    const sent = vi.mocked(invoke).mock.calls.map(
      (call) => (call[1] as { request: { command: unknown } }).request.command,
    );
    expect(sent).toContainEqual({
      command: "preview_system_notification",
      args: { template_key: "synthetic_probe", locale: "zh-CN" },
    });
  });

  it("keeps an in-app explanation when delivery is unavailable", async () => {
    await mockDispatch({
      get_platform_capabilities: okEnvelope(
        "get_platform_capabilities",
        NO_TRAY_CAPABILITIES,
      ),
      preview_system_notification: okEnvelope(
        "preview_system_notification",
        notice("unsupported", false, "en"),
      ),
    });
    renderRegion();

    fireEvent.click(await screen.findByRole("button", { name: "Send a test notification" }));

    await waitFor(() => {
      expect(
        screen.getByText("Delivery is unavailable, so the message stays here instead."),
      ).toBeDefined();
    });
    // The text the OS would have shown is still visible in the application, so
    // the user is never left wondering what was sent.
    expect(screen.getByText("QuotaVista test notification")).toBeDefined();
  });

  it("explains a failed transport without pretending the notification went out", async () => {
    await mockDispatch({
      get_platform_capabilities: okEnvelope("get_platform_capabilities", CAPABILITIES),
      preview_system_notification: errorEnvelope("internal_error"),
    });
    renderRegion();

    fireEvent.click(await screen.findByRole("button", { name: "Send a test notification" }));

    await waitFor(() => {
      expect(screen.getByText("The test notification could not be sent.")).toBeDefined();
    });
    expect(screen.queryByText("The platform accepted the notification.")).toBeNull();
  });
});
