import { describe, expect, it } from "vitest";
import type { BiasUpdate } from "../api/client";
import { useBiasStore } from "./useBiasStore";

describe("useBiasStore", () => {
  it("ingest keys entries by mac and stores the gyro bias triplet", () => {
    const update: BiasUpdate = {
      entries: [{ mac: [1, 2, 3, 4, 5, 6], gyr_bias: [0.1, -0.2, 0.3] }],
    };
    useBiasStore.getState().ingest(update);
    expect(useBiasStore.getState().perMac["010203040506"]).toEqual([0.1, -0.2, 0.3]);
  });

  it("ingest merges across updates and overwrites per mac", () => {
    useBiasStore.getState().ingest({
      entries: [{ mac: [1, 2, 3, 4, 5, 6], gyr_bias: [9, 9, 9] }],
    });
    useBiasStore.getState().ingest({
      entries: [{ mac: [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff], gyr_bias: [1, 2, 3] }],
    });
    const perMac = useBiasStore.getState().perMac;
    expect(perMac["010203040506"]).toEqual([9, 9, 9]);
    expect(perMac.aabbccddeeff).toEqual([1, 2, 3]);
  });
});
