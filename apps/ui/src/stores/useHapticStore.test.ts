import { describe, expect, it } from "vitest";
import type { HapticConfigDto } from "../api/client";
import { useHapticStore } from "./useHapticStore";

describe("useHapticStore", () => {
  it("add prepends new addresses and dedupes repeats", () => {
    useHapticStore.getState().add("/avatar/a");
    useHapticStore.getState().add("/avatar/b");
    useHapticStore.getState().add("/avatar/a");
    expect(useHapticStore.getState().discovered).toEqual(["/avatar/b", "/avatar/a"]);
  });

  it("caps discovered at 200 addresses", () => {
    for (let i = 0; i < 205; i++) {
      useHapticStore.getState().add(`/addr/${i}`);
    }
    const discovered = useHapticStore.getState().discovered;
    expect(discovered).toHaveLength(200);
    expect(discovered[0]).toBe("/addr/204");
  });

  it("clear empties discovered but keeps the draft config", () => {
    const config: HapticConfigDto = { enabled: true, listen_port: 9001, rules: [] };
    useHapticStore.getState().setConfig(config);
    useHapticStore.getState().add("/avatar/a");
    useHapticStore.getState().clear();
    expect(useHapticStore.getState().discovered).toEqual([]);
    expect(useHapticStore.getState().config).toEqual(config);
  });

  it("setConfig stores the draft and flips configLoaded", () => {
    const config: HapticConfigDto = {
      enabled: false,
      listen_port: 9001,
      rules: [
        {
          osc_address: "/avatar/parameters/contact",
          device_mac: "010203040506",
          mode: { kind: "proximity", gain: 1, min_threshold: 0.1 },
        },
      ],
    };
    useHapticStore.getState().setConfig(config);
    expect(useHapticStore.getState().config).toEqual(config);
    expect(useHapticStore.getState().configLoaded).toBe(true);
  });
});
