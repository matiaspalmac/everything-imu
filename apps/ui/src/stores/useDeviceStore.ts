import { create } from "zustand";
import type { DeviceMetadataDto } from "../api/client";
import { macKey } from "../lib/macFormat";

type State = {
  devices: Record<string, DeviceMetadataDto>;
  setAll(list: DeviceMetadataDto[]): void;
  add(d: DeviceMetadataDto): void;
};

export const useDeviceStore = create<State>((set) => ({
  devices: {},
  setAll: (list) =>
    set({
      devices: Object.fromEntries(list.map((d) => [macKey(d.mac), d])),
    }),
  add: (d) =>
    set((s) => ({
      devices: { ...s.devices, [macKey(d.mac)]: d },
    })),
}));
