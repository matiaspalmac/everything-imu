import { create } from "zustand";
import type { ConnectionStatusUpdate } from "../api/client";

type State = {
  status: ConnectionStatusUpdate | null;
  set(s: ConnectionStatusUpdate): void;
  clear(): void;
};

export const useConnectionStore = create<State>((set) => ({
  status: null,
  set: (s) => set({ status: s }),
  clear: () => set({ status: null }),
}));
