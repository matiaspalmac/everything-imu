import { describe, expect, it } from "vitest";
import type { ImuSampleUpdate } from "../api/client";
import { useImuStreamStore } from "./useImuStreamStore";

function update(elapsedMs: number): ImuSampleUpdate {
  return {
    samples: [
      {
        mac: [1, 2, 3, 4, 5, 6],
        gyr_xyz: [0.1, 0.2, 0.3],
        acc_xyz: [0, 0, 9.81],
        mag_xyz: null,
        elapsed_ms: elapsedMs,
      },
    ],
  };
}

describe("useImuStreamStore", () => {
  it("ingest appends per-axis history keyed by mac", () => {
    useImuStreamStore.getState().ingest(update(10));
    useImuStreamStore.getState().ingest(update(20));
    const h = useImuStreamStore.getState().perMac["010203040506"];
    expect(h.gyr[0]).toEqual([0.1, 0.1]);
    expect(h.acc[2]).toEqual([9.81, 9.81]);
    expect(h.ts).toEqual([10, 20]);
  });

  it("caps each series at 240 samples", () => {
    for (let i = 0; i < 250; i++) {
      useImuStreamStore.getState().ingest(update(i));
    }
    const h = useImuStreamStore.getState().perMac["010203040506"];
    expect(h.ts).toHaveLength(240);
    expect(h.ts[0]).toBe(10);
    expect(h.ts[239]).toBe(249);
    expect(h.gyr[1]).toHaveLength(240);
  });

  it("clear drops all histories", () => {
    useImuStreamStore.getState().ingest(update(1));
    useImuStreamStore.getState().clear();
    expect(useImuStreamStore.getState().perMac).toEqual({});
  });
});
