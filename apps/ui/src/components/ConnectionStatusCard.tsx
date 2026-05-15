import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useConnectionStore } from "../stores/useConnectionStore";
import { useTrackerStore } from "../stores/useTrackerStore";

function formatMsAgo(ms: number | null | undefined): string {
  if (ms == null) return "—";
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`;
}

export function ConnectionStatusCard() {
  const { t } = useTranslation();
  const status = useConnectionStore((s) => s.status);
  const trackers = useTrackerStore((s) => s.trackers);
  const list = Object.values(trackers);
  const liveCount = list.filter((tr) => tr.rate_hz > 0 || tr.quat_xyzw[3] !== 1).length;

  // Tick to refresh "ms ago" displays once per second.
  const [, force] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => force((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, []);

  const statusBadge = !status
    ? { label: t("conn.starting"), cls: "text-[var(--fg-muted)]" }
    : status.last_send_ms_ago != null && status.last_send_ms_ago < 2000
      ? { label: t("status.live"), cls: "text-[var(--success)]" }
      : list.length > 0
        ? { label: t("status.stalled"), cls: "text-[var(--warn)]" }
        : { label: t("status.idle"), cls: "text-[var(--fg-muted)]" };

  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <Stat label={t("conn.slime_target")} value={status?.server_addr ?? "—"} mono />
      <Stat
        label={t("conn.status")}
        value={statusBadge.label}
        valueClassName={`${statusBadge.cls} font-semibold`}
      />
      <Stat label={t("conn.live_trackers")} value={`${liveCount} / ${list.length}`} />
      <Stat
        label={t("conn.bundle_mode")}
        value={status?.server_supports_bundle ? t("conn.bundle") : t("conn.individual")}
        hint={
          status?.server_supports_bundle ? t("conn.feature_flags_ok") : t("conn.fallback_two_send")
        }
      />
      <Stat label={t("conn.packets_sent")} value={(status?.packets_sent ?? 0).toLocaleString()} />
      <Stat label={t("conn.last_send")} value={formatMsAgo(status?.last_send_ms_ago)} />
      <Stat label={t("conn.last_handshake")} value={formatMsAgo(status?.last_handshake_ms_ago)} />
      <Stat label={t("conn.protocol")} value={t("conn.protocol_short")} />
    </div>
  );
}

function Stat({
  label,
  value,
  hint,
  mono,
  valueClassName,
}: {
  label: string;
  value: string;
  hint?: string;
  mono?: boolean;
  valueClassName?: string;
}) {
  return (
    <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-3">
      <div className="text-[10px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
        {label}
      </div>
      <div
        className={`mt-1 truncate text-base ${
          mono ? "font-mono text-[var(--fg-primary)]" : "text-[var(--fg-primary)]"
        } ${valueClassName ?? ""}`}
      >
        {value}
      </div>
      {hint && <div className="mt-0.5 text-[10px] text-[var(--fg-muted)]">{hint}</div>}
    </div>
  );
}
