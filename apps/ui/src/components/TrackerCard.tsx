import { Crosshair, EyeSlash, PencilSimple, X } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TrackerSnapshot } from "../api/client";
import { api } from "../api/client";
import { macHex, macKey } from "../lib/macFormat";
import { usePerDeviceSettingsStore } from "../stores/usePerDeviceSettingsStore";
import { useToastStore } from "../stores/useToastStore";
import { BatteryRing } from "./BatteryRing";
import { SignalMeter } from "./SignalMeter";
import { StatusBadge } from "./StatusBadge";
import { TrackerViz } from "./TrackerViz";

export function TrackerCard({ snap, targetHz }: { snap: TrackerSnapshot; targetHz: number }) {
  const { t } = useTranslation();
  const battery = Number.isFinite(snap.battery_fraction)
    ? Math.round(snap.battery_fraction * 100)
    : null;
  const key = macKey(snap.mac);
  const settings = usePerDeviceSettingsStore((s) => s.perMac[key]);
  const ensure = usePerDeviceSettingsStore((s) => s.ensure);
  const patch = usePerDeviceSettingsStore((s) => s.patch);
  const pushToast = useToastStore((s) => s.push);
  const [editing, setEditing] = useState(false);
  const [draftLabel, setDraftLabel] = useState("");
  // One-shot pulse animation the first time a card sees rate_hz > 0 — the
  // "device just woke up" affordance. Set during render (not in an effect)
  // so it sticks the first time the condition is met.
  const [pulsed, setPulsed] = useState(false);
  if (!pulsed && snap.rate_hz > 0) {
    setPulsed(true);
  }

  useEffect(() => {
    void ensure(snap.mac);
  }, [snap.mac, ensure]);

  const label = settings?.label ?? "";
  const group = settings?.group ?? "";

  async function broadcastYaw(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    await api.requestReset(snap.mac, "yaw");
    pushToast({
      level: "info",
      message: t("toast.yaw_reset_sent", { mac: macHex(snap.mac) }),
      ttlMs: 1800,
    });
  }

  async function hide(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    patch(snap.mac, { hidden: true });
    await api.setTrackerHidden(snap.mac, true);
    pushToast({
      level: "info",
      message: t("toast.tracker_hidden", { mac: label || macHex(snap.mac) }),
      ttlMs: 5000,
      action: {
        label: t("actions.undo"),
        run: async () => {
          patch(snap.mac, { hidden: false });
          await api.setTrackerHidden(snap.mac, false);
        },
      },
    });
  }

  function startEdit(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    setDraftLabel(label);
    setEditing(true);
  }

  async function commitEdit() {
    const prev = label;
    const next = draftLabel.trim();
    setEditing(false);
    if (next === prev) return;
    patch(snap.mac, { label: next });
    await api.setTrackerLabel(snap.mac, next);
    pushToast({
      level: "info",
      message: t("toast.label_updated"),
      ttlMs: 5000,
      action: {
        label: t("actions.undo"),
        run: async () => {
          patch(snap.mac, { label: prev });
          await api.setTrackerLabel(snap.mac, prev);
        },
      },
    });
  }

  return (
    <div
      className={`group/card relative flex gap-4 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4 transition hover:border-[var(--border-strong)] ${pulsed ? "connect-pulse" : ""}`}
    >
      <TrackerViz quat={snap.quat_xyzw} />
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          {editing ? (
            <span className="flex flex-1 items-center gap-1">
              <input
                // oxlint-disable-next-line jsx-a11y/no-autofocus -- rename button just clicked; user expects focus inside the input on mount
                autoFocus
                aria-label={t("actions.rename")}
                value={draftLabel}
                onChange={(e) => setDraftLabel(e.target.value)}
                onClick={(e) => e.preventDefault()}
                onBlur={() => void commitEdit()}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    (e.currentTarget as HTMLInputElement).blur();
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    setEditing(false);
                  }
                }}
                maxLength={64}
                placeholder={t("label_placeholder")}
                className="min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--accent)] bg-[var(--bg)] px-1.5 py-0.5 text-sm text-[var(--fg-primary)] focus:outline-none"
              />
              <button
                type="button"
                aria-label={t("window.dismiss")}
                onMouseDown={(e) => {
                  // mousedown fires before blur — cancel without committing.
                  e.preventDefault();
                  e.stopPropagation();
                  setEditing(false);
                }}
                className="rounded-[var(--radius-sm)] p-1 text-[var(--fg-muted)] hover:bg-[var(--accent-soft)]"
              >
                <X size={14} weight="bold" />
              </button>
            </span>
          ) : (
            <span className="truncate text-sm text-[var(--fg-primary)]">
              {label || <span className="font-mono">{macHex(snap.mac)}</span>}
            </span>
          )}
          <StatusBadge rateHz={snap.rate_hz} targetHz={targetHz} />
          <SignalMeter mac={snap.mac} rateHz={snap.rate_hz} targetHz={targetHz} compact />
        </div>

        <div className="flex items-center gap-2">
          <span className="truncate font-mono text-[10px] text-[var(--fg-muted)]">
            {label ? macHex(snap.mac) : snap.serial}
          </span>
          {group && (
            <span className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-[var(--accent)]">
              {group}
            </span>
          )}
        </div>

        {battery !== null && (
          <div className="flex items-center gap-2 text-xs text-[var(--fg-secondary)]">
            <BatteryRing fraction={snap.battery_fraction} size={20} />
            <div className="h-1.5 flex-1 overflow-hidden rounded bg-[var(--bg-elevated)]">
              <div
                className={`h-full rounded ${battery < 15 ? "bg-[var(--warn)]" : "bg-[var(--accent)]"}`}
                style={{ width: `${battery}%` }}
              />
            </div>
            <span className="metric-num font-mono">{battery}%</span>
          </div>
        )}
      </div>

      {!editing && (
        <div className="pointer-events-none absolute inset-x-2 bottom-2 flex justify-end gap-1 opacity-0 transition-opacity group-hover/card:pointer-events-auto group-hover/card:opacity-100">
          <CardAction
            onClick={broadcastYaw}
            title={t("actions.yaw_reset")}
            icon={<Crosshair size={12} />}
          />
          <CardAction
            onClick={startEdit}
            title={t("actions.rename")}
            icon={<PencilSimple size={12} />}
          />
          <CardAction onClick={hide} title={t("actions.hide")} icon={<EyeSlash size={12} />} />
        </div>
      )}
    </div>
  );
}

function CardAction({
  onClick,
  title,
  icon,
}: {
  onClick: (e: React.MouseEvent) => void;
  title: string;
  icon: React.ReactNode;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      className="grid size-6 place-items-center rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] text-[var(--fg-muted)] shadow-sm hover:border-[var(--accent)] hover:text-[var(--accent)]"
    >
      {icon}
    </button>
  );
}
