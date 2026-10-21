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

function envelope(revision: string) {
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

/** The revision the screen currently shows, read from its label/value pair. */
function shownRevision(): string | null {
  const label = screen.getByText("Data revision");
  return label.nextElementSibling?.textContent ?? null;
}

let changeHandler: ((event: { payload: unknown }) => void) | undefined;
let unlisten = vi.fn();

beforeEach(async () => {
  window.localStorage.setItem("quotavista.ui_locale", "en");
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
  changeHandler = undefined;
  unlisten = vi.fn();
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

describe("OverviewScreen", () => {
  it("reloads the view when a commit announces a newer revision", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke)
      .mockResolvedValueOnce(envelope("1"))
      .mockResolvedValueOnce(envelope("2"));
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));

    await announce("2");

    await waitFor(() => expect(shownRevision()).toBe("2"));
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("installs the change listener before the first read", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    const order: string[] = [];
    vi.mocked(listen).mockImplementation((async () => {
      order.push("listen");
      return unlisten;
    }) as never);
    vi.mocked(invoke).mockImplementation((async () => {
      order.push("invoke");
      return envelope("1");
    }) as never);

    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));

    // A commit that lands between the read and the registration would be
    // missed, so the listener has to be in place first.
    expect(order).toEqual(["listen", "invoke"]);
  });


  it("reloads the view when a revision is skipped", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke)
      .mockResolvedValueOnce(envelope("1"))
      .mockResolvedValueOnce(envelope("4"));
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));

    // Revision 3 was skipped, so the view cannot be patched and is reloaded.
    await announce("4");

    await waitFor(() => expect(shownRevision()).toBe("4"));
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("discards a response that is older than the observed revision", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    // The reload answers with a read that predates the announced commit.
    vi.mocked(invoke)
      .mockResolvedValueOnce(envelope("2"))
      .mockResolvedValueOnce(envelope("1"));
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("2"));

    await announce("3");

    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
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
      .mockResolvedValueOnce(envelope("2"));
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
    vi.mocked(invoke).mockResolvedValue(envelope("1"));
    vi.mocked(listen).mockRejectedValue(new Error("no event bridge"));
    renderScreen();
    await waitFor(() => expect(shownRevision()).toBe("1"));
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("disposes the change listener when the screen unmounts", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    vi.mocked(invoke).mockResolvedValue(envelope("1"));
    vi.mocked(listen).mockImplementation((async (_event: string, handler: unknown) => {
      changeHandler = handler as (event: { payload: unknown }) => void;
      return unlisten;
    }) as never);
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
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
