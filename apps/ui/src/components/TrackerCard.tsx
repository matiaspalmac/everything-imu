import { useEffect, useState } from "react";
import type { TrackerSnapshot } from "../api/client";
import { api } from "../api/client";
import { macHex, macKey } from "../lib/macFormat";
import { StatusBadge } from "./StatusBadge";
import { TrackerViz } from "./TrackerViz";

export function TrackerCard({ snap, targetHz }: { snap: TrackerSnapshot; targetHz: number }) {
  const battery = Number.isFinite(snap.battery_fraction)
    ? Math.round(snap.battery_fraction * 100)
    : null;
  const [label, setLabel] = useState<string>("");
  // biome-ignore lint/correctness/useExhaustiveDependencies: snap.mac stable per card
  useEffect(() => {
    let cancelled = false;
    api.getPerDeviceSettings(snap.mac).then((res) => {
      if (!cancelled && res.status === "ok") setLabel(res.data.label ?? "");
    });
    return () => {
      cancelled = true;
    };
  }, [macKey(snap.mac)]);

  return (
    <div className="flex gap-4 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4 transition hover:border-[var(--border-strong)]">
      <TrackerViz quat={snap.quat_xyzw} />
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate text-sm text-[var(--fg-primary)]">
            {label || <span className="font-mono">{macHex(snap.mac)}</span>}
          </span>
          <StatusBadge rateHz={snap.rate_hz} targetHz={targetHz} />
        </div>
        <span className="truncate font-mono text-[10px] text-[var(--fg-muted)]">
          {label ? macHex(snap.mac) : snap.serial}
        </span>
        {battery !== null && (
          <div className="flex items-center gap-2 text-xs text-[var(--fg-secondary)]">
            <span>battery</span>
            <div className="h-1.5 flex-1 overflow-hidden rounded bg-[var(--bg-elevated)]">
              <div className="h-full rounded bg-[var(--accent)]" style={{ width: `${battery}%` }} />
            </div>
            <span className="font-mono">{battery}%</span>
          </div>
        )}
      </div>
    </div>
  );
}
