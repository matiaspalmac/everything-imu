import {
  ArrowsClockwise,
  Crosshair,
  GearSix,
  House,
  ListBullets,
  type Icon as PhosphorIcon,
  Plugs,
  Pulse,
  Target,
} from "@phosphor-icons/react";
import { Command } from "cmdk";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../api/client";
import { useTrackerStore } from "../stores/useTrackerStore";

type Action = {
  id: string;
  label: string;
  hint?: string;
  icon: PhosphorIcon;
  run: () => void | Promise<void>;
};

/**
 * Ctrl+K / Cmd+K command palette over routes + broadcast actions.
 *
 * Cmdk handles the fuzzy ranking + keyboard nav internally; this
 * component only wires the action registry and the global hotkey.
 */
export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const trackers = useTrackerStore((s) => s.trackers);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const macs = Object.values(trackers).map((t) => t.mac);

  function broadcast(kind: "yaw" | "full" | "mounting") {
    for (const m of macs) void api.requestReset(m, kind);
  }

  const actions: Action[] = [
    {
      id: "nav-dash",
      label: "Go to Dashboard",
      icon: House,
      run: () => navigate("/"),
    },
    {
      id: "nav-conn",
      label: "Go to Connection",
      hint: "diagnostics + activity feed",
      icon: Pulse,
      run: () => navigate("/connection"),
    },
    {
      id: "nav-dev",
      label: "Go to Devices",
      icon: Plugs,
      run: () => navigate("/devices"),
    },
    {
      id: "nav-logs",
      label: "Go to Logs",
      icon: ListBullets,
      run: () => navigate("/logs"),
    },
    {
      id: "nav-settings",
      label: "Go to Settings",
      icon: GearSix,
      run: () => navigate("/settings"),
    },
    {
      id: "broadcast-yaw",
      label: "Broadcast Yaw Reset",
      hint: `${macs.length} tracker${macs.length === 1 ? "" : "s"}`,
      icon: Crosshair,
      run: () => broadcast("yaw"),
    },
    {
      id: "broadcast-full",
      label: "Broadcast Full Reset",
      hint: `${macs.length} tracker${macs.length === 1 ? "" : "s"}`,
      icon: ArrowsClockwise,
      run: () => broadcast("full"),
    },
    {
      id: "broadcast-mounting",
      label: "Broadcast Mounting Calibrate",
      hint: `${macs.length} tracker${macs.length === 1 ? "" : "s"}`,
      icon: Target,
      run: () => broadcast("mounting"),
    },
  ];

  function exec(a: Action) {
    setOpen(false);
    void a.run();
  }

  if (!open) return null;
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: dimmer click closes palette; cmdk owns inner keyboard nav
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-24 backdrop-blur-sm"
      onClick={() => setOpen(false)}
      onKeyDown={(e) => {
        if (e.key === "Escape") setOpen(false);
      }}
    >
      <Command
        label="Command palette"
        loop
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-xl overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] shadow-2xl"
      >
        <Command.Input
          autoFocus
          placeholder="Type a command or search…"
          className="w-full border-b border-[var(--border-subtle)] bg-transparent px-4 py-3 text-sm text-[var(--fg-primary)] placeholder:text-[var(--fg-muted)] focus:outline-none"
        />
        <Command.List className="max-h-80 overflow-auto p-2">
          <Command.Empty className="px-3 py-4 text-center text-xs text-[var(--fg-muted)]">
            No matching command.
          </Command.Empty>
          {actions.map((a) => {
            const Icon = a.icon;
            return (
              <Command.Item
                key={a.id}
                value={`${a.label} ${a.hint ?? ""}`}
                onSelect={() => exec(a)}
                className="flex cursor-pointer items-center gap-3 rounded-[var(--radius-sm)] px-3 py-2 text-sm text-[var(--fg-secondary)] data-[selected=true]:bg-[var(--warn-soft)] data-[selected=true]:text-[var(--accent)]"
              >
                <Icon size={16} weight="duotone" />
                <span className="flex-1 text-[var(--fg-primary)]">{a.label}</span>
                {a.hint && <span className="text-[10px] text-[var(--fg-muted)]">{a.hint}</span>}
              </Command.Item>
            );
          })}
        </Command.List>
        <div className="flex items-center justify-between border-t border-[var(--border-subtle)] px-3 py-1.5 text-[10px] text-[var(--fg-muted)]">
          <span>↑↓ navigate · ↵ select · Esc close</span>
          <span>Ctrl+K toggle</span>
        </div>
      </Command>
    </div>
  );
}
