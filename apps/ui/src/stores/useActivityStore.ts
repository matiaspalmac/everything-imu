import { create } from "zustand";

export type ActivityEntry = {
  id: string;
  ts: number;
  level: "info" | "success" | "warn" | "error";
  message: string;
};

const CAP = 200;

type State = {
  entries: ActivityEntry[];
  push(e: Omit<ActivityEntry, "id" | "ts"> & { ts?: number }): void;
  clear(): void;
};

let nextId = 1;

export const useActivityStore = create<State>((set) => ({
  entries: [],
  push: (e) =>
    set((s) => {
      const entry: ActivityEntry = {
        id: `a${nextId++}`,
        ts: e.ts ?? Date.now(),
        level: e.level,
        message: e.message,
      };
      const merged = [entry, ...s.entries].slice(0, CAP);
      return { entries: merged };
    }),
  clear: () => set({ entries: [] }),
}));
