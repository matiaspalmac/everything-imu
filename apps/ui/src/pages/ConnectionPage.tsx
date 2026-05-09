import { ActivityFeed } from "../components/ActivityFeed";
import { ConnectionStatusCard } from "../components/ConnectionStatusCard";
import { Sparkline } from "../components/Sparkline";
import { macHex, macKey } from "../lib/macFormat";
import { useMetricsStore } from "../stores/useMetricsStore";
import { useTrackerStore } from "../stores/useTrackerStore";

export function ConnectionPage() {
  const trackers = useTrackerStore((s) => s.trackers);
  const histByMac = useMetricsStore((s) => s.perMacHist);
  const list = Object.values(trackers);

  return (
    <div className="flex flex-col gap-6">
      <Section title="Connection">
        <ConnectionStatusCard />
      </Section>

      <Section title="Per-tracker rate">
        {list.length === 0 ? (
          <Empty>No trackers yet.</Empty>
        ) : (
          <div className="flex flex-col divide-y divide-[var(--border-subtle)] overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)]">
            {list.map((t) => {
              const macStr = macHex(t.mac);
              const k = macKey(t.mac);
              const hist = histByMac[k];
              return (
                <div key={macStr} className="flex items-center gap-4 px-3 py-2 text-sm">
                  <span className="w-44 truncate font-mono text-[var(--fg-primary)]">{macStr}</span>
                  <span className="w-32 truncate text-[var(--fg-secondary)]">{t.serial}</span>
                  <Sparkline values={hist?.rates ?? []} />
                  <span className="ml-auto font-mono text-[var(--fg-secondary)]">
                    {t.rate_hz.toFixed(0)} Hz
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </Section>

      <Section title="Activity">
        <ActivityFeed />
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-[var(--radius-md)] border border-dashed border-[var(--border-subtle)] p-6 text-center text-sm text-[var(--fg-muted)]">
      {children}
    </div>
  );
}
