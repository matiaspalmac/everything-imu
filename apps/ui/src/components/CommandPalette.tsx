import {
  ArrowsClockwise,
  Broadcast,
  Crosshair,
  FilmStrip,
  GearSix,
  House,
  ListBullets,
  Pause,
  type Icon as PhosphorIcon,
  Plugs,
  Pulse,
  Target,
} from "@phosphor-icons/react";
import { Command } from "cmdk";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { api } from "../api/client";
import { useCinemaStore } from "../stores/useCinemaStore";
import { useEmissionStore } from "../stores/useEmissionStore";
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
  const { t } = useTranslation();
  // oxlint-disable-next-line react-doctor/rerender-state-only-in-handlers -- open IS read at `if (!open) return null` below
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const trackers = useTrackerStore((s) => s.trackers);
  const paused = useEmissionStore((s) => s.paused);
  const toggleEmission = useEmissionStore((s) => s.toggle);
  const toggleCinema = useCinemaStore((s) => s.toggle);

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

  const macs = Object.values(trackers).map((tr) => tr.mac);

  function broadcast(kind: "yaw" | "full" | "mounting") {
    for (const m of macs) void api.requestReset(m, kind);
  }

  const countHint = t(macs.length === 1 ? "status.tracker_count" : "status.tracker_count_plural", {
    count: macs.length,
  });
  const actions: Action[] = [
    {
      id: "nav-dash",
      label: t("palette.go_dashboard"),
      icon: House,
      run: () => navigate("/"),
    },
    {
      id: "nav-conn",
      label: t("palette.go_connection"),
      hint: t("palette.go_connection_hint"),
      icon: Pulse,
      run: () => navigate("/connection"),
    },
    {
      id: "nav-dev",
      label: t("palette.go_devices"),
      icon: Plugs,
      run: () => navigate("/devices"),
    },
    {
      id: "nav-logs",
      label: t("palette.go_logs"),
      icon: ListBullets,
      run: () => navigate("/logs"),
    },
    {
      id: "nav-settings",
      label: t("palette.go_settings"),
      icon: GearSix,
      run: () => navigate("/settings"),
    },
    {
      id: "bridge-toggle",
      label: paused ? t("palette.bridge_resume") : t("palette.bridge_pause"),
      hint: "Ctrl+Shift+B",
      icon: paused ? Broadcast : Pause,
      run: async () => {
        await toggleEmission();
      },
    },
    {
      id: "cinema-toggle",
      label: t("palette.cinema_toggle"),
      hint: "Ctrl+Enter",
      icon: FilmStrip,
      run: () => toggleCinema(),
    },
    {
      id: "broadcast-yaw",
      label: t("palette.bc_yaw"),
      hint: countHint,
      icon: Crosshair,
      run: () => broadcast("yaw"),
    },
    {
      id: "broadcast-full",
      label: t("palette.bc_full"),
      hint: countHint,
      icon: ArrowsClockwise,
      run: () => broadcast("full"),
    },
    {
      id: "broadcast-mounting",
      label: t("palette.bc_mounting"),
      hint: countHint,
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
    // oxlint-disable-next-line jsx-a11y/no-static-element-interactions
    // biome-ignore lint/a11y/noStaticElementInteractions: dimmer click closes palette; cmdk owns inner keyboard nav
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-24 backdrop-blur-sm"
      role="presentation"
      onClick={() => setOpen(false)}
      onKeyDown={(e) => {
        if (e.key === "Escape") setOpen(false);
      }}
    >
      <Command
        label={t("palette.title")}
        loop
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-xl overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] shadow-2xl"
      >
        <Command.Input
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- command palette opened by hotkey; focus on open is expected UX
          autoFocus
          placeholder={t("palette.placeholder")}
          className="w-full border-b border-[var(--border-subtle)] bg-transparent px-4 py-3 text-sm text-[var(--fg-primary)] placeholder:text-[var(--fg-muted)] focus:outline-none"
        />
        <Command.List className="max-h-80 overflow-auto p-2">
          <Command.Empty className="px-3 py-4 text-center text-xs text-[var(--fg-muted)]">
            {t("palette.empty")}
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
          <span>{t("palette.footer_nav")}</span>
          <span>{t("palette.footer_toggle")}</span>
        </div>
      </Command>
    </div>
  );
}
