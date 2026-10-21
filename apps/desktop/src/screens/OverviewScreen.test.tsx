import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n";
import { revisionTracker } from "../ipc";
import OverviewScreen from "./OverviewScreen";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

function connectionsEnvelope(revision: string) {
  return {
    status: "ok",
    contract_version: 1,
    request_id: "fixture-request",
    data: {
      command: "list_connections",
      data: { connections: [], data_revision: revision },
    },
  };
}

function overviewEnvelope(revision: string) {
  const interval = { start: "2026-10-21T00:00:00Z", end: "2026-10-22T00:00:00Z" };
  return {
    status: "ok",
    contract_version: 1,
    request_id: "fixture-request",
    data: {
      command: "get_overview",
      data: {
        metrics: [],
        summary: {
          requested_interval: interval,
          effective_intervals: [interval],
          reporting_zone: "UTC",
          grain: "day",
          as_of: "2026-10-21T09:00:00Z",
          provisional: true,
          coverage: [],
          excluded_scopes: [],
          totals: [],
          series: [],
          plans: [],
          data_revision: revision,
        },
      },
    },
  };
}

/** Route each invoke call to the envelope matching its request command. */
function routeByCommand(resolver: (command: string) => unknown) {
  return async (_command: string, payload: unknown) =>
    resolver((payload as { request: { command: { command: string } } }).request.command.command);
}

/** The revision the screen currently shows, read from its label/value pair. */
function shownRevision(): string | null {
  const label = screen.getByText("Data revision");
  return label.nextElementSibling?.textContent ?? null;
}

let changeHandler: ((event: { payload: unknown }) => void) | undefined;
let unlisten = vi.fn();
let revision = "1";

beforeEach(async () => {
  window.localStorage.setItem("quotavista.ui_locale", "en");
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
  changeHandler = undefined;
  unlisten = vi.fn();
  revision = "1";
  vi.mocked(invoke).mockImplementation(
    routeByCommand((command) =>
      command === "get_overview" ? overviewEnvelope(revision) : connectionsEnvelope(revision),
    ),
  );
  vi.mocked(listen).mockImplementation((async (_event: string, handler: unknown) => {
    changeHandler = handler as (event: { payload: unknown }) => void;
    return unlisten;
  }) as never);
  revisionTracker.reset("0");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function renderScreen() {
  return render(
    <I18nProvider>
      <OverviewScreen />
    </I18nProvider>,
  );
}

async function announce(next: string): Promise<void> {
  await waitFor(() => expect(changeHandler).toBeDefined());
  await act(async () => {
    changeHandler?.({
      payload: {
        data_revision: next,
        affected_connection_ids: [],
        affected_scope_ids: [],
      },
    });
  });
}

describe("OverviewScreen", () => {
  it("reloads the view when a commit announces a newer revision", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));

    revision = "2";
    await announce("2");

    await waitFor(() => expect(shownRevision()).toBe("2"));
    // Each load reads the connection list and the overview together.
    expect(invoke).toHaveBeenCalledTimes(4);
  });

  it("installs the change listener before the first read", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    const order: string[] = [];
    vi.mocked(listen).mockImplementation((async () => {
      order.push("listen");
      return unlisten;
    }) as never);
    vi.mocked(invoke).mockImplementation((async (_command: string, payload: unknown) => {
      order.push("invoke");
      return routeByCommand((command) =>
        command === "get_overview" ? overviewEnvelope("1") : connectionsEnvelope("1"),
      )(_command, payload);
    }) as never);

    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));

    // A commit that lands between the read and the registration would be
    // missed, so the listener has to be in place first.
    expect(order).toEqual(["listen", "invoke", "invoke"]);
  });

  it("reloads the view when a revision is skipped", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));

    // Revision 3 was skipped, so the view cannot be patched and is reloaded.
    revision = "4";
    await announce("4");

    await waitFor(() => expect(shownRevision()).toBe("4"));
    expect(invoke).toHaveBeenCalledTimes(4);
  });

  it("discards a response that is older than the observed revision", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    revision = "2";
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("2"));

    // The reload answers with a read that predates the announced commit.
    revision = "1";
    await announce("3");

    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(4));
    expect(shownRevision()).toBe("2");
  });

  it("keeps the newer view when an older read fails afterwards", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    let rejectFirst: ((reason: unknown) => void) | undefined;
    const first = new Promise((_resolve, reject) => {
      rejectFirst = reject;
    });
    vi.mocked(invoke)
      .mockReturnValueOnce(first as never)
      .mockImplementation(
        routeByCommand((command) =>
          command === "get_overview" ? overviewEnvelope("2") : connectionsEnvelope("2"),
        ),
      );
    renderScreen();
    await waitFor(() => expect(changeHandler).toBeDefined());

    // The commit starts a second read that answers with the newer state.
    await announce("2");
    await waitFor(() => expect(shownRevision()).toBe("2"));

    // The older read fails afterwards. Its error is obsolete and must not
    // replace the newer view with the error state.
    await act(async () => {
      rejectFirst?.(new Error("the older read failed"));
      await Promise.resolve();
    });
    expect(shownRevision()).toBe("2");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("still shows the last known state when the change channel cannot be installed", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    vi.mocked(listen).mockRejectedValue(new Error("no event bridge"));
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));
    expect(invoke).toHaveBeenCalledTimes(2);
  });


  it("releases the change listener on unmount", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const view = renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));
    await waitFor(() => expect(changeHandler).toBeDefined());

    view.unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalled());

    await act(async () => {
      changeHandler?.({
        payload: {
          data_revision: "2",
          affected_connection_ids: [],
          affected_scope_ids: [],
        },
      });
    });
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});
