import { create } from "zustand";

export type ToastLevel = "info" | "success" | "warn" | "error";

export type Toast = {
  id: string;
  level: ToastLevel;
  title?: string;
  message: string;
  ts: number;
  ttlMs: number;
};

let nextId = 1;

type State = {
  toasts: Toast[];
  push(t: Omit<Toast, "id" | "ts" | "ttlMs"> & { ttlMs?: number }): string;
  dismiss(id: string): void;
};

export const useToastStore = create<State>((set) => ({
  toasts: [],
  push: (t) => {
    const id = `t${nextId++}`;
    const ttlMs = t.ttlMs ?? 5000;
    const toast: Toast = {
      id,
      level: t.level,
      title: t.title,
      message: t.message,
      ts: Date.now(),
      ttlMs,
    };
    set((s) => ({ toasts: [...s.toasts, toast] }));
    if (typeof window !== "undefined" && ttlMs > 0) {
      window.setTimeout(() => {
        set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }));
      }, ttlMs);
    }
    return id;
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) })),
}));
