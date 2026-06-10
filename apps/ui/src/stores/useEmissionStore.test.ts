import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api/client";
import { useEmissionStore } from "./useEmissionStore";

vi.mock("../api/client", () => ({
  api: {
    getEmissionPaused: vi.fn(),
    setEmissionPaused: vi.fn(),
  },
}));

const mocked = vi.mocked(api);

describe("useEmissionStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("hydrate pulls the backend flag and marks hydrated", async () => {
    mocked.getEmissionPaused.mockResolvedValue({ status: "ok", data: true });
    await useEmissionStore.getState().hydrate();
    expect(useEmissionStore.getState().paused).toBe(true);
    expect(useEmissionStore.getState().hydrated).toBe(true);
  });

  it("hydrate leaves state untouched on backend error", async () => {
    mocked.getEmissionPaused.mockResolvedValue({
      status: "error",
      error: "nope",
    } as never);
    await useEmissionStore.getState().hydrate();
    expect(useEmissionStore.getState().paused).toBe(false);
    expect(useEmissionStore.getState().hydrated).toBe(false);
  });

  it("toggle applies optimistically and persists", async () => {
    mocked.setEmissionPaused.mockResolvedValue({ status: "ok", data: null });
    const result = await useEmissionStore.getState().toggle();
    expect(result).toBe(true);
    expect(useEmissionStore.getState().paused).toBe(true);
    expect(mocked.setEmissionPaused).toHaveBeenCalledWith(true);
  });

  it("toggle rolls back when the backend rejects", async () => {
    mocked.setEmissionPaused.mockResolvedValue({
      status: "error",
      error: "nope",
    } as never);
    const result = await useEmissionStore.getState().toggle();
    expect(result).toBe(false);
    expect(useEmissionStore.getState().paused).toBe(false);
  });

  it("set rolls back when the backend rejects", async () => {
    mocked.setEmissionPaused.mockResolvedValue({
      status: "error",
      error: "nope",
    } as never);
    await useEmissionStore.getState().set(true);
    expect(useEmissionStore.getState().paused).toBe(false);
  });
});
