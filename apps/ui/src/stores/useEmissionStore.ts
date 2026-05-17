import { create } from "zustand";
import { api } from "../api/client";

type State = {
  paused: boolean;
  hydrated: boolean;
  hydrate(): Promise<void>;
  toggle(): Promise<boolean>;
  set(paused: boolean): Promise<void>;
};

export const useEmissionStore = create<State>((set, get) => ({
  paused: false,
  hydrated: false,
  async hydrate() {
    const res = await api.getEmissionPaused();
    if (res.status === "ok") set({ paused: res.data, hydrated: true });
  },
  async toggle() {
    const next = !get().paused;
    set({ paused: next });
    const res = await api.setEmissionPaused(next);
    if (res.status !== "ok") {
      set({ paused: !next });
      return get().paused;
    }
    return next;
  },
  async set(paused) {
    set({ paused });
    const res = await api.setEmissionPaused(paused);
    if (res.status !== "ok") set({ paused: !paused });
  },
}));
