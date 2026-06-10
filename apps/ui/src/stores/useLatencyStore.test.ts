import { describe, expect, it } from "vitest";
import type { LatencyEntry } from "../api/client";
import { useLatencyStore } from "./useLatencyStore";

function entry(mac: [number, number, number, number, number, number], p50: number): LatencyEntry {
  return {
    mac,
    interval_us_p50: p50,
    interval_us_p95: p50 * 2,
    interval_us_p99: p50 * 3,
    jitter_us: 100,
    send_us_p50: 50,
    send_us_p95: 90,
    dropped_estimate: 0,
    samples_window: 256,
  };
}

describe("useLatencyStore", () => {
  it("ingest keys entries by mac and stamps lastUpdateMs", () => {
    const before = Date.now();
    useLatencyStore.getState().ingest({ entries: [entry([1, 2, 3, 4, 5, 6], 5000)] });
    const s = useLatencyStore.getState();
    expect(s.perMac["010203040506"].interval_us_p50).toBe(5000);
    expect(s.lastUpdateMs).toBeGreaterThanOrEqual(before);
  });

  it("ingest merges new macs and overwrites repeats", () => {
    useLatencyStore.getState().ingest({ entries: [entry([1, 2, 3, 4, 5, 6], 5000)] });
    useLatencyStore.getState().ingest({
      entries: [entry([1, 2, 3, 4, 5, 6], 7000), entry([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff], 1000)],
    });
    const perMac = useLatencyStore.getState().perMac;
    expect(perMac["010203040506"].interval_us_p50).toBe(7000);
    expect(perMac.aabbccddeeff.interval_us_p50).toBe(1000);
  });
});
