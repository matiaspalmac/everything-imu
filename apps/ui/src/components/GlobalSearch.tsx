import { ListBullets, MagnifyingGlass, Plugs, Tag } from "@phosphor-icons/react";
import { Command } from "cmdk";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { macHex, macKey } from "../lib/macFormat";
import { useDeviceStore } from "../stores/useDeviceStore";
import { useLogStore } from "../stores/useLogStore";
import { usePerDeviceSettingsStore } from "../stores/usePerDeviceSettingsStore";
import { useTrackerStore } from "../stores/useTrackerStore";

type Hit =
  | { kind: "tracker"; key: string; label: string; mac: number[] }
  | { kind: "device"; key: string; label: string; mac: number[] }
  | { kind: "log"; ts: number; level: string; target: string; message: string };

/**
 * Ctrl+F global search across trackers, devices and the in-memory log
 * buffer. Cheap client-side filter — small N today (<= a few hundred
 * log lines), so no virtualization or index needed. Mounted once in
 * AppShell next to CommandPalette; both share the cmdk primitive but
 * serve different purposes: Ctrl+K is action-oriented, Ctrl+F is
 * lookup-oriented.
 */
export function GlobalSearch() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  // oxlint-disable-next-line react-doctor/rerender-state-only-in-handlers -- open IS read at `if (!open) return null` below
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");

  const trackers = useTrackerStore((s) => s.trackers);
  const devices = useDeviceStore((s) => s.devices);
  const perDev = usePerDeviceSettingsStore((s) => s.perMac);
  const logs = useLogStore((s) => s.entries);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setOpen((v) => !v);
      } else if (e.key === "Escape" && open) {
        setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  const hits = useMemo<Hit[]>(() => {
    if (!q.trim()) return [];
    const needle = q.toLowerCase();
    const out: Hit[] = [];

    for (const snap of Object.values(trackers)) {
      const k = macKey(snap.mac);
      const label = perDev[k]?.label ?? "";
      const hay = `${label} ${macHex(snap.mac)} ${snap.serial}`.toLowerCase();
      // oxlint-disable-next-line react-doctor/js-set-map-lookups -- String.prototype.includes, not Array.includes
      if (hay.includes(needle)) {
        out.push({ kind: "tracker", key: k, label: label || macHex(snap.mac), mac: snap.mac });
      }
    }

    for (const dev of Object.values(devices)) {
      const k = macKey(dev.mac);
      if (trackers[k]) continue; // already listed as tracker
      const hay = `${dev.kind} ${macHex(dev.mac)} ${dev.firmware ?? ""}`.toLowerCase();
      // oxlint-disable-next-line react-doctor/js-set-map-lookups -- String.prototype.includes, not Array.includes
      if (hay.includes(needle)) {
        out.push({ kind: "device", key: k, label: `${dev.kind} ${macHex(dev.mac)}`, mac: dev.mac });
      }
    }

    // Limit log scan — last 200 entries cover what's visible in Logs page.
    const recent = logs.slice(-200);
    for (let i = recent.length - 1; i >= 0 && out.length < 50; i--) {
      const e = recent[i];
      const hay = `${e.target} ${e.message} ${e.level}`.toLowerCase();
      // oxlint-disable-next-line react-doctor/js-set-map-lookups -- String.prototype.includes, not Array.includes
      if (hay.includes(needle)) {
        out.push({
          kind: "log",
          ts: e.ts_ms,
          level: e.level,
          target: e.target,
          message: e.message,
        });
      }
    }

    return out;
  }, [q, trackers, devices, perDev, logs]);

  function go(hit: Hit) {
    setOpen(false);
    setQ("");
    if (hit.kind === "tracker") {
      navigate(`/devices/${hit.key}`);
    } else if (hit.kind === "device") {
      navigate("/devices");
    } else {
      navigate("/logs");
    }
  }

  if (!open) return null;
  return (
    // oxlint-disable-next-line jsx-a11y/no-static-element-interactions -- dimmer
    // biome-ignore lint/a11y/noStaticElementInteractions: dimmer click closes overlay; cmdk owns inner keyboard nav
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-24 backdrop-blur-sm"
      role="presentation"
      onClick={() => setOpen(false)}
      onKeyDown={(e) => {
        if (e.key === "Escape") setOpen(false);
      }}
    >
      <Command
        label={t("search.title")}
        loop
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-xl overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] shadow-2xl"
        shouldFilter={false}
      >
        <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-3">
          <MagnifyingGlass size={14} className="text-[var(--fg-muted)]" />
          <Command.Input
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- search overlay opened by hotkey; focus on open is expected UX
            autoFocus
            value={q}
            onValueChange={setQ}
            placeholder={t("search.placeholder")}
            className="w-full bg-transparent py-3 text-sm text-[var(--fg-primary)] placeholder:text-[var(--fg-muted)] focus:outline-none"
          />
        </div>
        <Command.List className="max-h-96 overflow-auto p-2">
          {q.trim().length === 0 ? (
            <div className="px-3 py-4 text-center text-xs text-[var(--fg-muted)]">
              {t("search.empty_hint")}
            </div>
          ) : hits.length === 0 ? (
            <Command.Empty className="px-3 py-4 text-center text-xs text-[var(--fg-muted)]">
              {t("search.no_results")}
            </Command.Empty>
          ) : (
            hits.map((h) => {
              const Icon = h.kind === "tracker" ? Tag : h.kind === "device" ? Plugs : ListBullets;
              const subline =
                h.kind === "log"
                  ? `${h.level.toUpperCase()} · ${h.target}`
                  : h.kind === "tracker"
                    ? macHex(h.mac)
                    : "device";
              const primary = h.kind === "log" ? h.message : h.label;
              const id =
                h.kind === "log" ? `log-${h.ts}-${h.message.slice(0, 32)}` : `${h.kind}-${h.key}`;
              return (
                <Command.Item
                  key={id}
                  value={id}
                  onSelect={() => go(h)}
                  className="flex cursor-pointer items-start gap-3 rounded-[var(--radius-sm)] px-3 py-2 text-sm text-[var(--fg-secondary)] data-[selected=true]:bg-[var(--warn-soft)] data-[selected=true]:text-[var(--accent)]"
                >
                  <Icon size={14} weight="duotone" className="mt-0.5" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[var(--fg-primary)]">{primary}</div>
                    <div className="truncate text-[10px] text-[var(--fg-muted)]">{subline}</div>
                  </div>
                </Command.Item>
              );
            })
          )}
        </Command.List>
        <div className="flex items-center justify-between border-t border-[var(--border-subtle)] px-3 py-1.5 text-[10px] text-[var(--fg-muted)]">
          <span>{t("search.footer")}</span>
          <span>Ctrl+F</span>
        </div>
      </Command>
    </div>
  );
}
