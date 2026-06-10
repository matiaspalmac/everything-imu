// @vitest-environment jsdom
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

// jsdom does not implement matchMedia, and useThemeStore both calls it for
// "system" resolution and registers an OS-scheme change listener at module
// load — so the stub must exist before the module is imported (hence the
// dynamic import below).
//
// Note: the dynamic import also means the zustand auto-reset hook from
// __mocks__/zustand.ts registers too late for this file, so every test
// normalizes its own baseline in beforeEach instead of relying on it.
let prefersLight = false;
const mqListeners: Array<() => void> = [];

let useThemeStore: typeof import("./useThemeStore")["useThemeStore"];

beforeAll(async () => {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: prefersLight,
    media: query,
    addEventListener: (_event: string, cb: () => void) => {
      mqListeners.push(cb);
    },
    removeEventListener: () => {},
  }));
  ({ useThemeStore } = await import("./useThemeStore"));
});

describe("useThemeStore", () => {
  beforeEach(() => {
    window.localStorage.clear();
    prefersLight = false;
    useThemeStore.getState().set("dark");
  });

  it("set('dark') applies dataset + class to the DOM", () => {
    expect(useThemeStore.getState().theme).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("set('light') updates state, DOM, and persists to localStorage", () => {
    useThemeStore.getState().set("light");
    expect(useThemeStore.getState().theme).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(window.localStorage.getItem("everything-imu:theme")).toBe("light");
  });

  it("set('system') resolves the effective theme through matchMedia", () => {
    prefersLight = true;
    useThemeStore.getState().set("system");
    expect(useThemeStore.getState().theme).toBe("system");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("reacts to OS scheme changes while in system mode", () => {
    prefersLight = true;
    useThemeStore.getState().set("system");
    expect(document.documentElement.dataset.theme).toBe("light");
    prefersLight = false;
    for (const cb of mqListeners) cb();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("ignores OS scheme changes when an explicit theme is set", () => {
    useThemeStore.getState().set("light");
    prefersLight = false;
    for (const cb of mqListeners) cb();
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
