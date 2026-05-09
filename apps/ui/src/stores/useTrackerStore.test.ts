import { describe, expect, it } from "vitest";
import type { TrackerSnapshot } from "../api/client";
import { useTrackerStore } from "./useTrackerStore";

describe("useTrackerStore", () => {
  it("apply replaces by mac key", () => {
    const t1: TrackerSnapshot = {
      mac: [1, 2, 3, 4, 5, 6],
      serial: "A",
      quat_xyzw: [0, 0, 0, 1],
      battery_fraction: 0.5,
      rate_hz: 60,
    };
    useTrackerStore.getState().apply({ trackers: [t1] });
    expect(Object.keys(useTrackerStore.getState().trackers)).toEqual(["010203040506"]);
  });

  it("clear empties", () => {
    useTrackerStore.getState().clear();
    expect(useTrackerStore.getState().trackers).toEqual({});
  });
});
