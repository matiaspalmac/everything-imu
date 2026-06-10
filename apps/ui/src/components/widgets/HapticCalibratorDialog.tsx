import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { HapticCalibrationDto } from "../../api/bindings";
import { api } from "../../api/client";
import { useToastStore } from "../../stores/useToastStore";

/// Per-device haptic perception wizard. The user steps the intensity
/// slider, hits Test, and marks "barely felt" (sets floor) and "comfort
/// max" (sets gain). Saves on close. The floor + gain pair is persisted
/// via `set_haptic_calibration` and is then used by the OSC rumble
/// dispatcher to map raw 0..1 intensities to felt 0..1.
export function HapticCalibratorDialog({
  mac,
  onClose,
}: {
  mac: [number, number, number, number, number, number];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const pushToast = useToastStore((s) => s.push);
  const [intensity, setIntensity] = useState(0.5);
  const [cal, setCal] = useState<HapticCalibrationDto>({ floor: 0, gain: 1 });
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    const res = await api.getHapticCalibration(mac);
    if (res.status === "ok") setCal(res.data);
  }, [mac]);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function test() {
    setBusy(true);
    await api.testRumbleAt(mac, intensity, 400);
    setBusy(false);
  }

  function markFloor() {
    setCal((c) => ({ ...c, floor: intensity }));
  }

  function markCeiling() {
    // Pick a gain that maps 1.0 → the felt-max intensity. With the
    // dispatcher's `out = floor + clamp(in - floor, 0, 1) * gain` model
    // we want `out == intensity` when `in == 1`, so:
    //   gain = (intensity - floor) / (1 - floor)
    // Clamp to a sane range so a degenerate setting does not zero rumble.
    const span = Math.max(0.01, 1 - cal.floor);
    const g = Math.max(0.1, Math.min(2.0, (intensity - cal.floor) / span));
    setCal((c) => ({ ...c, gain: g }));
  }

  async function save() {
    setBusy(true);
    const res = await api.setHapticCalibration(mac, cal);
    setBusy(false);
    if (res.status === "ok") {
      pushToast({
        level: "info",
        title: t("haptic_cal.title"),
        message: t("haptic_cal.saved"),
      });
      onClose();
    } else {
      pushToast({
        level: "warn",
        title: t("haptic_cal.title"),
        message: "message" in res.error ? res.error.message : res.error.type,
      });
    }
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: modal backdrop pattern; inner <dialog> owns focus + a11y semantics, this wrapper only intercepts outside-click / Escape.
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={(e) => e.target === e.currentTarget && onClose()}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
      role="presentation"
    >
      <dialog
        open
        aria-modal="true"
        aria-label={t("haptic_cal.title")}
        className="w-full max-w-md rounded-[var(--radius-lg)] border border-[var(--border-strong)] bg-[var(--bg-panel)] p-5"
      >
        <header className="flex flex-col gap-1 pb-3">
          <h2 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
            {t("haptic_cal.title")}
          </h2>
          <p className="text-[11px] text-[var(--fg-muted)]">{t("haptic_cal.body")}</p>
        </header>

        <div className="flex flex-col gap-3 py-2">
          <div className="flex items-center gap-3">
            <input
              type="range"
              aria-label={t("haptic_cal.intensity") ?? "intensity"}
              min={0}
              max={1}
              step={0.01}
              value={intensity}
              onChange={(e) => setIntensity(Number.parseFloat(e.target.value))}
              className="flex-1 accent-[var(--accent)]"
            />
            <span className="metric-num w-14 text-right font-mono text-sm text-[var(--fg-primary)]">
              {(intensity * 100).toFixed(0)}%
            </span>
          </div>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => void test()}
              className="rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 py-1 text-xs font-semibold text-[var(--fg-inverse)] hover:bg-[var(--accent-bright)] disabled:opacity-50"
            >
              {t("haptic_cal.test")}
            </button>
            <button
              type="button"
              onClick={markFloor}
              className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
            >
              {t("haptic_cal.set_floor")}
            </button>
            <button
              type="button"
              onClick={markCeiling}
              className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
            >
              {t("haptic_cal.set_ceiling")}
            </button>
          </div>
          <dl className="grid grid-cols-2 gap-y-1 pt-1 text-[11px] text-[var(--fg-muted)]">
            <dt>{t("haptic_cal.floor")}</dt>
            <dd className="metric-num text-right font-mono text-[var(--fg-primary)]">
              {(cal.floor * 100).toFixed(0)}%
            </dd>
            <dt>{t("haptic_cal.gain")}</dt>
            <dd className="metric-num text-right font-mono text-[var(--fg-primary)]">
              {cal.gain.toFixed(2)}×
            </dd>
          </dl>
        </div>

        <footer className="flex justify-end gap-2 pt-3">
          <button
            type="button"
            onClick={onClose}
            className="rounded-[var(--radius-sm)] px-3 py-1 text-xs text-[var(--fg-secondary)] hover:text-[var(--fg-primary)]"
          >
            {t("actions.cancel")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void save()}
            className="rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 py-1 text-xs font-semibold text-[var(--fg-inverse)] hover:bg-[var(--accent-bright)] disabled:opacity-50"
          >
            {t("actions.save")}
          </button>
        </footer>
      </dialog>
    </div>
  );
}
