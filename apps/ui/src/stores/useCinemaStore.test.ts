import { describe, expect, it } from "vitest";
import { useCinemaStore } from "./useCinemaStore";

describe("useCinemaStore", () => {
  it("toggle flips open", () => {
    useCinemaStore.getState().close();
    useCinemaStore.getState().toggle();
    expect(useCinemaStore.getState().open).toBe(true);
    useCinemaStore.getState().toggle();
    expect(useCinemaStore.getState().open).toBe(false);
  });

  it("close forces open=false even when already closed", () => {
    useCinemaStore.getState().close();
    expect(useCinemaStore.getState().open).toBe(false);
    useCinemaStore.getState().toggle();
    useCinemaStore.getState().close();
    expect(useCinemaStore.getState().open).toBe(false);
  });
});
