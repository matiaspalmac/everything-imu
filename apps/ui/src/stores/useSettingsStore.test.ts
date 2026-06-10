import { describe, expect, it } from "vitest";
import type { SettingsDto } from "../api/client";
import { useSettingsStore } from "./useSettingsStore";

describe("useSettingsStore", () => {
  it("starts with sane defaults", () => {
    const s = useSettingsStore.getState().settings;
    expect(s.slime_server_addr).toBe("127.0.0.1:6969");
    expect(s.close_to_tray).toBe(true);
  });

  it("set merges a partial patch without touching other fields", () => {
    useSettingsStore.getState().set({ log_filter: "debug" });
    const s = useSettingsStore.getState().settings;
    expect(s.log_filter).toBe("debug");
    expect(s.slime_server_addr).toBe("127.0.0.1:6969");
  });

  it("replace swaps the whole settings object", () => {
    const next: SettingsDto = {
      ...useSettingsStore.getState().settings,
      slime_server_addr: "192.168.1.50:6969",
      theme: "light",
    };
    useSettingsStore.getState().replace(next);
    expect(useSettingsStore.getState().settings).toEqual(next);
  });
});
