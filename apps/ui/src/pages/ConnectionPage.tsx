import { Pulse } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "../components/ui/EmptyState";
import { Sparkline } from "../components/ui/Sparkline";
import { ActivityFeed } from "../components/widgets/ActivityFeed";
import { ConnectionStatusCard } from "../components/widgets/ConnectionStatusCard";
import { LatencySummary } from "../components/widgets/LatencyPanel";
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
    <div className="flex flex-col gap-6">
      <header className="flex items-end justify-between gap-3">
        <h1 className="text-xl font-semibold tracking-tight text-[var(--fg-primary)]">
          {t("pages.connection")}
        </h1>
        <span className="rounded-full border border-[var(--border-subtle)] px-3 py-1 text-[11px] text-[var(--fg-secondary)]">
          {t("status.tracker_count_plural", { count: list.length })}
        </span>
      </header>

      {/* Bento: status hero (span 2 + elevated) + at-a-glance stat tiles */}
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
          <div className="flex flex-col">
            {list.map((tr) => {
              const macStr = macHex(tr.mac);
              const k = macKey(tr.mac);
              const hist = histByMac[k];
              const stalled = tr.rate_hz <= 0;
              return (
                <div
                  key={macStr}
                  className="flex items-center gap-4 rounded-[var(--radius-md)] px-2 py-2.5 text-sm transition-colors hover:bg-[var(--bg-elevated)]"
                >
                  <span
                    aria-hidden
                    className="size-1.5 shrink-0 rounded-full"
                    style={{ background: stalled ? "var(--fg-muted)" : "var(--success)" }}
                  />
                  <span className="metric-num min-w-0 flex-1 truncate font-mono text-[var(--fg-primary)] sm:w-44 sm:flex-none">
                    {macStr}
                  </span>
                  <span className="hidden w-32 truncate text-[var(--fg-secondary)] md:block">
                    {tr.serial}
                  </span>
                  <span className="hidden sm:block">
                    <Sparkline values={hist?.rates ?? []} />
                  </span>
                  <span
                    className={`metric-num ml-auto font-mono ${
                      stalled ? "text-[var(--fg-muted)]" : "text-[var(--fg-primary)]"
                    }`}
                  >
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
    ? "border-[var(--border-strong)] bg-[var(--bg-elevated)] before:absolute before:inset-x-0 before:top-0 before:h-[2px] before:bg-[var(--accent)] before:content-['']"
    : "border-[var(--border-subtle)] bg-[var(--bg-panel)] hover:border-[var(--border-strong)]";
  return (
    <section
      className={`relative flex flex-col gap-3 overflow-hidden rounded-[var(--radius-xl)] border p-5 transition-colors ${spanCls} ${featureCls}`}
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
    <div className="flex h-full flex-col justify-end gap-1">
      <span className="metric-num text-[28px] font-semibold leading-none tracking-tight text-[var(--fg-primary)]">
        {value}
      </span>
      {hint && <span className="text-[11px] text-[var(--fg-muted)]">{hint}</span>}
    </div>
  );
}

function fmtCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1000_000).toFixed(2)}M`;
}
