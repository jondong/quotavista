import { describe, expect, it, vi } from "vitest";
import {
  RevisionTracker,
  isObsoleteResponse,
  isRevisionGap,
  subscribeToChanges,
  type ChangeEvent,
} from "./ipc";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

function changeEvent(revision: string): ChangeEvent {
  return {
    data_revision: revision,
    affected_connection_ids: [],
    affected_scope_ids: [],
  };
}

describe("RevisionTracker", () => {
  it("never lowers its watermark when an older event arrives", () => {
    const tracker = new RevisionTracker("5");
    const update = tracker.apply(changeEvent("3"));
    expect(update.kind).toBe("stale");
    expect(tracker.observed()).toBe("5");
  });

  it("ignores an event that repeats the observed revision", () => {
    const tracker = new RevisionTracker("5");
    expect(tracker.apply(changeEvent("5")).kind).toBe("stale");
    expect(tracker.observed()).toBe("5");
  });

  it("reports a gap when a revision is skipped", () => {
    const tracker = new RevisionTracker("3");
    const update = tracker.apply(changeEvent("5"));
    expect(update).toEqual({ kind: "gap", gap: true });
    expect(tracker.observed()).toBe("5");
  });

  it("applies the next revision", () => {
    const tracker = new RevisionTracker("3");
    expect(tracker.apply(changeEvent("4"))).toEqual({ kind: "apply", gap: false });
    expect(tracker.observed()).toBe("4");
  });

  it("does not notify subscribers about a stale event", () => {
    const tracker = new RevisionTracker("5");
    const handler = vi.fn();
    tracker.subscribe(handler);
    tracker.apply(changeEvent("4"));
    expect(handler).not.toHaveBeenCalled();
  });

  it("notifies subscribers about a new revision", () => {
    const tracker = new RevisionTracker("3");
    const handler = vi.fn();
    tracker.subscribe(handler);
    tracker.apply(changeEvent("4"));
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("refuses to observe a revision older than the watermark", () => {
    const tracker = new RevisionTracker("5");
    expect(tracker.observe("4")).toBe(false);
    expect(tracker.observed()).toBe("5");
    expect(tracker.observe("6")).toBe(true);
    expect(tracker.observed()).toBe("6");
  });

  it("keeps a rendered response from silencing another consumer's event", () => {
    const tracker = new RevisionTracker("0");
    const first = vi.fn();
    const second = vi.fn();
    tracker.subscribe(first);
    tracker.subscribe(second);

    // One consumer renders a response that already carries the revision the
    // event announces.
    expect(tracker.observe("5")).toBe(true);
    const update = tracker.apply(changeEvent("5"));
    expect(update.kind).toBe("stale");
    expect(tracker.observed()).toBe("5");
    // The event still reaches the consumers that have not seen it.
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("does not notify subscribers about a repeated event", () => {
    const tracker = new RevisionTracker("3");
    const handler = vi.fn();
    tracker.subscribe(handler);
    tracker.apply(changeEvent("4"));
    tracker.apply(changeEvent("4"));
    expect(handler).toHaveBeenCalledTimes(1);
  });
});

describe("revision helpers", () => {
  it("treats a response older than the observed revision as obsolete", () => {
    expect(isObsoleteResponse("4", "5")).toBe(true);
    expect(isObsoleteResponse("5", "5")).toBe(false);
    expect(isObsoleteResponse("6", "5")).toBe(false);
  });

  it("treats a skipped revision as a gap", () => {
    expect(isRevisionGap("3", "4")).toBe(false);
    expect(isRevisionGap("3", "5")).toBe(true);
    expect(isRevisionGap("0", "7")).toBe(false);
  });
});

describe("subscribeToChanges", () => {
  it("removes the tracker subscription when the listener cannot be installed", async () => {
    const { listen } = await import("@tauri-apps/api/event");
    vi.mocked(listen).mockRejectedValueOnce(new Error("no event bridge"));
    const tracker = new RevisionTracker("0");
    const handler = vi.fn();
    await expect(subscribeToChanges(tracker, handler)).rejects.toThrow("no event bridge");

    // The failed subscription must not leave a subscriber behind.
    tracker.apply(changeEvent("4"));
    expect(handler).not.toHaveBeenCalled();
  });
});
