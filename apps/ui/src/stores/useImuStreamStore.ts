import { create } from "zustand";
import type { ImuSampleUpdate } from "../api/client";
import { macKey } from "../lib/macFormat";

const HIST_LEN = 240;

export type ImuHistory = {
  gyr: [number[], number[], number[]];
  acc: [number[], number[], number[]];
  ts: number[];
};

function emptyHist(): ImuHistory {
  return {
    gyr: [[], [], []],
    acc: [[], [], []],
    ts: [],
  };
}

function pushCapped(arr: number[], v: number) {
  arr.push(v);
  if (arr.length > HIST_LEN) arr.shift();
}

type State = {
  perMac: Record<string, ImuHistory>;
  ingest(update: ImuSampleUpdate): void;
  clear(): void;
};

export const useImuStreamStore = create<State>((set, get) => ({
  perMac: {},
  ingest: (update) => {
    const next = { ...get().perMac };
    for (const e of update.samples) {
      const k = macKey(e.mac);
      const h = next[k] ?? emptyHist();
      for (let i = 0; i < 3; i++) {
        pushCapped(h.gyr[i] as number[], e.gyr_xyz[i] ?? 0);
        pushCapped(h.acc[i] as number[], e.acc_xyz[i] ?? 0);
      }
      pushCapped(h.ts, e.elapsed_ms);
      next[k] = h;
    }
    set({ perMac: next });
  },
  clear: () => set({ perMac: {} }),
}));
