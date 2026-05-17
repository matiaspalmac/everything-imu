import { create } from "zustand";
import type { LatencyEntry, LatencyUpdate } from "../api/client";
import { macKey } from "../lib/macFormat";

type State = {
  perMac: Record<string, LatencyEntry>;
  lastUpdateMs: number;
  ingest(update: LatencyUpdate): void;
};

export const useLatencyStore = create<State>((set, get) => ({
  perMac: {},
  lastUpdateMs: 0,
  ingest: (update) => {
    const next = { ...get().perMac };
    for (const e of update.entries) {
      next[macKey(e.mac)] = e;
    }
    set({ perMac: next, lastUpdateMs: Date.now() });
  },
}));
