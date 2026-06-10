import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Mac, MagCalibrationDto, MagCalProgressDto } from "../../api/client";
import { api } from "../../api/client";

/// Coverage at which the fit is geometrically meaningful and "Finish" unlocks.
const FINISH_THRESHOLD = 0.7;
const POLL_MS = 150;

type Phase = "idle" | "running" | "error";

/// Classify a finished calibration into a quality verdict for the user.
function verdict(cal: MagCalibrationDto): "good" | "marginal" | "poor" {
  const rel = cal.field_strength_ut > 0 ? cal.residual / cal.field_strength_ut : 1;
  if (cal.coverage >= 0.85 && rel < 0.08) return "good";
  if (cal.coverage < FINISH_THRESHOLD || rel > 0.2) return "poor";
  return "marginal";
}

/// Hard-iron magnetometer calibration: a guided rotate-the-device session
/// that fits the sphere centre, then persists it. Only rendered for devices
/// that actually carry a magnetometer.
export function MagCalibrationPanel({ mac }: { mac: Mac }) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("idle");
  const [cal, setCal] = useState<MagCalibrationDto | null>(null);
  const [progress, setProgress] = useState<MagCalProgressDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pollRef = useRef<number | null>(null);

  const reloadCal = useCallback(async () => {
    const res = await api.getMagCalibration(mac);
    if (res.status === "ok") setCal(res.data);
  }, [mac]);

  useEffect(() => {
    void reloadCal();
  }, [reloadCal]);

  const stopPolling = useCallback(() => {
    if (pollRef.current !== null) {
      window.clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  useEffect(() => stopPolling, [stopPolling]);

  async function start() {
    setBusy(true);
    setError(null);
    try {
      const res = await api.startMagCalibration(mac);
      if (res.status !== "ok" || !res.data) {
        setError(t("mag_cal.err_start"));
        return;
      }
      setProgress(null);
      setPhase("running");
      pollRef.current = window.setInterval(async () => {
        const p = await api.getMagCalProgress(mac);
        if (p.status === "ok") setProgress(p.data);
      }, POLL_MS);
    } finally {
      setBusy(false);
    }
  }

  async function finish() {
    setBusy(true);
    stopPolling();
    try {
      const res = await api.finishMagCalibration(mac);
      if (res.status === "ok") {
        setCal(res.data);
        setPhase("idle");
        setProgress(null);
      } else {
        setError("message" in res.error ? res.error.message : res.error.type);
        setPhase("error");
      }
    } finally {
      setBusy(false);
    }
  }

  async function cancel() {
    setBusy(true);
    stopPolling();
    try {
      await api.cancelMagCalibration(mac);
      setPhase("idle");
      setProgress(null);
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    setBusy(true);
    try {
      await api.clearMagCalibration(mac);
      await reloadCal();
    } finally {
      setBusy(false);
    }
  }

  if (phase === "running") {
    const coverage = progress?.coverage ?? 0;
    const canFinish = coverage >= FINISH_THRESHOLD;
    return (
      <div className="flex flex-col gap-3 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] p-3">
        <p className="text-[11px] text-[var(--fg-secondary)]">{t("mag_cal.instruction")}</p>
        <div className="flex items-center gap-4">
          <CoverageRing fraction={coverage} />
          <div className="flex flex-col gap-1 text-[11px] text-[var(--fg-muted)]">
            <span>{t("mag_cal.samples", { n: progress?.n_samples ?? 0 })}</span>
            <span>
              {t("mag_cal.field", {
                ut: (progress?.field_strength_ut ?? 0).toFixed(1),
              })}
            </span>
            <span>{t("mag_cal.coverage", { pct: Math.round(coverage * 100) })}</span>
          </div>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            disabled={!canFinish || busy}
            onClick={() => void finish()}
            className="rounded-[var(--radius-sm)] border border-[var(--accent)] bg-[var(--warn-soft)] px-3 py-1.5 text-xs text-[var(--accent)] disabled:opacity-40"
          >
            {t("mag_cal.finish")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void cancel()}
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] disabled:opacity-40"
          >
            {t("mag_cal.cancel")}
          </button>
        </div>
        {!canFinish && (
          <p className="text-[10px] text-[var(--fg-muted)]">{t("mag_cal.keep_rotating")}</p>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] p-3">
      {cal ? (
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            <span className="rounded-full bg-[var(--success-soft)] px-2 py-0.5 text-[10px] text-[var(--success)]">
              {t("mag_cal.status_calibrated")}
            </span>
            <span className="text-[10px] text-[var(--fg-muted)]">
              {t(`mag_cal.verdict_${verdict(cal)}`)}
            </span>
          </div>
          <span className="text-[10px] text-[var(--fg-muted)]">
            {t("mag_cal.detail", {
              ut: cal.field_strength_ut.toFixed(1),
              res: cal.residual.toFixed(2),
              pct: Math.round(cal.coverage * 100),
            })}
          </span>
        </div>
      ) : (
        <span className="rounded-full bg-[var(--bg-panel)] px-2 py-0.5 text-[10px] text-[var(--fg-muted)] self-start">
          {t("mag_cal.status_uncalibrated")}
        </span>
      )}
      {phase === "error" && error && <p className="text-[10px] text-[var(--danger)]">{error}</p>}
      <div className="flex gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => void start()}
          className="rounded-[var(--radius-sm)] border border-[var(--accent)] bg-[var(--warn-soft)] px-3 py-1.5 text-xs text-[var(--accent)] disabled:opacity-40"
        >
          {cal ? t("mag_cal.recalibrate") : t("mag_cal.calibrate")}
        </button>
        {cal && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void clear()}
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] disabled:opacity-40"
          >
            {t("mag_cal.clear")}
          </button>
        )}
      </div>
    </div>
  );
}

/// SVG progress ring driven by a 0..1 fraction.
function CoverageRing({ fraction }: { fraction: number }) {
  const r = 26;
  const c = 2 * Math.PI * r;
  const clamped = Math.max(0, Math.min(1, fraction));
  return (
    <svg width={64} height={64} viewBox="0 0 64 64" aria-hidden="true">
      <circle cx={32} cy={32} r={r} fill="none" stroke="var(--border-subtle)" strokeWidth={6} />
      <circle
        cx={32}
        cy={32}
        r={r}
        fill="none"
        stroke="var(--accent)"
        strokeWidth={6}
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - clamped)}
        transform="rotate(-90 32 32)"
      />
      <text
        x={32}
        y={36}
        textAnchor="middle"
        className="fill-[var(--fg-primary)] text-[13px] font-semibold"
      >
        {Math.round(clamped * 100)}%
      </text>
    </svg>
  );
}
