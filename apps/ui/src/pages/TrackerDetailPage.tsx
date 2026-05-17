import { ArrowLeft, ArrowsClockwise, Crosshair, Target } from "@phosphor-icons/react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api/client";
import { BiasDisplay } from "../components/BiasDisplay";
import { LatencyPanel } from "../components/LatencyPanel";
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
  const { t } = useTranslation();
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
      setMsg(
        res.status === "ok"
          ? t("msg.action_sent", { action: label })
          : t("msg.error_generic", { err: JSON.stringify(res.error) }),
      );
    } catch (e) {
      setMsg(t("msg.error_generic", { err: String(e) }));
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
        res.status === "ok"
          ? t("msg.rotation_applied")
          : t("msg.error_generic", { err: JSON.stringify(res.error) }),
      );
    } catch (e) {
      setMsg(t("msg.error_generic", { err: String(e) }));
    } finally {
      setBusy(null);
    }
  }

  if (!macBytes) {
    return (
      <div className="text-sm text-[var(--fg-muted)]">
        {t("hints.unknown_tracker")}{" "}
        <Link to="/devices" className="text-[var(--accent)] underline">
          {t("hints.back_to_devices")}
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
    <div className="flex flex-col gap-5">
      <header className="flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={() => navigate(-1)}
          className="flex items-center gap-1 text-xs text-[var(--fg-muted)] hover:text-[var(--accent)]"
        >
          <ArrowLeft size={14} /> {t("actions.back")}
        </button>
        {msg && <span className="text-[11px] text-[var(--fg-secondary)]">{msg}</span>}
      </header>

      {/*
        Hero tile — 3D viz on the left, identity + status badges on the
        right, packet-rate sparkline pulling the eye to the corner. This
        is the "what am I looking at" frame for the whole page.
      */}
      <Tile feature>
        <div className="flex flex-wrap items-center gap-5">
          <div className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg)] p-2">
            <TrackerViz quat={snap?.quat_xyzw ?? [0, 0, 0, 1]} />
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <div className="metric-num font-mono text-xl text-[var(--fg-primary)]">{macLabel}</div>
            <div className="text-sm text-[var(--fg-secondary)]">
              {dev?.kind ?? "—"} · {dev?.serial ?? snap?.serial ?? "—"}
            </div>
            <div className="flex flex-wrap items-center gap-2 pt-1">
              <StatusBadge rateHz={snap?.rate_hz ?? 0} targetHz={targetHz} />
              {battery !== null && (
                <span className="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-[10px] text-[var(--fg-secondary)]">
                  {t("labels.battery", { pct: battery })}
                </span>
              )}
              {dev?.has_magnetometer && <Pill>magnetometer</Pill>}
              {dev?.has_rumble && <Pill>rumble</Pill>}
              {dev?.firmware && <Pill>{dev.firmware}</Pill>}
            </div>
          </div>
          <div className="flex flex-col items-end gap-1">
            <span className="text-[10px] uppercase tracking-[0.16em] text-[var(--fg-muted)]">
              {t("status.live")}
            </span>
            <Sparkline values={rates} width={180} height={40} />
          </div>
        </div>
      </Tile>

      {/*
        Bento grid: orientation + bias side-by-side (both small fixed
        widgets), then gyr/acc sparklines as a 2-col wide block, then
        per-device config wide. Latency + reset/rotation actions cluster
        on the right rail for at-a-glance bridge ops.
      */}
      <div className="grid auto-rows-min grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        <TileTitled title={t("pages.orientation")} span={2}>
          <QuaternionDisplay quat={snap?.quat_xyzw ?? [0, 0, 0, 1]} />
        </TileTitled>
        <TileTitled title={t("pages.live_bias")}>
          <BiasDisplay bias={bias ?? null} />
        </TileTitled>

        <TileTitled title={t("pages.gyroscope")}>
          {gyrSeries.length === 0 || gyrSeries.every((s) => s.values.length === 0) ? (
            <div className="text-center text-[10px] text-[var(--fg-muted)]">
              {t("hints.waiting_samples")}
            </div>
          ) : (
            <MultiSparkline series={gyrSeries} width={400} height={90} />
          )}
        </TileTitled>
        <TileTitled title={t("pages.accelerometer")}>
          {accSeries.length === 0 || accSeries.every((s) => s.values.length === 0) ? (
            <div className="text-center text-[10px] text-[var(--fg-muted)]">
              {t("hints.waiting_samples")}
            </div>
          ) : (
            <MultiSparkline series={accSeries} width={400} height={90} />
          )}
        </TileTitled>
        <TileTitled title={t("pages.bridge_latency")}>
          <LatencyPanel mac={macBytes} compact />
        </TileTitled>

        <TileTitled title={t("pages.per_device_config")} span={2}>
          <PerDeviceConfig mac={macBytes} />
        </TileTitled>

        <div className="flex min-w-0 flex-col gap-4">
          <TileTitled title={t("pages.reset_actions")}>
            <div className="grid grid-cols-1 gap-2">
              <ActionButton
                disabled={busy !== null}
                label={t("actions.yaw_reset")}
                icon={<Crosshair size={16} />}
                onClick={() => void send("yaw", t("actions.yaw_reset"))}
              />
              <ActionButton
                disabled={busy !== null}
                label={t("actions.full_reset")}
                icon={<ArrowsClockwise size={16} />}
                onClick={() => void send("full", t("actions.full_reset"))}
              />
              <ActionButton
                disabled={busy !== null}
                label={t("actions.mounting_calibrate")}
                icon={<Target size={16} />}
                onClick={() => void send("mounting", t("actions.mounting_calibrate"))}
              />
            </div>
          </TileTitled>

          <TileTitled title={t("pages.rotation_offset")}>
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
              <span className="metric-num w-14 text-right font-mono text-sm text-[var(--fg-primary)]">
                {rotationDeg}°
              </span>
            </div>
            <div className="flex flex-wrap gap-2 pt-2">
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
                {busy === "rotation" ? t("actions.applying") : t("actions.apply")}
              </button>
            </div>
            <div className="pt-2 text-[11px] text-[var(--fg-muted)]">
              {t("hints.rotation_offset")}
            </div>
          </TileTitled>
        </div>
      </div>
    </div>
  );
}

function Tile({ children, feature }: { children: React.ReactNode; feature?: boolean }) {
  const cls = feature
    ? "border-[var(--accent-soft)] shadow-[var(--shadow-card)] before:absolute before:inset-x-0 before:top-0 before:h-[2px] before:bg-gradient-to-r before:from-transparent before:via-[var(--accent)] before:to-transparent before:opacity-60 before:content-['']"
    : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]";
  return (
    <section
      className={`relative overflow-hidden rounded-[var(--radius-lg)] border bg-[var(--bg-panel)] p-4 ${cls}`}
    >
      {children}
    </section>
  );
}

function TileTitled({
  title,
  children,
  span,
}: {
  title: string;
  children: React.ReactNode;
  span?: 1 | 2 | 3;
}) {
  const spanCls = span === 3 ? "lg:col-span-3" : span === 2 ? "lg:col-span-2" : "";
  return (
    <section
      className={`flex min-w-0 flex-col gap-3 rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4 transition-colors hover:border-[var(--border-strong)] ${spanCls}`}
    >
      <h3 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
        {title}
      </h3>
      <div className="min-w-0 flex-1">{children}</div>
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
      className="flex items-center justify-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg-primary)] transition-colors hover:border-[var(--accent)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-50"
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
