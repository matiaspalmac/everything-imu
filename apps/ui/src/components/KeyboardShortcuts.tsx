import { useEffect } from "react";
import { api } from "../api/client";
import { useToastStore } from "../stores/useToastStore";
import { useTrackerStore } from "../stores/useTrackerStore";

/**
 * Mounted once in AppShell. Listens for global hotkeys outside of input
 * fields and dispatches broadcast commands. Ctrl+K is owned by
 * CommandPalette; we deliberately don't claim it here.
 */
export function KeyboardShortcuts() {
  const trackers = useTrackerStore((s) => s.trackers);
  const pushToast = useToastStore((s) => s.push);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)
      ) {
        return;
      }
      const macs = Object.values(trackers).map((t) => t.mac);
      if (macs.length === 0) return;
      if (e.key === "r" || e.key === "R") {
        e.preventDefault();
        const kind: "yaw" | "full" = e.shiftKey ? "full" : "yaw";
        for (const m of macs) void api.requestReset(m, kind);
        pushToast({
          level: "info",
          message: `Broadcast ${kind === "full" ? "Full" : "Yaw"} reset to ${macs.length} tracker${macs.length === 1 ? "" : "s"}`,
          ttlMs: 2500,
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [trackers, pushToast]);

  return null;
}
