import { create } from "zustand";
import type { TrackerSnapshot } from "../api/client";
import { macKey } from "../lib/macFormat";

type State = {
  trackers: Record<string, TrackerSnapshot>;
  apply(update: { trackers: TrackerSnapshot[] }): void;
  clear(): void;
};

export const useTrackerStore = create<State>((set) => ({
  trackers: {},
  apply: (update) =>
    set(() => ({
      trackers: Object.fromEntries(update.trackers.map((t) => [macKey(t.mac), t])),
    })),
  clear: () => set({ trackers: {} }),
}));
