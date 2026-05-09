import { create } from "zustand";
import type { TrackerSnapshot } from "../api/client";
import { macKey } from "../lib/macFormat";

const HIST_LEN = 60;

type RateHistory = {
  rates: number[];
  lastSeenMs: number;
};

type State = {
  startedAtMs: number;
  totalUpdates: number;
  totalPackets: number;
  perMacHist: Record<string, RateHistory>;
  observe(snapshots: TrackerSnapshot[]): void;
  reset(): void;
};

export const useMetricsStore = create<State>((set, get) => ({
  startedAtMs: Date.now(),
  totalUpdates: 0,
  totalPackets: 0,
  perMacHist: {},
  observe: (snapshots) => {
    const now = Date.now();
    const next: Record<string, RateHistory> = { ...get().perMacHist };
    let added = 0;
    for (const s of snapshots) {
      const k = macKey(s.mac);
      const cur = next[k] ?? { rates: [], lastSeenMs: now };
      const rates = [...cur.rates, s.rate_hz].slice(-HIST_LEN);
      next[k] = { rates, lastSeenMs: now };
      added += s.rate_hz;
    }
    set((st) => ({
      totalUpdates: st.totalUpdates + 1,
      totalPackets: st.totalPackets + Math.round(added / Math.max(1, snapshots.length)),
      perMacHist: next,
    }));
  },
  reset: () =>
    set({
      startedAtMs: Date.now(),
      totalUpdates: 0,
      totalPackets: 0,
      perMacHist: {},
    }),
}));
