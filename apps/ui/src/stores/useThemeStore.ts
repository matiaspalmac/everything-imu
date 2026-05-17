import { create } from "zustand";

export type Theme = "dark" | "light" | "system";

const STORAGE_KEY = "everything-imu:theme";

function readSaved(): Theme {
  if (typeof window === "undefined") return "dark";
  const v = window.localStorage.getItem(STORAGE_KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "dark";
}

function effective(theme: Theme): "dark" | "light" {
  if (theme === "system") {
    if (typeof window === "undefined") return "dark";
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  return theme;
}

function applyToDom(theme: Theme) {
  if (typeof document === "undefined") return;
  const eff = effective(theme);
  document.documentElement.dataset.theme = eff;
  document.documentElement.classList.toggle("dark", eff === "dark");
}

type State = {
  theme: Theme;
  set(t: Theme): void;
};

export const useThemeStore = create<State>((set) => ({
  theme: readSaved(),
  set: (t) => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(STORAGE_KEY, t);
    }
    applyToDom(t);
    set({ theme: t });
  },
}));

// First paint sync: apply saved theme on module load.
applyToDom(readSaved());

// Listen for OS-level changes when "system" is active.
if (typeof window !== "undefined") {
  const mq = window.matchMedia("(prefers-color-scheme: light)");
  mq.addEventListener("change", () => {
    if (useThemeStore.getState().theme === "system") {
      applyToDom("system");
    }
  });
}
