import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api/client";
import { useCinemaStore } from "../stores/useCinemaStore";
import { useEmissionStore } from "../stores/useEmissionStore";
import { useToastStore } from "../stores/useToastStore";
import { useTrackerStore } from "../stores/useTrackerStore";

/**
 * Mounted once in AppShell. Listens for global hotkeys outside of input
 * fields and dispatches broadcast commands. Ctrl+K is owned by
 * CommandPalette; we deliberately don't claim it here.
 */
export function KeyboardShortcuts() {
  // Same trap as EventBridge: t from useTranslation is a new ref on every
  // render, and `trackers` mutates at the tracker-update cadence (~30 Hz).
  // Putting either in the deps array of the listener-registration effect
  // would re-add the window keydown listener at that rate, leaking
  // handlers into the browser. We read trackers directly from the store
  // inside the handler and pull t/toast/toggles via refs.
  const { t } = useTranslation();
  const tRef = useRef(t);
  useEffect(() => {
    tRef.current = t;
  }, [t]);

  const pushToast = useToastStore((s) => s.push);
  const toggleEmission = useEmissionStore((s) => s.toggle);
  const toggleCinema = useCinemaStore((s) => s.toggle);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const inField =
        !!target &&
        (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);

      // Ctrl+Shift+B — bridge kill-switch (allowed even with modifiers).
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && !e.altKey && e.key.toLowerCase() === "b") {
        if (inField) return;
        e.preventDefault();
        void toggleEmission().then((nextPaused) => {
          pushToast({
            level: nextPaused ? "warn" : "info",
            message: tRef.current(nextPaused ? "bridge.toast_paused" : "bridge.toast_resumed"),
            ttlMs: 2000,
          });
        });
        return;
      }

      // Ctrl+Enter — toggle Cinema mode overlay. F11 would conflict with
      // window-level fullscreen on most platforms, so we use Enter+modifier.
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key === "Enter") {
        if (inField) return;
        e.preventDefault();
        toggleCinema();
        return;
      }

      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (inField) return;
      const trackers = useTrackerStore.getState().trackers;
      const macs = Object.values(trackers).map((tr) => tr.mac);
      if (macs.length === 0) return;
      if (e.key === "r" || e.key === "R") {
        e.preventDefault();
        const kind: "yaw" | "full" = e.shiftKey ? "full" : "yaw";
        for (const m of macs) void api.requestReset(m, kind);
        const baseKey =
          kind === "full" ? "shortcuts.broadcast_full_done" : "shortcuts.broadcast_yaw_done";
        const key = macs.length === 1 ? baseKey : `${baseKey}_plural`;
        pushToast({
          level: "info",
          message: tRef.current(key, { count: macs.length }),
          ttlMs: 2500,
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pushToast, toggleEmission, toggleCinema]);

  return null;
}
