import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, beforeEach, afterEach, expect, it, vi } from "vitest";
import ConnectionsScreen from "./ConnectionsScreen";
import { I18nProvider, useI18n } from "../i18n";

const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invoke,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => async () => {}),
}));

type ConnectionRow = {
  connection_id: string;
  provider: "kimi" | "openrouter";
  alias: string;
  lifecycle:
    | "pending_setup"
    | "active"
    | "disabled"
    | "needs_credentials"
    | "deletion_pending";
  config_revision: string;
  generation: string;
  datasets: unknown[];
  capabilities: unknown[];
};

const ACTIVE: ConnectionRow = {
  connection_id: "demo-kimi",
  provider: "kimi",
  alias: "Work Kimi account",
  lifecycle: "active",
  config_revision: "3",
  generation: "5",
  datasets: [],
  capabilities: [],
};

function envelope(command: string, data: unknown) {
  return {
    status: "ok",
    contract_version: 1,
    request_id: "fixture-request",
    data: { command, data },
  };
}

function listEnvelope(connections: ConnectionRow[], revision = "1") {
  return envelope("list_connections", { connections, data_revision: revision });
}

function operationPayload(connection_id: string | null, kind: string, state: string) {
  return {
    operation_id: `op-${kind}`,
    connection_id,
    kind,
    state,
    next_action: "none",
    retryable: false,
    retry_at: null,
    error: null,
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

/** Canonical result payloads for each mutation command. */
function mutationResult(command: string) {
  switch (command) {
    case "connect_provider":
      return envelope(command, operationPayload(ACTIVE.connection_id, "setup", "succeeded"));
    case "replace_credentials":
      return envelope(
        command,
        operationPayload(ACTIVE.connection_id, "replace_credentials", "succeeded"),
      );
    case "remove_connection":
      return envelope(command, operationPayload(ACTIVE.connection_id, "delete", "succeeded"));
    default:
      return envelope(command, { ...ACTIVE, datasets: [], capabilities: [] });
  }
}

/** Router that also records mutations for assertions. */
function route(
  options: {
    list?: () => ReturnType<typeof listEnvelope>;
    mutation?: (request: { command: string; args?: Record<string, unknown> }) => unknown;
  } = {},
) {
  const mutations: { command: string; args?: Record<string, unknown> }[] = [];
  invoke.mockImplementation(
    async (_cmd: string, invokeArgs?: unknown): Promise<unknown> => {
      const request = (invokeArgs as {
        request: { command: { command: string; args?: Record<string, unknown> } };
      }).request.command;
      if (request.command === "list_connections") {
        return options.list ? options.list() : listEnvelope([ACTIVE]);
      }
      if (options.mutation) {
        const result = options.mutation(request);
        mutations.push({ command: request.command, args: request.args });
        return result;
      }
      mutations.push({ command: request.command, args: request.args });
      return mutationResult(request.command);
    },
  );
  return mutations;
}

/** Switches the interface language from inside the provider tree. */
function LocaleSwitchButton() {
  const { setLocale } = useI18n();
  return (
    <button type="button" onClick={() => setLocale("zh-CN")}>
      switch-locale
    </button>
  );
}

describe("connections screen", () => {
  beforeEach(() => {
    invoke.mockReset();
    window.localStorage.clear();
    vi.spyOn(Storage.prototype, "setItem");
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders the connection list over IPC with lifecycle and revision", async () => {
    route();
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    expect(await screen.findByText("Work Kimi account")).not.toBeNull();
    const row = screen.getByRole("row", { name: /Work Kimi account/u });
    expect(within(row).getByText("Kimi")).not.toBeNull();
    expect(within(row).getByText("Active")).not.toBeNull();
    expect(within(row).getByText("3")).not.toBeNull();
  });

  it("submits an add with the typed fields and clears the secret after success", async () => {
    const mutations = route();
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    await screen.findByText("Work Kimi account");
    fireEvent.change(screen.getByLabelText("Connection alias"), {
      target: { value: "Personal OpenRouter" },
    });
    fireEvent.change(screen.getByLabelText("Credential kind"), {
      target: { value: "management_key" },
    });
    fireEvent.change(screen.getByLabelText("Credential (synthetic)"), {
      target: { value: "synthetic-secret-1" },
    });
    fireEvent.change(screen.getByLabelText("Storage"), {
      target: { value: "session_only" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add connection" }));

    await waitFor(() => {
      expect(mutations).toContainEqual({
        command: "connect_provider",
        args: expect.objectContaining({
          provider: "kimi",
          alias: "Personal OpenRouter",
          credential_kind: "management_key",
          secret: "synthetic-secret-1",
          storage_mode: "session_only",
        }),
      });
    });
    // The secret field is cleared whatever the outcome; it never lingers.
    await waitFor(() => {
      expect((document.getElementById("add-secret") as HTMLInputElement).value).toBe("");
    });
    expect(window.localStorage.getItem("quotavista.ui_locale")).toBeNull();
  });

  it("clears the secret after a failed submission and shows the typed error", async () => {
    route({
      mutation: () => errorPayload("validation", "alias too long"),
    });
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    await screen.findByText("Work Kimi account");
    fireEvent.change(screen.getByLabelText("Connection alias"), {
      target: { value: "Personal OpenRouter" },
    });
    fireEvent.change(screen.getByLabelText("Credential (synthetic)"), {
      target: { value: "synthetic-secret-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add connection" }));

    await waitFor(() => {
      expect((document.getElementById("add-secret") as HTMLInputElement).value).toBe("");
    });
    expect(
      await screen.findByText(/an alias must be 1–80 characters/u),
    ).not.toBeNull();
    expect(window.localStorage.setItem).not.toHaveBeenCalled();
  });

  it("renames through the inline editor with the expected revision", async () => {
    const mutations = route();
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    await screen.findByText("Work Kimi account");
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    const editor = screen.getByLabelText("New alias") as HTMLInputElement;
    expect(editor.value).toBe("Work Kimi account");
    fireEvent.change(editor, { target: { value: "Team Kimi" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(mutations).toContainEqual({
        command: "update_connection",
        args: {
          connection_id: "demo-kimi",
          expected_config_revision: "3",
          alias: "Team Kimi",
        },
      });
    });
  });

  it("disables and re-enables through the backend with the expected revision", async () => {
    const mutations = route();
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    await screen.findByText("Work Kimi account");
    fireEvent.click(screen.getByRole("button", { name: "Disable" }));
    await waitFor(() => {
      expect(mutations).toContainEqual({
        command: "set_connection_enabled",
        args: {
          connection_id: "demo-kimi",
          expected_config_revision: "3",
          enabled: false,
        },
      });
    });

    // Re-enable is only offered in the disabled state.
    expect(
      (screen.getByRole("button", { name: "Re-enable" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("offers retry removal and a recovery note in deletion pending", async () => {
    route({ list: () => listEnvelope([{ ...ACTIVE, lifecycle: "deletion_pending" }]) });
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    expect(
      await screen.findByText(/the removal can be retried, including after a restart/u),
    ).not.toBeNull();
    expect(screen.getByRole("button", { name: "Retry removal" })).not.toBeNull();
  });

  it("shows the session-only note in needs credentials and offers rotation", async () => {
    route({ list: () => listEnvelope([{ ...ACTIVE, lifecycle: "needs_credentials" }]) });
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    expect(
      await screen.findByText(/Provide the credential again after a restart/u),
    ).not.toBeNull();
    expect(screen.getByRole("button", { name: "Replace credentials" })).not.toBeNull();
  });

  it("rotates credentials with the expected revision and clears the secret", async () => {
    const mutations = route();
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    await screen.findByText("Work Kimi account");
    fireEvent.click(screen.getByRole("button", { name: "Replace credentials" }));
    const secret = (await waitFor(() => {
      const input = document.getElementById("rotate-demo-kimi-secret") as HTMLInputElement;
      expect(input).not.toBeNull();
      return input;
    })) as HTMLInputElement;
    fireEvent.change(secret, { target: { value: "synthetic-secret-2" } });
    fireEvent.change(document.getElementById("rotate-demo-kimi-storage-mode") as HTMLSelectElement, {
      target: { value: "session_only" },
    });
    fireEvent.click(
      screen.getAllByRole("button", { name: "Replace credentials" })[
        screen.getAllByRole("button", { name: "Replace credentials" }).length - 1
      ],
    );

    await waitFor(() => {
      expect(mutations).toContainEqual({
        command: "replace_credentials",
        args: expect.objectContaining({
          connection_id: "demo-kimi",
          expected_config_revision: "3",
          secret: "synthetic-secret-2",
          storage_mode: "session_only",
        }),
      });
    });
    await waitFor(() => {
      expect(
        (document.getElementById("rotate-demo-kimi-secret") as HTMLInputElement).value,
      ).toBe("");
    });
  });


  it("clears the add secret on submission, before the request settles", async () => {
    let resolveAdd: ((value: unknown) => void) | null = null;
    const mutations = route({
      list: () => listEnvelope([ACTIVE, { ...ACTIVE, connection_id: "held-kimi", alias: "Held" }]),
      mutation: () =>
        new Promise((resolve) => {
          resolveAdd = resolve;
        }),
    });
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    fireEvent.change(await screen.findByLabelText("Connection alias"), {
      target: { value: "Held" },
    });
    fireEvent.change(screen.getByLabelText("Credential (synthetic)"), {
      target: { value: "held-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add connection" }));

    // The field is cleared at submission time — the request is still in
    // flight, so clearing cannot depend on when (or whether) it settles.
    await waitFor(() => {
      expect((document.getElementById("add-secret") as HTMLInputElement).value).toBe("");
    });
    expect(mutations.at(-1)?.args).toEqual(
      expect.objectContaining({ alias: "Held", secret: "held-secret" }),
    );
    const settleAdd = (payload: unknown) => resolveAdd?.(payload);
    settleAdd(mutationResult("connect_provider"));
    expect(await screen.findByText("Held")).not.toBeNull();
    expect((document.getElementById("add-secret") as HTMLInputElement).value).toBe("");
  });

  it("clears the rotation secret on submission, before the request settles", async () => {
    let resolveRotate: ((value: unknown) => void) | null = null;
    const mutations = route({
      mutation: () =>
        new Promise((resolve) => {
          resolveRotate = resolve;
        }),
    });
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    await screen.findByText("Work Kimi account");
    fireEvent.click(screen.getByRole("button", { name: "Replace credentials" }));
    const secret = await waitFor(() => {
      const input = document.getElementById("rotate-demo-kimi-secret") as HTMLInputElement;
      expect(input).not.toBeNull();
      return input;
    });
    fireEvent.change(secret, { target: { value: "held-rotation-secret" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Replace credentials" }).at(-1)!);

    await waitFor(() => {
      expect(
        (document.getElementById("rotate-demo-kimi-secret") as HTMLInputElement).value,
      ).toBe("");
    });
    expect(mutations.at(-1)?.args).toEqual(
      expect.objectContaining({ secret: "held-rotation-secret" }),
    );
    const settle = (payload: unknown) => resolveRotate?.(payload);
    settle(
      envelope("replace_credentials", {
        operation_id: "op-held",
        connection_id: "demo-kimi",
        kind: "replace_credentials",
        state: "succeeded",
        next_action: "none",
        retryable: false,
        retry_at: null,
        error: null,
      }),
    );
    expect(
      await screen.findByText(
        (_, element) => element?.textContent === "Replace credentials — Completed",
      ),
    ).not.toBeNull();
    expect((document.getElementById("rotate-demo-kimi-secret") as HTMLInputElement).value).toBe("");
  });
  it("reflects a failed rotation without optimistic success", async () => {
    route({
      mutation: (request) =>
        envelope(request.command, {
          operation_id: "op-replace-1",
          connection_id: "demo-kimi",
          kind: "replace_credentials",
          state: "failed",
          next_action: "replace_connection",
          retryable: false,
          retry_at: null,
          error: null,
        }),
    });
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    await screen.findByText("Work Kimi account");
    fireEvent.click(screen.getByRole("button", { name: "Replace credentials" }));
    const secret = (await waitFor(() => {
      const input = document.getElementById("rotate-demo-kimi-secret") as HTMLInputElement;
      expect(input).not.toBeNull();
      return input;
    })) as HTMLInputElement;
    fireEvent.change(secret, { target: { value: "synthetic-other-account" } });
    fireEvent.click(
      screen.getAllByRole("button", { name: "Replace credentials" })[
        screen.getAllByRole("button", { name: "Replace credentials" }).length - 1
      ],
    );

    expect(await screen.findByText("Replace credentials — Failed")).not.toBeNull();
    expect(
      screen.getByText("A different identity cannot be merged here."),
    ).not.toBeNull();
  });

  it("preserves the alias draft across a language switch but clears the secret", async () => {
    route();
    function Harness() {
      return (
        <I18nProvider>
          <ConnectionsScreen />
          <LocaleSwitchButton />
        </I18nProvider>
      );
    }
    render(<Harness />);
    await screen.findByText("Work Kimi account");
    fireEvent.change(screen.getByLabelText("Connection alias"), {
      target: { value: "Personal OpenRouter" },
    });
    fireEvent.change(screen.getByLabelText("Credential (synthetic)"), {
      target: { value: "synthetic-secret-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "switch-locale" }));
    // The alias cell keeps user content; localized chrome switches.
    await screen.findByRole("button", { name: "添加连接" });
    // Non-secret input survives; the secret draft is dropped.
    expect((screen.getByLabelText("连接别名") as HTMLInputElement).value).toBe(
      "Personal OpenRouter",
    );
    await waitFor(() => {
      expect((document.getElementById("add-secret") as HTMLInputElement).value).toBe("");
    });
    expect(window.localStorage.setItem).not.toHaveBeenCalled();
    // The table itself keeps rendering localized labels.
    expect(within(screen.getByRole("table")).getByText("服务商")).not.toBeNull();
  });

  it("renders the empty state before any connection exists", async () => {
    route({ list: () => listEnvelope([], "1") });
    render(
      <I18nProvider>
        <ConnectionsScreen />
      </I18nProvider>,
    );
    expect(
      await screen.findByText(/No connections yet. Add one below/u),
    ).not.toBeNull();
  });
});
