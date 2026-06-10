import { Broadcast, Pause, X } from "@phosphor-icons/react";
import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { macHex, macKey } from "../../lib/macFormat";
import { useCinemaStore } from "../../stores/useCinemaStore";
import { useEmissionStore } from "../../stores/useEmissionStore";
import { useLatencyStore } from "../../stores/useLatencyStore";
import { usePerDeviceSettingsStore } from "../../stores/usePerDeviceSettingsStore";
import { useTrackerStore } from "../../stores/useTrackerStore";
import { TrackerViz } from "./TrackerViz";

/**
 * Immersive full-window overlay for use while a VR session is running:
 * minimal chrome, oversized tracker visuals, key bridge metrics. Opens
 * with `Ctrl+Enter` (or via the tray / palette) and exits with `Esc`.
 * Renders on top of every route so users can flip it on without
 * navigating away from whatever they were doing.
 */
export function CinemaMode() {
  const { t } = useTranslation();
  const open = useCinemaStore((s) => s.open);
  const close = useCinemaStore((s) => s.close);
  const trackers = useTrackerStore((s) => s.trackers);
  const perDev = usePerDeviceSettingsStore((s) => s.perMac);
  const latencyMap = useLatencyStore((s) => s.perMac);
  const paused = useEmissionStore((s) => s.paused);
  const toggleBridge = useEmissionStore((s) => s.toggle);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, close]);

  const list = useMemo(() => {
    return Object.values(trackers)
      .filter((s) => !perDev[macKey(s.mac)]?.hidden)
      .sort((a, b) => {
        const oa = perDev[macKey(a.mac)]?.display_order ?? 0;
        const ob = perDev[macKey(b.mac)]?.display_order ?? 0;
        return oa - ob;
      });
  }, [trackers, perDev]);

  const worstP95 = useMemo(() => {
    let max = 0;
    for (const e of Object.values(latencyMap)) {
      if (e.samples_window > 0 && e.interval_us_p95 > max) max = e.interval_us_p95;
    }
    return max;
  }, [latencyMap]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[60] flex flex-col bg-[var(--bg-base)]/95 backdrop-blur-md">
      <header className="surface-glass flex items-center justify-between border-b border-[var(--border-subtle)] px-6 py-3">
        <div className="flex items-center gap-3">
          <span className="text-[10px] font-semibold uppercase tracking-[0.3em] text-[var(--accent)]">
            {t("cinema.title")}
          </span>
          <span className="text-[10px] text-[var(--fg-muted)]">
            {t("status.tracker_count_plural", { count: list.length })}
          </span>
        </div>

        <div className="flex items-center gap-2 text-[11px] text-[var(--fg-secondary)]">
          <Metric label={t("cinema.worst_p95")} value={fmtUs(worstP95)} />
          <button
            type="button"
            onClick={() => void toggleBridge()}
            className={`flex items-center gap-1 rounded-[var(--radius-sm)] border px-2 py-1 text-[11px] font-semibold transition-colors ${
              paused
                ? "border-[var(--danger)] bg-[var(--danger-soft)] text-[var(--danger)]"
                : "border-[var(--success)] bg-[var(--success-soft)] text-[var(--success)]"
            }`}
          >
            {paused ? <Pause size={12} weight="fill" /> : <Broadcast size={12} weight="fill" />}
            {paused ? t("bridge.off") : t("bridge.on")}
          </button>
          <button
            type="button"
            onClick={close}
            aria-label={t("window.close")}
            className="grid size-7 place-items-center rounded-[var(--radius-sm)] text-[var(--fg-muted)] hover:bg-[var(--accent-soft)] hover:text-[var(--fg-primary)]"
          >
            <X size={14} />
          </button>
        </div>
      </header>

      <div className="flex-1 overflow-auto p-8">
        {list.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-[var(--fg-muted)]">
            {t("hints.no_trackers")}
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-6">
            {list.map((snap) => {
              const k = macKey(snap.mac);
              const lat = latencyMap[k];
              const label = perDev[k]?.label ?? "";
              const group = perDev[k]?.group ?? "";
              return (
                <div
                  key={k}
                  className="flex flex-col items-center gap-3 rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--bg-panel)]/60 p-4 backdrop-blur"
                >
                  <div className="scale-150">
                    <TrackerViz quat={snap.quat_xyzw} />
                  </div>
                  <div className="text-center">
                    <div className="text-sm font-semibold text-[var(--fg-primary)]">
                      {label || macHex(snap.mac)}
                    </div>
                    {group && (
                      <div className="mt-0.5 text-[10px] uppercase tracking-[0.2em] text-[var(--accent)]">
                        {group}
                      </div>
                    )}
                  </div>
                  <div className="grid w-full grid-cols-2 gap-2 text-[10px] text-[var(--fg-muted)]">
                    <Cell
                      label={t("status.live").toUpperCase()}
                      value={`${Math.round(snap.rate_hz)} Hz`}
                    />
                    <Cell
                      label="p95"
                      value={lat && lat.samples_window > 0 ? fmtUs(lat.interval_us_p95) : "—"}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <footer className="border-t border-[var(--border-subtle)] px-6 py-2 text-center text-[10px] text-[var(--fg-muted)]">
        {t("cinema.exit_hint")}
      </footer>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="hidden items-baseline gap-1.5 sm:flex">
      <span className="text-[9px] uppercase tracking-[0.18em] text-[var(--fg-muted)]">{label}</span>
      <span className="metric-num text-[var(--fg-primary)]">{value}</span>
    </div>
  );
}

function Cell({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1">
      <div className="text-[9px] uppercase tracking-wide">{label}</div>
      <div className="metric-num text-[var(--fg-primary)]">{value}</div>
    </div>
  );
}

function fmtUs(us: number): string {
  if (!Number.isFinite(us) || us <= 0) return "—";
  if (us >= 1000) return `${(us / 1000).toFixed(1)} ms`;
  return `${Math.round(us)} µs`;
}
