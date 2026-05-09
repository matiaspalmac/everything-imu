import { create } from "zustand";
import type { SettingsDto } from "../api/client";

type State = {
  settings: SettingsDto;
  set(patch: Partial<SettingsDto>): void;
  replace(s: SettingsDto): void;
};

export const useSettingsStore = create<State>((set) => ({
  settings: {
    slime_server_addr: "127.0.0.1:6969",
    log_filter: "info",
    theme: "dark",
    auto_start_synthetic: false,
  },
  set: (patch) => set((s) => ({ settings: { ...s.settings, ...patch } })),
  replace: (s) => set({ settings: s }),
}));
