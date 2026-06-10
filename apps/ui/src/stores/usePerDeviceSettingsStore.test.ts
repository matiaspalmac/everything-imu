import { beforeEach, describe, expect, it, vi } from "vitest";
import { api, type Mac, type PerDeviceSettingsDto } from "../api/client";
import { usePerDeviceSettingsStore } from "./usePerDeviceSettingsStore";

vi.mock("../api/client", () => ({
  api: {
    getPerDeviceSettings: vi.fn(),
  },
}));

const mocked = vi.mocked(api);

const MAC: Mac = [1, 2, 3, 4, 5, 6];
const KEY = "010203040506";

function dto(label: string): PerDeviceSettingsDto {
  return {
    fusion: "vqf",
    mounting: "identity",
    magnetometer_enabled: true,
    rotation_offset_deg: 0,
    gyro_scale: 1,
    label,
    hidden: false,
    display_order: 0,
    group: "",
  };
}

describe("usePerDeviceSettingsStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Auto-reset restores the *initial* state object, which holds the same
    // Set instance across tests — swap in a fresh one so a hypothetical
    // stuck in-flight entry can never leak between tests.
    usePerDeviceSettingsStore.setState({ inflight: new Set() });
  });

  it("ensure fetches once and serves later calls from cache", async () => {
    mocked.getPerDeviceSettings.mockResolvedValue({ status: "ok", data: dto("shin") });
    const first = await usePerDeviceSettingsStore.getState().ensure(MAC);
    const second = await usePerDeviceSettingsStore.getState().ensure(MAC);
    expect(first?.label).toBe("shin");
    expect(second?.label).toBe("shin");
    expect(mocked.getPerDeviceSettings).toHaveBeenCalledTimes(1);
  });

  it("ensure returns null and caches nothing on backend error", async () => {
    mocked.getPerDeviceSettings.mockResolvedValue({
      status: "error",
      error: "nope",
    } as never);
    const result = await usePerDeviceSettingsStore.getState().ensure(MAC);
    expect(result).toBeNull();
    expect(usePerDeviceSettingsStore.getState().perMac[KEY]).toBeUndefined();
  });

  it("ensure dedupes a concurrent in-flight fetch", async () => {
    let resolve!: (v: { status: "ok"; data: PerDeviceSettingsDto }) => void;
    mocked.getPerDeviceSettings.mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const first = usePerDeviceSettingsStore.getState().ensure(MAC);
    const second = await usePerDeviceSettingsStore.getState().ensure(MAC);
    expect(second).toBeNull(); // in-flight, nothing cached yet
    resolve({ status: "ok", data: dto("shin") });
    await first;
    expect(mocked.getPerDeviceSettings).toHaveBeenCalledTimes(1);
    expect(usePerDeviceSettingsStore.getState().perMac[KEY].label).toBe("shin");
  });

  it("refresh overwrites the cached entry", async () => {
    mocked.getPerDeviceSettings.mockResolvedValue({ status: "ok", data: dto("old") });
    await usePerDeviceSettingsStore.getState().ensure(MAC);
    mocked.getPerDeviceSettings.mockResolvedValue({ status: "ok", data: dto("new") });
    await usePerDeviceSettingsStore.getState().refresh(MAC);
    expect(usePerDeviceSettingsStore.getState().perMac[KEY].label).toBe("new");
  });

  it("patch merges into an existing entry and ignores unknown macs", async () => {
    mocked.getPerDeviceSettings.mockResolvedValue({ status: "ok", data: dto("shin") });
    await usePerDeviceSettingsStore.getState().ensure(MAC);
    usePerDeviceSettingsStore.getState().patch(MAC, { hidden: true });
    expect(usePerDeviceSettingsStore.getState().perMac[KEY].hidden).toBe(true);
    expect(usePerDeviceSettingsStore.getState().perMac[KEY].label).toBe("shin");

    usePerDeviceSettingsStore.getState().patch([9, 9, 9, 9, 9, 9], { hidden: true });
    expect(usePerDeviceSettingsStore.getState().perMac["090909090909"]).toBeUndefined();
  });
});
