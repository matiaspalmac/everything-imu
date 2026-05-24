import { useTranslation } from "react-i18next";
import { macHex } from "../lib/macFormat";
import { useBiasStore } from "../stores/useBiasStore";
import { useConnectionStore } from "../stores/useConnectionStore";
import { useDeviceStore } from "../stores/useDeviceStore";
import { useImuStreamStore } from "../stores/useImuStreamStore";
import { useLatencyStore } from "../stores/useLatencyStore";
import { useTrackerStore } from "../stores/useTrackerStore";

/// Developer-facing diagnostics. Surfaces raw values straight out of the
/// existing stores — no fresh subscriptions, no new IPC — so it doubles
/// as a fast-loading sanity check for the event pipeline. Three sections:
/// bridge / per-device / outgoing packet summary.
export function DebugPage() {
  const { t } = useTranslation();
  const trackers = useTrackerStore((s) => s.trackers);
  const imuPerMac = useImuStreamStore((s) => s.perMac);
  const biasPerMac = useBiasStore((s) => s.perMac);
  const latencyPerMac = useLatencyStore((s) => s.perMac);
  const devices = useDeviceStore((s) => s.devices);
  const conn = useConnectionStore((s) => s.status);

  const macs = Object.keys(devices);

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-1">
        <h2 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("debug.title")}
        </h2>
        <span className="text-[11px] text-[var(--fg-muted)]">{t("debug.body")}</span>
      </header>

      <section className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-3">
        <h3 className="pb-2 text-[11px] uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("debug.bridge")}
        </h3>
        <dl className="grid grid-cols-2 gap-x-6 gap-y-1 text-[11px]">
          <dt className="text-[var(--fg-muted)]">server_addr</dt>
          <dd className="metric-num font-mono text-[var(--fg-primary)]">
            {conn?.server_addr ?? "—"}
          </dd>
          <dt className="text-[var(--fg-muted)]">bundle</dt>
          <dd className="metric-num font-mono text-[var(--fg-primary)]">
            {conn ? String(conn.server_supports_bundle) : "—"}
          </dd>
          <dt className="text-[var(--fg-muted)]">packets_sent</dt>
          <dd className="metric-num font-mono text-[var(--fg-primary)]">
            {conn?.packets_sent ?? 0}
          </dd>
          <dt className="text-[var(--fg-muted)]">last_send_ms_ago</dt>
          <dd className="metric-num font-mono text-[var(--fg-primary)]">
            {conn?.last_send_ms_ago ?? "—"}
          </dd>
          <dt className="text-[var(--fg-muted)]">last_handshake_ms_ago</dt>
          <dd className="metric-num font-mono text-[var(--fg-primary)]">
            {conn?.last_handshake_ms_ago ?? "—"}
          </dd>
        </dl>
      </section>

      {macs.length === 0 ? (
        <p className="text-[11px] text-[var(--fg-muted)]">{t("debug.empty")}</p>
      ) : (
        macs.map((k) => {
          const dev = devices[k];
          const tr = trackers[k];
          const sample = imuPerMac[k];
          const bias = biasPerMac[k];
          const lat = latencyPerMac[k];
          const lastGyr = sample?.gyr?.map((axis) => axis[axis.length - 1] ?? 0) ?? [0, 0, 0];
          const lastAcc = sample?.acc?.map((axis) => axis[axis.length - 1] ?? 0) ?? [0, 0, 0];
          return (
            <section
              key={k}
              className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-3"
            >
              <h3 className="pb-2 text-[11px] uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
                {dev.kind} · <span className="font-mono">{macHex(dev.mac)}</span>
              </h3>
              <dl className="grid grid-cols-2 gap-x-6 gap-y-1 font-mono text-[11px]">
                <dt className="text-[var(--fg-muted)]">rate_hz</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {(tr?.rate_hz ?? 0).toFixed(1)}
                </dd>
                <dt className="text-[var(--fg-muted)]">battery</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {Number.isFinite(tr?.battery_fraction)
                    ? `${Math.round((tr?.battery_fraction ?? 0) * 100)}%`
                    : "—"}
                </dd>
                <dt className="text-[var(--fg-muted)]">quat (x y z w)</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {(tr?.quat_xyzw ?? [0, 0, 0, 1]).map((v) => v.toFixed(3)).join(" ")}
                </dd>
                <dt className="text-[var(--fg-muted)]">last gyro (rad/s)</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {lastGyr.map((v) => v.toFixed(3)).join(" ")}
                </dd>
                <dt className="text-[var(--fg-muted)]">last accel (m/s²)</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {lastAcc.map((v) => v.toFixed(3)).join(" ")}
                </dd>
                <dt className="text-[var(--fg-muted)]">gyro bias</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {bias ? bias.map((v) => v.toFixed(4)).join(" ") : "—"}
                </dd>
                <dt className="text-[var(--fg-muted)]">interval p50 / p95 / p99 (µs)</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {lat
                    ? `${lat.interval_us_p50.toFixed(0)} / ${lat.interval_us_p95.toFixed(0)} / ${lat.interval_us_p99.toFixed(0)}`
                    : "—"}
                </dd>
                <dt className="text-[var(--fg-muted)]">jitter / send p95 (µs)</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {lat ? `${lat.jitter_us.toFixed(0)} / ${lat.send_us_p95.toFixed(0)}` : "—"}
                </dd>
                <dt className="text-[var(--fg-muted)]">dropped / window</dt>
                <dd className="metric-num text-[var(--fg-primary)]">
                  {lat ? `${lat.dropped_estimate} / ${lat.samples_window}` : "—"}
                </dd>
              </dl>
            </section>
          );
        })
      )}
    </div>
  );
}
