import { ArrowLeft, ArrowsClockwise, Crosshair, Target } from "@phosphor-icons/react";
import { useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api/client";
import { BiasDisplay } from "../components/BiasDisplay";
import { MultiSparkline } from "../components/MultiSparkline";
import { PerDeviceConfig } from "../components/PerDeviceConfig";
import { QuaternionDisplay } from "../components/QuaternionDisplay";
import { Sparkline } from "../components/Sparkline";
import { StatusBadge } from "../components/StatusBadge";
import { TrackerViz } from "../components/TrackerViz";
import { macHex } from "../lib/macFormat";
import { useBiasStore } from "../stores/useBiasStore";
import { useDeviceStore } from "../stores/useDeviceStore";
import { useImuStreamStore } from "../stores/useImuStreamStore";
import { useMetricsStore } from "../stores/useMetricsStore";
import { useTrackerStore } from "../stores/useTrackerStore";

const AXIS_COLOR = ["#e57373", "#81c784", "#64b5f6"];

export function TrackerDetailPage() {
  const params = useParams<{ macKey: string }>();
  const macKey = params.macKey ?? "";
  const navigate = useNavigate();
  const trackers = useTrackerStore((s) => s.trackers);
  const devices = useDeviceStore((s) => s.devices);
  const histByMac = useMetricsStore((s) => s.perMacHist);
  const imuHist = useImuStreamStore((s) => s.perMac[macKey]);
  const bias = useBiasStore((s) => s.perMac[macKey]);
  const snap = trackers[macKey];
  const dev = devices[macKey];

  const [rotationDeg, setRotationDeg] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const macBytes = useMemo<[number, number, number, number, number, number] | null>(() => {
    if (snap) return snap.mac;
    if (dev) return dev.mac;
    if (macKey.length === 12) {
      const out: number[] = [];
      for (let i = 0; i < 12; i += 2) {
        out.push(Number.parseInt(macKey.slice(i, i + 2), 16));
      }
      return out as [number, number, number, number, number, number];
    }
    return null;
  }, [snap, dev, macKey]);

  async function send(action: "yaw" | "full" | "mounting", label: string) {
    if (!macBytes) return;
    setBusy(action);
    setMsg(null);
    try {
      const res = await api.requestReset(macBytes, action);
      setMsg(res.status === "ok" ? `${label} sent` : `Error: ${JSON.stringify(res.error)}`);
    } catch (e) {
      setMsg(`Error: ${e}`);
    } finally {
      setBusy(null);
    }
  }

  async function applyRotation() {
    if (!macBytes) return;
    setBusy("rotation");
    setMsg(null);
    try {
      const res = await api.setDeviceRotationOffset(macBytes, rotationDeg);
      setMsg(
        res.status === "ok" ? "Rotation offset applied" : `Error: ${JSON.stringify(res.error)}`,
      );
    } catch (e) {
      setMsg(`Error: ${e}`);
    } finally {
      setBusy(null);
    }
  }

  if (!macBytes) {
    return (
      <div className="text-sm text-[var(--fg-muted)]">
        Unknown tracker.{" "}
        <Link to="/devices" className="text-[var(--accent)] underline">
          Back to devices
        </Link>
      </div>
    );
  }

  const macLabel = macHex(macBytes);
  const targetHz = dev?.native_imu_rate_hz ?? 200;
  const battery =
    snap && Number.isFinite(snap.battery_fraction) ? Math.round(snap.battery_fraction * 100) : null;
  const rates = histByMac[macKey]?.rates ?? [];

  const gyrSeries =
    imuHist?.gyr.map((vals, i) => ({
      values: vals,
      color: AXIS_COLOR[i] ?? "#888",
      label: `gyr ${"xyz"[i]}`,
    })) ?? [];
  const accSeries =
    imuHist?.acc.map((vals, i) => ({
      values: vals,
      color: AXIS_COLOR[i] ?? "#888",
      label: `acc ${"xyz"[i]}`,
    })) ?? [];

  return (
    <div className="flex flex-col gap-6">
      <button
        type="button"
        onClick={() => navigate(-1)}
        className="flex items-center gap-1 text-xs text-[var(--fg-muted)] hover:text-[var(--accent)]"
      >
        <ArrowLeft size={14} /> back
      </button>

      <div className="flex flex-wrap items-center gap-4">
        <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-2">
          <TrackerViz quat={snap?.quat_xyzw ?? [0, 0, 0, 1]} />
        </div>
        <div className="flex flex-col gap-1">
          <div className="font-mono text-lg text-[var(--fg-primary)]">{macLabel}</div>
          <div className="text-sm text-[var(--fg-secondary)]">
            {dev?.kind ?? "—"} · {dev?.serial ?? snap?.serial ?? "—"}
          </div>
          <div className="flex items-center gap-2 pt-1">
            <StatusBadge rateHz={snap?.rate_hz ?? 0} targetHz={targetHz} />
            {battery !== null && (
              <span className="text-xs text-[var(--fg-secondary)]">battery {battery}%</span>
            )}
            {dev?.has_magnetometer && <Pill>magnetometer</Pill>}
            {dev?.has_rumble && <Pill>rumble</Pill>}
            {dev?.firmware && <Pill>{dev.firmware}</Pill>}
          </div>
        </div>
        <Sparkline values={rates} width={160} height={40} />
      </div>

      <Section title="Orientation">
        <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4">
          <QuaternionDisplay quat={snap?.quat_xyzw ?? [0, 0, 0, 1]} />
        </div>
      </Section>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Section title="Gyroscope (rad/s)">
          <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-3">
            {gyrSeries.length === 0 || gyrSeries.every((s) => s.values.length === 0) ? (
              <Empty>Waiting for samples…</Empty>
            ) : (
              <MultiSparkline series={gyrSeries} width={400} height={90} />
            )}
          </div>
        </Section>
        <Section title="Accelerometer (m/s²)">
          <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-3">
            {accSeries.length === 0 || accSeries.every((s) => s.values.length === 0) ? (
              <Empty>Waiting for samples…</Empty>
            ) : (
              <MultiSparkline series={accSeries} width={400} height={90} />
            )}
          </div>
        </Section>
      </div>

      <Section title="Live VQF gyro bias">
        <BiasDisplay bias={bias ?? null} />
      </Section>

      <Section title="Per-device configuration">
        <PerDeviceConfig mac={macBytes} />
      </Section>

      <Section title="Reset actions">
        <div className="flex flex-wrap gap-2">
          <ActionButton
            disabled={busy !== null}
            label="Yaw Reset"
            icon={<Crosshair size={16} />}
            onClick={() => void send("yaw", "Yaw reset")}
          />
          <ActionButton
            disabled={busy !== null}
            label="Full Reset"
            icon={<ArrowsClockwise size={16} />}
            onClick={() => void send("full", "Full reset")}
          />
          <ActionButton
            disabled={busy !== null}
            label="Mounting Calibrate"
            icon={<Target size={16} />}
            onClick={() => void send("mounting", "Mounting calibrate")}
          />
        </div>
      </Section>

      <Section title="Rotation offset">
        <div className="flex flex-col gap-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4">
          <div className="flex items-center gap-3">
            <input
              type="range"
              min={-180}
              max={180}
              step={1}
              value={rotationDeg}
              onChange={(e) => setRotationDeg(Number.parseInt(e.target.value, 10))}
              className="flex-1 accent-[var(--accent)]"
            />
            <span className="w-16 text-right font-mono text-sm text-[var(--fg-primary)]">
              {rotationDeg}°
            </span>
          </div>
          <div className="flex flex-wrap gap-2">
            {[-90, 0, 90, 180].map((d) => (
              <button
                key={d}
                type="button"
                onClick={() => setRotationDeg(d)}
                className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-2 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
              >
                {d}°
              </button>
            ))}
            <button
              type="button"
              disabled={busy !== null}
              onClick={() => void applyRotation()}
              className="ml-auto rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 py-1 text-xs font-semibold text-[var(--fg-inverse)] transition-colors hover:bg-[var(--accent-bright)] disabled:opacity-50"
            >
              {busy === "rotation" ? "applying…" : "apply"}
            </button>
          </div>
          <div className="text-[11px] text-[var(--fg-muted)]">
            Offset is applied client-side to outgoing rotation packets. Body assignment and mounting
            model live on SlimeVR-Server.
          </div>
        </div>
      </Section>

      {msg && <div className="text-xs text-[var(--fg-secondary)]">{msg}</div>}
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

function ActionButton({
  label,
  icon,
  onClick,
  disabled,
}: {
  label: string;
  icon: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-2 text-sm text-[var(--fg-primary)] transition-colors hover:bg-[var(--warn-soft)] hover:text-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-50"
    >
      <span className="text-[var(--accent)]">{icon}</span>
      {label}
    </button>
  );
}

function Pill({ children }: { children: React.ReactNode }) {
  return (
    <span className="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-[10px] uppercase tracking-wide text-[var(--fg-secondary)]">
      {children}
    </span>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-dashed border-[var(--border-subtle)] p-4 text-center text-xs text-[var(--fg-muted)]">
      {children}
    </div>
  );
}
