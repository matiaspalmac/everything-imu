import { create } from "zustand";

type State = {
  open: boolean;
  toggle(): void;
  close(): void;
};

export const useCinemaStore = create<State>((set, get) => ({
  open: false,
  toggle: () => set({ open: !get().open }),
  close: () => set({ open: false }),
}));
