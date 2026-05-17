import { Pulse } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { ActivityFeed } from "../components/ActivityFeed";
import { ConnectionStatusCard } from "../components/ConnectionStatusCard";
import { EmptyState } from "../components/EmptyState";
import { LatencySummary } from "../components/LatencyPanel";
import { Sparkline } from "../components/Sparkline";
import { macHex, macKey } from "../lib/macFormat";
import { useConnectionStore } from "../stores/useConnectionStore";
import { useMetricsStore } from "../stores/useMetricsStore";
import { useTrackerStore } from "../stores/useTrackerStore";

export function ConnectionPage() {
  const { t } = useTranslation();
  const trackers = useTrackerStore((s) => s.trackers);
  const histByMac = useMetricsStore((s) => s.perMacHist);
  const status = useConnectionStore((s) => s.status);
  const list = Object.values(trackers);

  const meanRate = list.length === 0 ? 0 : list.reduce((s, t) => s + t.rate_hz, 0) / list.length;
  const totalPackets = status?.packets_sent ?? 0;

  return (
    <div className="flex flex-col gap-5">
      <header className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("pages.connection")}
        </h2>
        <div className="flex items-center gap-3 text-[11px] text-[var(--fg-muted)]">
          <span>{t("status.tracker_count_plural", { count: list.length })}</span>
        </div>
      </header>

      {/* Bento: status hero (span 2 + feature halo) + at-a-glance stat tiles */}
      <div className="grid auto-rows-min grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Tile span={2} feature title={t("pages.connection")}>
          <ConnectionStatusCard />
        </Tile>
        <Tile title={t("conn.live_trackers")}>
          <Stat
            value={String(list.length)}
            hint={t("status.tracker_count_plural", { count: list.length })}
          />
        </Tile>
        <Tile title={t("status.live")}>
          <Stat value={`${meanRate.toFixed(0)} Hz`} hint={t("conn.mean_rate")} />
        </Tile>
        <Tile span={2} title={t("pages.bridge_latency")}>
          <LatencySummary />
        </Tile>
        <Tile title={t("conn.packets_sent")}>
          <Stat value={fmtCount(totalPackets)} hint="udp" />
        </Tile>
        <Tile title={t("conn.last_send")}>
          <Stat
            value={
              status?.last_send_ms_ago != null
                ? `${(status.last_send_ms_ago / 1000).toFixed(1)} s`
                : "—"
            }
            hint={t("conn.since_last")}
          />
        </Tile>
      </div>

      <Tile title={t("pages.per_tracker_rate")}>
        {list.length === 0 ? (
          <EmptyState
            icon={Pulse}
            title={t("empty.no_rates_title")}
            description={t("empty.no_rates_desc")}
            compact
          />
        ) : (
          <div className="flex flex-col divide-y divide-[var(--border-subtle)]">
            {list.map((tr) => {
              const macStr = macHex(tr.mac);
              const k = macKey(tr.mac);
              const hist = histByMac[k];
              return (
                <div key={macStr} className="flex items-center gap-4 py-2 text-sm">
                  <span className="metric-num w-44 truncate font-mono text-[var(--fg-primary)]">
                    {macStr}
                  </span>
                  <span className="w-32 truncate text-[var(--fg-secondary)]">{tr.serial}</span>
                  <Sparkline values={hist?.rates ?? []} />
                  <span className="metric-num ml-auto font-mono text-[var(--fg-secondary)]">
                    {tr.rate_hz.toFixed(0)} Hz
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </Tile>

      <Tile title={t("pages.activity")}>
        <ActivityFeed />
      </Tile>
    </div>
  );
}

function Tile({
  title,
  children,
  span,
  feature,
}: {
  title: string;
  children: React.ReactNode;
  span?: 1 | 2 | 3 | 4;
  feature?: boolean;
}) {
  const spanCls =
    span === 4 ? "lg:col-span-4" : span === 3 ? "lg:col-span-3" : span === 2 ? "lg:col-span-2" : "";
  const featureCls = feature
    ? "border-[var(--accent-soft)] shadow-[var(--shadow-card)] before:absolute before:inset-x-0 before:top-0 before:h-[2px] before:bg-gradient-to-r before:from-transparent before:via-[var(--accent)] before:to-transparent before:opacity-60 before:content-['']"
    : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]";
  return (
    <section
      className={`relative flex flex-col gap-3 overflow-hidden rounded-[var(--radius-lg)] border bg-[var(--bg-panel)] p-4 ${spanCls} ${featureCls}`}
    >
      <h3 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
        {title}
      </h3>
      <div className="min-w-0 flex-1">{children}</div>
    </section>
  );
}

function Stat({ value, hint }: { value: string; hint?: string }) {
  return (
    <div className="flex flex-col">
      <span className="metric-num text-2xl font-semibold text-[var(--fg-primary)]">{value}</span>
      {hint && (
        <span className="text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">{hint}</span>
      )}
    </div>
  );
}

function fmtCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1000_000).toFixed(2)}M`;
}
