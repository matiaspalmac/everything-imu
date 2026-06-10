import { Lightning } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { macKey } from "../../lib/macFormat";
import { useLatencyStore } from "../../stores/useLatencyStore";

type Props = {
  mac: number[] | Uint8Array;
  compact?: boolean;
};

function fmtUs(us: number): string {
  if (!Number.isFinite(us) || us <= 0) return "—";
  if (us >= 1000) return `${(us / 1000).toFixed(1)} ms`;
  return `${Math.round(us)} µs`;
}

/**
 * Bridge-only telemetry: how steadily and how fast the bridge ingests
 * driver batches and pushes them to SlimeVR-Server. Not motion data, not
 * fusion quality — just plumbing health.
 *
 * Values originate from `LatencyTracker` in `crates/core/src/latency.rs`
 * and arrive at 1 Hz via `LatencyUpdate`.
 */
export function LatencyPanel({ mac, compact }: Props) {
  const { t } = useTranslation();
  const entry = useLatencyStore((s) => s.perMac[macKey(mac)]);

  if (!entry || entry.samples_window === 0) {
    return (
      <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-3 text-[12px] text-[var(--fg-muted)]">
        {t("latency.waiting")}
      </div>
    );
  }

  const intervals: Array<[string, number]> = [
    [t("latency.p50"), entry.interval_us_p50],
    [t("latency.p95"), entry.interval_us_p95],
    [t("latency.p99"), entry.interval_us_p99],
  ];

  return (
    <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-3 text-[12px]">
      <header className="mb-2 flex items-center gap-1.5 text-[var(--fg-secondary)]">
        <Lightning size={13} weight="duotone" className="text-[var(--accent)]" />
        <span className="font-semibold">{t("latency.title")}</span>
        <span className="ml-auto text-[10px] text-[var(--fg-muted)]">
          {t("latency.window", { n: entry.samples_window })}
        </span>
      </header>

      <section className="mb-3">
        <div className="mb-1 text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
          {t("latency.interval_label")}
        </div>
        <div className="grid grid-cols-3 gap-2">
          {intervals.map(([label, value]) => (
            <div
              key={label}
              className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1"
            >
              <div className="text-[10px] text-[var(--fg-muted)]">{label}</div>
              <div className="font-mono text-[13px] text-[var(--fg-primary)]">{fmtUs(value)}</div>
            </div>
          ))}
        </div>
      </section>

      {!compact && (
        <section className="mb-3 grid grid-cols-2 gap-2">
          <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1">
            <div className="text-[10px] text-[var(--fg-muted)]">{t("latency.jitter")}</div>
            <div className="font-mono text-[13px] text-[var(--fg-primary)]">
              {fmtUs(entry.jitter_us)}
            </div>
          </div>
          <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1">
            <div className="text-[10px] text-[var(--fg-muted)]">{t("latency.dropped")}</div>
            <div
              className={`font-mono text-[13px] ${
                entry.dropped_estimate > 0 ? "text-[var(--warn)]" : "text-[var(--fg-primary)]"
              }`}
            >
              {entry.dropped_estimate}
            </div>
          </div>
        </section>
      )}

      <section>
        <div className="mb-1 text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
          {t("latency.send_label")}
        </div>
        <div className="grid grid-cols-2 gap-2">
          <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1">
            <div className="text-[10px] text-[var(--fg-muted)]">{t("latency.p50")}</div>
            <div className="font-mono text-[13px] text-[var(--fg-primary)]">
              {fmtUs(entry.send_us_p50)}
            </div>
          </div>
          <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1">
            <div className="text-[10px] text-[var(--fg-muted)]">{t("latency.p95")}</div>
            <div className="font-mono text-[13px] text-[var(--fg-primary)]">
              {fmtUs(entry.send_us_p95)}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

/**
 * Compact aggregate summary across every tracker for the Dashboard tile.
 * Shows worst-case p95 interval + total dropped batches.
 */
export function LatencySummary() {
  const { t } = useTranslation();
  const perMac = useLatencyStore((s) => s.perMac);
  const entries = Object.values(perMac);
  const ready = entries.filter((e) => e.samples_window > 0);

  if (ready.length === 0) {
    return <div className="py-2 text-[12px] text-[var(--fg-muted)]">{t("latency.waiting")}</div>;
  }

  const worstP95 = ready.reduce((m, e) => Math.max(m, e.interval_us_p95), 0);
  const meanJitter = ready.reduce((s, e) => s + e.jitter_us, 0) / ready.length;
  const totalDropped = ready.reduce((s, e) => s + e.dropped_estimate, 0);

  return (
    <div className="flex h-full flex-col justify-between gap-3">
      <header className="flex items-center gap-1.5 text-[12px] text-[var(--fg-secondary)]">
        <Lightning size={13} weight="duotone" className="text-[var(--accent)]" />
        <span className="font-semibold">{t("latency.summary_title")}</span>
        <span className="ml-auto text-[10px] text-[var(--fg-muted)]">
          {t("latency.across", { n: ready.length })}
        </span>
      </header>
      <div className="grid grid-cols-3 gap-2">
        <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] p-3">
          <div className="text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
            {t("latency.worst_p95")}
          </div>
          <div className="metric-num mt-1 font-mono text-base text-[var(--fg-primary)]">
            {fmtUs(worstP95)}
          </div>
        </div>
        <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] p-3">
          <div className="text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
            {t("latency.jitter_mean")}
          </div>
          <div className="metric-num mt-1 font-mono text-base text-[var(--fg-primary)]">
            {fmtUs(meanJitter)}
          </div>
        </div>
        <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] p-3">
          <div className="text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
            {t("latency.dropped_total")}
          </div>
          <div
            className={`metric-num mt-1 font-mono text-base ${
              totalDropped > 0 ? "text-[var(--warn)]" : "text-[var(--fg-primary)]"
            }`}
          >
            {totalDropped}
          </div>
        </div>
      </div>
    </div>
  );
}
