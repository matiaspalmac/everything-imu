import { create } from "zustand";
import type { LogEntryDto } from "../api/client";

const CAP = 5000;

/** Store-side wrapper: a monotonically increasing `seq` gives every row a
 * unique, stable React key — `ts_ms + target + message` collides when the
 * backend emits identical lines within the same millisecond. */
type LogEntry = LogEntryDto & { seq: number };

let nextSeq = 0;

type State = {
  entries: LogEntry[];
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
    const entry: LogEntry = { ...e, seq: nextSeq++ };
    set((s) => ({
      entries: s.entries.length >= CAP ? [...s.entries.slice(1), entry] : [...s.entries, entry],
    }));
  },
  pushBatch: (es) => {
    if (get().paused) return;
    const stamped: LogEntry[] = es.map((e) => ({ ...e, seq: nextSeq++ }));
    set((s) => ({
      entries: [...s.entries, ...stamped].slice(-CAP),
    }));
  },
  setFilterLevel: (l) => set({ filterLevel: l }),
  setFilterText: (t) => set({ filterText: t }),
  setPaused: (p) => set({ paused: p }),
  setFollow: (f) => set({ follow: f }),
}));
