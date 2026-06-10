import { describe, expect, it } from "vitest";
import type { DeviceMetadataDto } from "../api/client";
import { useDeviceStore } from "./useDeviceStore";

function device(
  mac: [number, number, number, number, number, number],
  serial: string,
): DeviceMetadataDto {
  return {
    mac,
    serial,
    kind: "joycon_left",
    firmware: null,
    has_magnetometer: false,
    has_battery: true,
    has_rumble: true,
    native_imu_rate_hz: 200,
  };
}

describe("useDeviceStore", () => {
  it("setAll replaces the whole map keyed by mac", () => {
    useDeviceStore.getState().add(device([9, 9, 9, 9, 9, 9], "OLD"));
    useDeviceStore.getState().setAll([device([1, 2, 3, 4, 5, 6], "A")]);
    const devices = useDeviceStore.getState().devices;
    expect(Object.keys(devices)).toEqual(["010203040506"]);
    expect(devices["010203040506"].serial).toBe("A");
  });

  it("add merges without dropping existing devices", () => {
    useDeviceStore.getState().setAll([device([1, 2, 3, 4, 5, 6], "A")]);
    useDeviceStore.getState().add(device([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff], "B"));
    const devices = useDeviceStore.getState().devices;
    expect(Object.keys(devices).sort()).toEqual(["010203040506", "aabbccddeeff"]);
  });

  it("add overwrites an existing mac with fresher metadata", () => {
    useDeviceStore.getState().setAll([device([1, 2, 3, 4, 5, 6], "A")]);
    useDeviceStore.getState().add(device([1, 2, 3, 4, 5, 6], "A2"));
    expect(useDeviceStore.getState().devices["010203040506"].serial).toBe("A2");
  });
});
