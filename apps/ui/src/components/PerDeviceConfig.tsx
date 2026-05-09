import { useCallback, useEffect, useState } from "react";
import type { FusionAlgoDto, MountingOrientationDto, PerDeviceSettingsDto } from "../api/client";
import { api } from "../api/client";

const FUSION_OPTIONS: { id: FusionAlgoDto; label: string; hint: string }[] = [
  {
    id: "vqf",
    label: "VQF",
    hint: "default · Laidig 2023, gyro+accel+mag, bias Kalman",
  },
  {
    id: "madgwick",
    label: "Madgwick",
    hint: "gradient-descent, lighter CPU",
  },
  {
    id: "basic_vqf",
    label: "Basic VQF",
    hint: "6D subset of VQF without bias / rest detection",
  },
];

const MOUNTING_OPTIONS: { id: MountingOrientationDto; label: string }[] = [
  { id: "identity", label: "Identity" },
  { id: "left_side", label: "Left side" },
  { id: "right_side", label: "Right side" },
  { id: "upside_down", label: "Upside down" },
  { id: "facing_forward", label: "Facing forward" },
  { id: "facing_back", label: "Facing back" },
];

export function PerDeviceConfig({
  mac,
}: {
  mac: [number, number, number, number, number, number];
}) {
  const [settings, setSettings] = useState<PerDeviceSettingsDto | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

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
        setMsg(`Fusion → ${algo} (reconnect tracker to apply)`);
        await reload();
      } else {
        setMsg(`Error: ${JSON.stringify(res.error)}`);
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
        setMsg(`Mounting → ${o} (live)`);
        await reload();
      } else {
        setMsg(`Error: ${JSON.stringify(res.error)}`);
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
        setMsg(`Magnetometer ${enabled ? "enabled" : "disabled"} (live)`);
        await reload();
      } else {
        setMsg(`Error: ${JSON.stringify(res.error)}`);
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
        setMsg(`Error: ${JSON.stringify(res.error)}`);
      }
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="flex flex-col gap-4 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4">
      <Section label="Label">
        <input
          type="text"
          value={settings?.label ?? ""}
          maxLength={64}
          placeholder="e.g. right shin"
          onChange={(e) => {
            if (settings) setSettings({ ...settings, label: e.target.value });
          }}
          onBlur={(e) => void saveLabel(e.target.value)}
          disabled={busy === "label"}
          className="w-full rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
        />
        <p className="text-[11px] text-[var(--fg-muted)]">
          Informational only. SlimeVR-Server owns body assignment.
        </p>
      </Section>

      <Section label="Fusion algorithm">
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

      <Section label="Mounting orientation">
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

      <Section label="Magnetometer">
        <label className="flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            checked={settings?.magnetometer_enabled ?? false}
            disabled={busy === "mag"}
            onChange={(e) => void toggleMag(e.target.checked)}
            className="h-4 w-4 accent-[var(--accent)]"
          />
          Feed magnetometer to fusion (9D yaw)
        </label>
        <p className="text-[11px] text-[var(--fg-muted)]">
          Joy-Con 1 has no magnetometer. Enabling on JC2/Wii forwards mag samples once
          SET_CONFIG_FLAG is wired (Sprint 7).
        </p>
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
