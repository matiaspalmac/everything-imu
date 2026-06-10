import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useConnectionStore } from "../../stores/useConnectionStore";
import { useTrackerStore } from "../../stores/useTrackerStore";

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

  const live = status?.last_send_ms_ago != null && status.last_send_ms_ago < 2000;
  const statusBadge = !status
    ? { label: t("conn.starting"), tone: "var(--fg-muted)" }
    : live
      ? { label: t("status.live"), tone: "var(--success)" }
      : list.length > 0
        ? { label: t("status.stalled"), tone: "var(--warn)" }
        : { label: t("status.idle"), tone: "var(--fg-muted)" };

  // Handshake ring progress: SlimeVR-Server is expected to PING the
  // bridge ~1 Hz. We render the time-since-last-handshake as a
  // shrinking ring; when it crosses 2 s we recolor to warn. Pure
  // visual feedback, no semantic change.
  const handshakeMs = status?.last_handshake_ms_ago ?? null;
  const handshakeProgress = handshakeMs == null ? 0 : Math.min(1, handshakeMs / 2000);
  const handshakeTone =
    handshakeMs == null ? "var(--fg-muted)" : handshakeMs > 2000 ? "var(--warn)" : "var(--success)";

  return (
    <div className="flex flex-col gap-4">
      {/* Hero row — ring on the left, identity strip on the right */}
      <div className="flex items-center gap-4">
        <HandshakeRing
          progress={handshakeProgress}
          tone={handshakeTone}
          label={statusBadge.label}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="metric-num truncate font-mono text-lg text-[var(--fg-primary)]">
            {status?.server_addr ?? "—"}
          </div>
          <div className="text-[10px] uppercase tracking-[0.16em] text-[var(--fg-muted)]">
            {t("conn.slime_target")}
          </div>
          <div className="mt-1 flex items-center gap-2 text-[11px] text-[var(--fg-secondary)]">
            <span
              className="inline-block size-1.5 rounded-full"
              style={{ background: statusBadge.tone }}
            />
            <span style={{ color: statusBadge.tone }} className="font-semibold">
              {statusBadge.label}
            </span>
            <span className="text-[var(--fg-muted)]">·</span>
            <span>
              {liveCount} / {list.length} {t("conn.live_trackers").toLowerCase()}
            </span>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 lg:grid-cols-3">
        <Stat
          label={t("conn.bundle_mode")}
          value={status?.server_supports_bundle ? t("conn.bundle") : t("conn.individual")}
          hint={
            status?.server_supports_bundle
              ? t("conn.feature_flags_ok")
              : t("conn.fallback_two_send")
          }
        />
        <Stat label={t("conn.packets_sent")} value={(status?.packets_sent ?? 0).toLocaleString()} />
        <Stat label={t("conn.last_send")} value={formatMsAgo(status?.last_send_ms_ago)} />
        <Stat label={t("conn.last_handshake")} value={formatMsAgo(status?.last_handshake_ms_ago)} />
      </div>
    </div>
  );
}

function HandshakeRing({
  progress,
  tone,
  label,
}: {
  progress: number; // 0 = freshly received, 1 = 2 s elapsed
  tone: string;
  label: string;
}) {
  // SVG ring math: stroke-dasharray = circumference; dashoffset shrinks
  // as `progress` grows so the visible arc represents time-remaining.
  const size = 56;
  const r = 24;
  const c = 2 * Math.PI * r;
  const offset = c * progress;
  return (
    <div className="relative grid size-14 place-items-center">
      <svg width={size} height={size} className="absolute inset-0 -rotate-90">
        <title>{label}</title>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="var(--border-subtle)"
          strokeWidth={3}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={tone}
          strokeWidth={3}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={offset}
          style={{ transition: "stroke-dashoffset 0.5s linear, stroke 0.3s ease" }}
        />
      </svg>
      <span
        className="metric-num relative text-[10px] font-semibold uppercase tracking-wide"
        style={{ color: tone }}
      >
        {label.slice(0, 4)}
      </span>
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
    <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] p-3">
      <div className="text-[10px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
        {label}
      </div>
      <div
        className={`metric-num mt-1 truncate text-base font-medium ${
          mono ? "font-mono" : ""
        } text-[var(--fg-primary)] ${valueClassName ?? ""}`}
      >
        {value}
      </div>
      {hint && <div className="mt-0.5 text-[10px] text-[var(--fg-muted)]">{hint}</div>}
    </div>
  );
}
