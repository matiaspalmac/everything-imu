import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { FusionAlgoDto, MountingOrientationDto, PerDeviceSettingsDto } from "../api/client";
import { api } from "../api/client";

const FUSION_IDS: FusionAlgoDto[] = ["vqf", "madgwick", "basic_vqf"];
const MOUNTING_IDS: MountingOrientationDto[] = [
  "identity",
  "left_side",
  "right_side",
  "upside_down",
  "facing_forward",
  "facing_back",
];

export function PerDeviceConfig({
  mac,
}: {
  mac: [number, number, number, number, number, number];
}) {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<PerDeviceSettingsDto | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const FUSION_OPTIONS = FUSION_IDS.map((id) => ({
    id,
    label: t(`fusion.${id}`),
    hint: t(`fusion.${id}_hint`),
  }));
  const MOUNTING_OPTIONS = MOUNTING_IDS.map((id) => ({
    id,
    label: t(`mounting.${id}`),
  }));

  const reload = useCallback(async () => {
    const res = await api.getPerDeviceSettings(mac);
    if (res.status === "ok") setSettings(res.data);
  }, [mac]);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function changeFusion(algo: FusionAlgoDto) {
    setBusy("fusion");
    setMsg(null);
    try {
      const res = await api.setFusionAlgo(mac, algo);
      if (res.status === "ok") {
        setMsg(t("msg.fusion_set", { algo: t(`fusion.${algo}`) }));
        await reload();
      } else {
        setMsg(t("msg.error_generic", { err: JSON.stringify(res.error) }));
      }
    } finally {
      setBusy(null);
    }
  }

  async function changeMounting(o: MountingOrientationDto) {
    setBusy("mounting");
    setMsg(null);
    try {
      const res = await api.setMountingOrientation(mac, o);
      if (res.status === "ok") {
        setMsg(t("msg.mounting_set", { orientation: t(`mounting.${o}`) }));
        await reload();
      } else {
        setMsg(t("msg.error_generic", { err: JSON.stringify(res.error) }));
      }
    } finally {
      setBusy(null);
    }
  }

  async function toggleMag(enabled: boolean) {
    setBusy("mag");
    setMsg(null);
    try {
      const res = await api.setMagnetometerEnabled(mac, enabled);
      if (res.status === "ok") {
        setMsg(enabled ? t("msg.mag_enabled") : t("msg.mag_disabled"));
        await reload();
      } else {
        setMsg(t("msg.error_generic", { err: JSON.stringify(res.error) }));
      }
    } finally {
      setBusy(null);
    }
  }

  async function saveLabel(label: string) {
    setBusy("label");
    try {
      const res = await api.setTrackerLabel(mac, label);
      if (res.status === "ok") {
        await reload();
      } else {
        setMsg(t("msg.error_generic", { err: JSON.stringify(res.error) }));
      }
    } finally {
      setBusy(null);
    }
  }

  async function saveGroup(group: string) {
    setBusy("group");
    try {
      const res = await api.setTrackerGroup(mac, group);
      if (res.status === "ok") {
        await reload();
      } else {
        setMsg(t("msg.error_generic", { err: JSON.stringify(res.error) }));
      }
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="flex flex-col gap-4 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4">
      <Section label={t("labels.label")}>
        <input
          type="text"
          value={settings?.label ?? ""}
          maxLength={64}
          placeholder={t("label_placeholder")}
          onChange={(e) => {
            const v = e.target.value;
            setSettings((prev) => (prev ? { ...prev, label: v } : prev));
          }}
          onBlur={(e) => void saveLabel(e.target.value)}
          disabled={busy === "label"}
          className="w-full rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
        />
        <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.label_info")}</p>
      </Section>

      <Section label={t("labels.group")}>
        <input
          type="text"
          value={settings?.group ?? ""}
          maxLength={32}
          placeholder={t("group_placeholder")}
          onChange={(e) => {
            const v = e.target.value;
            setSettings((prev) => (prev ? { ...prev, group: v } : prev));
          }}
          onBlur={(e) => void saveGroup(e.target.value)}
          disabled={busy === "group"}
          className="w-full rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
        />
        <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.group_info")}</p>
      </Section>

      <Section label={t("labels.fusion_algorithm")}>
        <div className="flex flex-wrap gap-2">
          {FUSION_OPTIONS.map((opt) => {
            const active = settings?.fusion === opt.id;
            return (
              <button
                key={opt.id}
                type="button"
                disabled={busy === "fusion"}
                onClick={() => void changeFusion(opt.id)}
                title={opt.hint}
                className={`flex flex-col items-start rounded-[var(--radius-sm)] border px-3 py-2 text-left transition-colors disabled:opacity-50 ${
                  active
                    ? "border-[var(--accent)] bg-[var(--warn-soft)] text-[var(--accent)]"
                    : "border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--fg-secondary)] hover:border-[var(--border-strong)]"
                }`}
              >
                <span className="text-sm font-semibold">{opt.label}</span>
                <span className="text-[10px] text-[var(--fg-muted)]">{opt.hint}</span>
              </button>
            );
          })}
        </div>
      </Section>

      <Section label={t("labels.mounting_orientation")}>
        <div className="flex flex-wrap gap-2">
          {MOUNTING_OPTIONS.map((opt) => {
            const active = settings?.mounting === opt.id;
            return (
              <button
                key={opt.id}
                type="button"
                disabled={busy === "mounting"}
                onClick={() => void changeMounting(opt.id)}
                className={`rounded-[var(--radius-sm)] border px-3 py-1.5 text-xs transition-colors disabled:opacity-50 ${
                  active
                    ? "border-[var(--accent)] bg-[var(--warn-soft)] text-[var(--accent)]"
                    : "border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--fg-secondary)] hover:border-[var(--border-strong)]"
                }`}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </Section>

      <Section label={t("labels.magnetometer")}>
        <label className="flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            checked={settings?.magnetometer_enabled ?? false}
            disabled={busy === "mag"}
            onChange={(e) => void toggleMag(e.target.checked)}
            className="h-4 w-4 accent-[var(--accent)]"
          />
          {t("hints.feed_mag")}
        </label>
        <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.magnetometer")}</p>
      </Section>

      {msg && <div className="text-[11px] text-[var(--fg-muted)]">{msg}</div>}
    </div>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="text-[10px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
        {label}
      </div>
      {children}
    </div>
  );
}
