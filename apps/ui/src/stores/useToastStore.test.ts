// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useToastStore } from "./useToastStore";

// jsdom env: the store schedules auto-dismiss via window.setTimeout, which
// only happens when `window` exists — fake timers let the TTL be asserted.
describe("useToastStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("push appends a toast with a 5s default ttl and returns its id", () => {
    const id = useToastStore.getState().push({ level: "info", message: "hi" });
    const toasts = useToastStore.getState().toasts;
    expect(toasts).toHaveLength(1);
    expect(toasts[0].id).toBe(id);
    expect(toasts[0].ttlMs).toBe(5000);
  });

  it("auto-dismisses after the ttl elapses", () => {
    useToastStore.getState().push({ level: "info", message: "bye" });
    vi.advanceTimersByTime(4999);
    expect(useToastStore.getState().toasts).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it("ttlMs 0 means sticky — never auto-dismissed", () => {
    useToastStore.getState().push({ level: "error", message: "stay", ttlMs: 0 });
    vi.advanceTimersByTime(60_000);
    expect(useToastStore.getState().toasts).toHaveLength(1);
  });

  it("push honors an explicit title and action", () => {
    const run = vi.fn();
    useToastStore.getState().push({
      level: "error",
      title: "Boom",
      message: "it broke",
      ttlMs: 0,
      action: { label: "Undo", run },
    });
    const t = useToastStore.getState().toasts[0];
    expect(t.title).toBe("Boom");
    expect(t.action?.label).toBe("Undo");
  });

  it("ids are unique across pushes", () => {
    const a = useToastStore.getState().push({ level: "info", message: "a" });
    const b = useToastStore.getState().push({ level: "info", message: "b" });
    expect(a).not.toBe(b);
  });

  it("dismiss removes only the targeted toast", () => {
    const a = useToastStore.getState().push({ level: "info", message: "a" });
    const b = useToastStore.getState().push({ level: "warn", message: "b" });
    useToastStore.getState().dismiss(a);
    const toasts = useToastStore.getState().toasts;
    expect(toasts).toHaveLength(1);
    expect(toasts[0].id).toBe(b);
  });
});
