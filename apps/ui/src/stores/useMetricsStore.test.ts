import { describe, expect, it } from "vitest";
import type { TrackerSnapshot } from "../api/client";
import { useMetricsStore } from "./useMetricsStore";

function snap(
  mac: [number, number, number, number, number, number],
  rate: number,
): TrackerSnapshot {
  return {
    mac,
    serial: "S",
    quat_xyzw: [0, 0, 0, 1],
    battery_fraction: 1,
    rate_hz: rate,
  };
}

describe("useMetricsStore", () => {
  it("observe accumulates update count and average packet rate", () => {
    useMetricsStore
      .getState()
      .observe([snap([1, 2, 3, 4, 5, 6], 60), snap([9, 9, 9, 9, 9, 9], 120)]);
    const s = useMetricsStore.getState();
    expect(s.totalUpdates).toBe(1);
    expect(s.totalPackets).toBe(90); // (60 + 120) / 2 trackers
    expect(s.perMacHist["010203040506"].rates).toEqual([60]);
    expect(s.perMacHist["090909090909"].rates).toEqual([120]);
  });

  it("caps per-mac rate history at 60 samples", () => {
    for (let i = 0; i < 65; i++) {
      useMetricsStore.getState().observe([snap([1, 2, 3, 4, 5, 6], i)]);
    }
    const rates = useMetricsStore.getState().perMacHist["010203040506"].rates;
    expect(rates).toHaveLength(60);
    expect(rates[0]).toBe(5);
    expect(rates[59]).toBe(64);
  });

  it("reset zeroes counters and clears history", () => {
    useMetricsStore.getState().observe([snap([1, 2, 3, 4, 5, 6], 60)]);
    useMetricsStore.getState().reset();
    const s = useMetricsStore.getState();
    expect(s.totalUpdates).toBe(0);
    expect(s.totalPackets).toBe(0);
    expect(s.perMacHist).toEqual({});
  });
});
