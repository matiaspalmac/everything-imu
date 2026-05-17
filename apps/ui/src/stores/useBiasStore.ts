import { create } from "zustand";
import type { BiasUpdate } from "../api/client";
import { macKey } from "../lib/macFormat";

type State = {
  perMac: Record<string, [number, number, number]>;
  ingest(update: BiasUpdate): void;
};

export const useBiasStore = create<State>((set, get) => ({
  perMac: {},
  ingest: (update) => {
    const next = { ...get().perMac };
    for (const e of update.entries) {
      next[macKey(e.mac)] = [e.gyr_bias[0], e.gyr_bias[1], e.gyr_bias[2]];
    }
    set({ perMac: next });
  },
}));
