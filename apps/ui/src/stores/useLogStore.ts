import { create } from "zustand";
import type { LogEntryDto } from "../api/client";

const CAP = 5000;

type State = {
  entries: LogEntryDto[];
  filterLevel: string;
  filterText: string;
  paused: boolean;
  follow: boolean;
  push(e: LogEntryDto): void;
  pushBatch(es: LogEntryDto[]): void;
  setFilterLevel(l: string): void;
  setFilterText(t: string): void;
  setPaused(p: boolean): void;
  setFollow(f: boolean): void;
};

export const useLogStore = create<State>((set, get) => ({
  entries: [],
  filterLevel: "trace",
  filterText: "",
  paused: false,
  follow: true,
  push: (e) => {
    if (get().paused) return;
    set((s) => ({
      entries: s.entries.length >= CAP ? [...s.entries.slice(1), e] : [...s.entries, e],
    }));
  },
  pushBatch: (es) => {
    if (get().paused) return;
    set((s) => ({
      entries: [...s.entries, ...es].slice(-CAP),
    }));
  },
  setFilterLevel: (l) => set({ filterLevel: l }),
  setFilterText: (t) => set({ filterText: t }),
  setPaused: (p) => set({ paused: p }),
  setFollow: (f) => set({ follow: f }),
}));
