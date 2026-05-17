import { create } from "zustand";
import { api, type Mac, type PerDeviceSettingsDto } from "../api/client";
import { macKey } from "../lib/macFormat";

type State = {
  perMac: Record<string, PerDeviceSettingsDto>;
  inflight: Set<string>;
  ensure(mac: Mac): Promise<PerDeviceSettingsDto | null>;
  refresh(mac: Mac): Promise<void>;
  patch(mac: Mac, patch: Partial<PerDeviceSettingsDto>): void;
};

/**
 * Lazy cache for per-device settings. Components call `ensure(mac)` once
 * on mount; subsequent reads come from the store without IPC chatter.
 * Mutations call `patch` for optimistic UI and then `refresh` after the
 * backend command resolves.
 */
export const usePerDeviceSettingsStore = create<State>((set, get) => ({
  perMac: {},
  inflight: new Set(),
  async ensure(mac) {
    const k = macKey(mac);
    const s = get();
    if (s.perMac[k] || s.inflight.has(k)) return s.perMac[k] ?? null;
    s.inflight.add(k);
    const res = await api.getPerDeviceSettings(mac);
    s.inflight.delete(k);
    if (res.status === "ok") {
      set((cur) => ({ perMac: { ...cur.perMac, [k]: res.data } }));
      return res.data;
    }
    return null;
  },
  async refresh(mac) {
    const k = macKey(mac);
    const res = await api.getPerDeviceSettings(mac);
    if (res.status === "ok") {
      set((cur) => ({ perMac: { ...cur.perMac, [k]: res.data } }));
    }
  },
  patch(mac, patch) {
    const k = macKey(mac);
    set((cur) => {
      const existing = cur.perMac[k];
      if (!existing) return cur;
      return { perMac: { ...cur.perMac, [k]: { ...existing, ...patch } } };
    });
  },
}));
