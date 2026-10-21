import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SettingsScreen from "./SettingsScreen";
import { I18nProvider } from "../i18n";
import { SettingsProvider } from "../SettingsContext";

const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invoke,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => async () => {}),
}));

type SettingsData = {
  settings_revision: string;
  ui_locale: "en" | "zh-CN";
  reporting_zone: string;
  display_currency: string;
  polling: "disabled" | "one_minute" | "five_minutes" | "fifteen_minutes";
  quota_threshold: string;
  balance_threshold: string;
};

const DEFAULTS: SettingsData = {
  settings_revision: "0",
  ui_locale: "en",
  reporting_zone: "UTC",
  display_currency: "USD",
  polling: "five_minutes",
  quota_threshold: "0.2",
  balance_threshold: "0",
};

function settingsEnvelope(data: Partial<SettingsData> = {}) {
  const merged = { ...DEFAULTS, ...data };
  return {
    status: "ok" as const,
    contract_version: 1,
    request_id: "fixture-request",
    data: { command: "update_settings", data: merged },
  };
}

function errorPayload(code: string, message: string) {
  return {
    status: "error" as const,
    contract_version: 1,
    request_id: "fixture-request",
    error: { code, retryable: false, retry_at: null, message },
  };
}

type UpdateCall = {
  expected_settings_revision: string;
  changes: Record<string, string | null>;
};

function updateCalls(): UpdateCall[] {
  return invoke.mock.calls
    .map((call) => {
      const args = call[1] as {
        request: {
          command: {
            command: string;
            args?: { expected_settings_revision: string; changes: Record<string, string | null> };
          };
        };
      };
      return args.request.command;
    })
    .filter((command) => command.command === "update_settings")
    .map((command) => ({
      expected_settings_revision: command.args?.expected_settings_revision ?? "",
      changes: command.args?.changes ?? {},
    }));
}

describe("settings screen", () => {
  beforeEach(() => {
    invoke.mockReset();
    window.localStorage.clear();
    vi.spyOn(Storage.prototype, "setItem");
    invoke.mockImplementation(async (_cmd: string, invokeArgs?: unknown): Promise<unknown> => {
      const command = (invokeArgs as {
        request: { command: { command: string } };
      }).request.command.command;
      if (command === "update_settings") return settingsEnvelope();
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  function renderScreen() {
    return render(
      <I18nProvider>
        <SettingsProvider>
          <SettingsScreen />
        </SettingsProvider>
      </I18nProvider>,
    );
  }

  it("reads the persisted settings with a canonical empty patch", async () => {
    renderScreen();
    expect(await screen.findByDisplayValue("UTC")).not.toBeNull();
    await waitFor(() => {
      expect(updateCalls()[0]).toEqual({
        expected_settings_revision: "0",
        changes: {
          ui_locale: null,
          reporting_zone: null,
          display_currency: null,
          polling: null,
          quota_threshold: null,
          balance_threshold: null,
        },
      });
    });
    expect(screen.getByDisplayValue("USD")).not.toBeNull();
    expect(screen.getByDisplayValue("0.2")).not.toBeNull();
    expect(screen.getByDisplayValue("0")).not.toBeNull();
    expect((screen.getByLabelText("Automatic refresh") as HTMLSelectElement).value).toBe(
      "five_minutes",
    );
  });

  it("saves only changed fields and reports the saved state", async () => {
    let saved = false;
    invoke.mockImplementation(async (_cmd: string, invokeArgs?: unknown): Promise<unknown> => {
      const command = (invokeArgs as {
        request: { command: { command: string } };
      }).request.command.command;
      if (command !== "update_settings") throw new Error("unexpected");
      if (saved) return settingsEnvelope({ settings_revision: "1" });
      return settingsEnvelope();
    });
    renderScreen();
    await screen.findByDisplayValue("UTC");
    fireEvent.change(screen.getByLabelText("Reporting time zone"), {
      target: { value: "Asia/Shanghai" },
    });
    fireEvent.change(screen.getByLabelText("Automatic refresh"), {
      target: { value: "disabled" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    saved = true;

    expect(await screen.findByText("Settings saved.")).not.toBeNull();
    await waitFor(() => {
      expect(updateCalls().at(-1)?.changes).toEqual({
        ui_locale: null,
        reporting_zone: "Asia/Shanghai",
        display_currency: null,
        polling: "disabled",
        quota_threshold: null,
        balance_threshold: null,
      });
    });
    await waitFor(() => {
      expect(updateCalls().at(-1)?.expected_settings_revision).toBe("0");
    });
  });

  it("surfaces a stale-revision conflict, reloads, and never overwrites the newer choice", async () => {
    let attempts = 0;
    invoke.mockImplementation(async (_cmd: string, invokeArgs?: unknown): Promise<unknown> => {
      const command = (invokeArgs as {
        request: { command: { command: string } };
      }).request.command.command;
      if (command !== "update_settings") throw new Error("unexpected");
      attempts += 1;
      // The first read succeeds; the save hits a revision raced elsewhere;
      // the reload returns the newer persisted values.
      if (attempts === 2) return errorPayload("conflict", "stale revision");
      return settingsEnvelope(
        attempts === 1 ? {} : { settings_revision: "4", reporting_zone: "Europe/Berlin" },
      );
    });
    renderScreen();
    await screen.findByDisplayValue("UTC");
    fireEvent.change(screen.getByLabelText("Reporting time zone"), {
      target: { value: "Asia/Shanghai" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));

    expect(
      await screen.findByText(/The settings changed elsewhere; showing the stored values/u),
    ).not.toBeNull();
    // The drafts were reset to the newer persisted truth, not to the stale
    // local edit.
    expect(
      (screen.getByLabelText("Reporting time zone") as HTMLInputElement).value,
    ).toBe("Europe/Berlin");
  });

  it("surfaces a validation failure without clobbering the drafts", async () => {
    let attempts = 0;
    invoke.mockImplementation(async (_cmd: string, invokeArgs?: unknown): Promise<unknown> => {
      const command = (invokeArgs as {
        request: { command: { command: string } };
      }).request.command.command;
      if (command !== "update_settings") throw new Error("unexpected");
      attempts += 1;
      if (attempts >= 2) return errorPayload("validation", "bad zone");
      return settingsEnvelope();
    });
    renderScreen();
    await screen.findByDisplayValue("UTC");
    fireEvent.change(screen.getByLabelText("Reporting time zone"), {
      target: { value: "Not/AZone" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));

    expect(await screen.findByText(/One or more values were rejected/u)).not.toBeNull();
    // The rejected draft stays visible for correction.
    expect((screen.getByLabelText("Reporting time zone") as HTMLInputElement).value).toBe(
      "Not/AZone",
    );
  });

  it("preserves non-secret form drafts across a language switch", async () => {
    invoke.mockImplementation(async (_cmd: string, invokeArgs?: unknown): Promise<unknown> => {
      const command = (invokeArgs as {
        request: { command: { command: string } };
      }).request.command.command;
      if (command !== "update_settings") throw new Error("unexpected");
      return settingsEnvelope();
    });
    renderScreen();
    await screen.findByDisplayValue("UTC");
    fireEvent.change(screen.getByLabelText("Reporting time zone"), {
      target: { value: "Asia/Shanghai" },
    });
    fireEvent.change(screen.getByLabelText("Interface language"), {
      target: { value: "zh-CN" },
    });

    expect(await screen.findByText(/该选择随持久化设置保存/u)).not.toBeNull();
    // The zone draft (non-secret form state) survives the switch.
    expect((screen.getByLabelText("报表时区") as HTMLInputElement).value).toBe(
      "Asia/Shanghai",
    );
    // The explicit locale choice is persisted through the versioned setting.
    await waitFor(() => {
      const localeWrite = updateCalls().at(-1);
      expect(localeWrite?.changes.ui_locale).toBe("zh-CN");
    });
    expect(window.localStorage.setItem).not.toHaveBeenCalled();
  });

  it("offers refresh Off explicitly alongside the intervals", async () => {
    renderScreen();
    const select = (await screen.findByLabelText(
      "Automatic refresh",
    )) as HTMLSelectElement;
    expect([...select.options].map((option) => option.value)).toEqual([
      "disabled",
      "one_minute",
      "five_minutes",
      "fifteen_minutes",
    ]);
    expect(screen.getByText("Off (manual refresh only)")).not.toBeNull();
  });

  it("renders the pending managed-backup note explicitly", async () => {
    renderScreen();
    expect(await screen.findByDisplayValue("UTC")).not.toBeNull();
    expect(
      screen.getByText(/Managed-backup deletion guarantees remain explicitly pending/u),
    ).not.toBeNull();
  });

  it("shows the read error with a retry when the settings cannot be loaded", async () => {
    let failed = true;
    invoke.mockImplementation(async (_cmd: string, invokeArgs?: unknown): Promise<unknown> => {
      const command = (invokeArgs as {
        request: { command: { command: string } };
      }).request.command.command;
      if (command !== "update_settings") throw new Error("unexpected");
      if (failed) throw new Error("backend unavailable");
      return settingsEnvelope();
    });
    renderScreen();
    expect(await screen.findByText(/Settings could not be read from the backend/u)).not.toBeNull();
    failed = false;
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByDisplayValue("UTC")).not.toBeNull();
  });
});
