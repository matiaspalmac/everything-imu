import { PlusIcon, TrashIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { HapticConfigDto, HapticModeDto, HapticRuleDto } from "../api/client";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { UdpHaptics } from "../components/UdpHaptics";
import { macHex, macKey } from "../lib/macFormat";
import { useDeviceStore } from "../stores/useDeviceStore";
import { useHapticStore } from "../stores/useHapticStore";
import { useToastStore } from "../stores/useToastStore";

const DEFAULT_PROXIMITY: HapticModeDto = {
  kind: "proximity",
  gain: 1.0,
  min_threshold: 0.05,
};

function defaultRule(deviceMac: string): HapticRuleDto {
  return { osc_address: "", device_mac: deviceMac, mode: DEFAULT_PROXIMITY };
}

export function HapticsPage() {
  const { t } = useTranslation();
  const devices = useDeviceStore((s) => s.devices);
  const discovered = useHapticStore((s) => s.discovered);
  const config = useHapticStore((s) => s.config);
  const setConfig = useHapticStore((s) => s.setConfig);
  const configLoaded = useHapticStore((s) => s.configLoaded);
  const pushToast = useToastStore((s) => s.push);

  const [status, setStatus] = useState<"idle" | "saving" | "saved">("idle");
  const saveTimer = useRef<number | null>(null);
  const savedTimer = useRef<number | null>(null);
  const initialLoad = useRef(true);

  const rumbleDevices = Object.values(devices).filter((d) => d.has_rumble);

  useEffect(() => {
    // Fetch only once per session. Refetching on every remount would
    // clobber unsaved edits when switching tabs.
    if (configLoaded) return;
    api.getHapticConfig().then((res) => {
      if (res.status === "ok") setConfig(res.data);
    });
  }, [configLoaded, setConfig]);

  // Autosave: debounced commit of every config mutation. Skips the very
  // first render after fetch so loading the persisted config doesn't echo
  // straight back to the backend.
  useEffect(() => {
    if (!config) return;
    if (initialLoad.current) {
      initialLoad.current = false;
      return;
    }
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    setStatus("saving");
    saveTimer.current = window.setTimeout(() => {
      void (async () => {
        const res = await api.setHapticConfig(config);
        if (res.status === "ok") {
          setStatus("saved");
          if (savedTimer.current) window.clearTimeout(savedTimer.current);
          savedTimer.current = window.setTimeout(() => setStatus("idle"), 1200);
        } else {
          setStatus("idle");
          const detail = "message" in res.error ? res.error.message : res.error.type;
          pushToast({ level: "warn", title: t("haptics.title"), message: detail });
        }
      })();
    }, 400);
    return () => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
    };
  }, [config, pushToast, t]);

  if (!config) {
    return <div className="p-6 text-xs text-[var(--fg-muted)]">…</div>;
  }

  const firstDeviceMac = rumbleDevices[0] ? macKey(rumbleDevices[0].mac) : "";

  const patch = (next: Partial<HapticConfigDto>) => setConfig({ ...config, ...next });

  const patchRule = (idx: number, next: Partial<HapticRuleDto>) =>
    patch({
      rules: config.rules.map((r, i) => (i === idx ? { ...r, ...next } : r)),
    });

  const addRule = (address = "") =>
    patch({ rules: [...config.rules, { ...defaultRule(firstDeviceMac), osc_address: address }] });

  const removeRule = (idx: number) => patch({ rules: config.rules.filter((_, i) => i !== idx) });

  return (
    <div className="flex flex-col gap-5">
      <header className="flex items-center justify-between gap-3">
        <div className="flex flex-col">
          <h2 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
            {t("haptics.title")}
          </h2>
          <span className="text-[11px] text-[var(--fg-muted)]">{t("haptics.subtitle")}</span>
        </div>
        <span
          aria-live="polite"
          className="text-[11px] uppercase tracking-[0.12em] text-[var(--fg-muted)]"
        >
          {status === "saving"
            ? t("hints.saving")
            : status === "saved"
              ? t("msg.generic_saved")
              : ""}
        </span>
      </header>

      <Tile title={t("haptics.how_title")}>
        <p className="text-xs leading-relaxed text-[var(--fg-secondary)]">
          {t("haptics.how_body")}
        </p>
      </Tile>

      <Tile title={config.enabled ? t("haptics.enabled") : t("haptics.disabled")}>
        <div className="flex flex-wrap items-center gap-6">
          <label className="flex items-center gap-2 text-xs text-[var(--fg-secondary)]">
            <input
              type="checkbox"
              aria-label={t("haptics.enabled")}
              checked={config.enabled}
              onChange={(e) => patch({ enabled: e.target.checked })}
            />
            {config.enabled ? t("haptics.enabled") : t("haptics.disabled")}
          </label>
          <label className="flex items-center gap-2 text-xs text-[var(--fg-secondary)]">
            {t("haptics.listen_port")}
            <input
              type="number"
              aria-label={t("haptics.listen_port")}
              value={config.listen_port}
              onChange={(e) => patch({ listen_port: Number(e.target.value) || 0 })}
              className="w-24 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1 font-mono text-[var(--fg-primary)]"
            />
          </label>
        </div>
      </Tile>

      <Tile title={t("haptics.test_title")}>
        {rumbleDevices.length === 0 ? (
          <p className="text-xs text-[var(--fg-muted)]">{t("haptics.no_devices")}</p>
        ) : (
          <div className="flex flex-col gap-2">
            <p className="text-[11px] text-[var(--fg-muted)]">{t("haptics.test_hint")}</p>
            <div className="flex flex-wrap gap-2">
              {rumbleDevices.map((d) => (
                <button
                  key={macKey(d.mac)}
                  type="button"
                  onClick={() =>
                    void api.testRumble(
                      d.mac as [number, number, number, number, number, number],
                      400,
                    )
                  }
                  className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-3 py-1.5 text-xs text-[var(--fg-primary)] hover:border-[var(--accent)]"
                >
                  {d.kind} · {macHex(d.mac)}
                </button>
              ))}
            </div>
          </div>
        )}
      </Tile>

      <Tile title={t("haptics.rules")}>
        {rumbleDevices.length === 0 && (
          <p className="mb-3 text-xs text-[var(--warn)]">{t("haptics.no_devices")}</p>
        )}
        {config.rules.length === 0 ? (
          <EmptyState icon={PlusIcon} title={t("haptics.no_rules")} compact />
        ) : (
          <div className="flex flex-col gap-3">
            {config.rules.map((rule, idx) => (
              <RuleRow
                // biome-ignore lint/suspicious/noArrayIndexKey: rules array uses positional identity — keying off rule fields would remount the row on every keystroke and steal input focus.
                key={idx}
                rule={rule}
                devices={rumbleDevices}
                onChange={(next) => patchRule(idx, next)}
                onRemove={() => removeRule(idx)}
              />
            ))}
          </div>
        )}
        <button
          type="button"
          onClick={() => addRule()}
          className="mt-3 flex items-center gap-1.5 text-xs font-semibold text-[var(--accent)]"
        >
          <PlusIcon size={14} /> {t("haptics.add_rule")}
        </button>
      </Tile>

      <Tile title={t("haptics.discovered")}>
        {discovered.length === 0 ? (
          <p className="text-xs text-[var(--fg-muted)]">{t("haptics.no_discovered")}</p>
        ) : (
          <div className="flex flex-col divide-y divide-[var(--border-subtle)]">
            {discovered.map((addr) => (
              <div key={addr} className="flex items-center gap-3 py-1.5 text-xs">
                <span className="truncate font-mono text-[var(--fg-primary)]">{addr}</span>
                <button
                  type="button"
                  onClick={() => addRule(addr)}
                  className="ml-auto rounded-[var(--radius-sm)] border border-[var(--border-subtle)] px-2 py-0.5 text-[var(--fg-secondary)] hover:border-[var(--accent)]"
                >
                  {t("haptics.use")}
                </button>
              </div>
            ))}
          </div>
        )}
      </Tile>

      <UdpHaptics />
    </div>
  );
}

function RuleRow({
  rule,
  devices,
  onChange,
  onRemove,
}: {
  rule: HapticRuleDto;
  devices: { mac: number[]; serial: string; kind: string }[];
  onChange: (next: Partial<HapticRuleDto>) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const fieldCls =
    "rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1 text-xs text-[var(--fg-primary)]";

  const setMode = (kind: HapticModeDto["kind"]) =>
    onChange({
      mode: kind === "proximity" ? DEFAULT_PROXIMITY : { kind: "pulse", pulse_ms: 150 },
    });

  return (
    <div className="flex flex-wrap items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] p-2">
      <input
        type="text"
        aria-label={t("haptics.osc_address_ph")}
        value={rule.osc_address}
        placeholder={t("haptics.osc_address_ph")}
        onChange={(e) => onChange({ osc_address: e.target.value })}
        className={`${fieldCls} min-w-[14rem] flex-1 font-mono`}
      />
      <select
        value={rule.device_mac}
        onChange={(e) => onChange({ device_mac: e.target.value })}
        className={fieldCls}
      >
        {devices.length === 0 && <option value="">(none)</option>}
        {devices.map((d) => (
          <option key={macKey(d.mac)} value={macKey(d.mac)}>
            {d.kind} · {macHex(d.mac)}
          </option>
        ))}
      </select>
      <select
        value={rule.mode.kind}
        onChange={(e) => setMode(e.target.value as HapticModeDto["kind"])}
        className={fieldCls}
      >
        <option value="proximity">{t("haptics.mode_proximity")}</option>
        <option value="pulse">{t("haptics.mode_pulse")}</option>
      </select>
      {rule.mode.kind === "proximity" ? (
        <>
          <NumField
            label={t("haptics.gain")}
            value={rule.mode.gain}
            step={0.1}
            onChange={(v) =>
              onChange({
                mode: {
                  kind: "proximity",
                  gain: v,
                  min_threshold: rule.mode.kind === "proximity" ? rule.mode.min_threshold : 0.05,
                },
              })
            }
          />
          <NumField
            label={t("haptics.min_threshold")}
            value={rule.mode.min_threshold}
            step={0.01}
            onChange={(v) =>
              onChange({
                mode: {
                  kind: "proximity",
                  gain: rule.mode.kind === "proximity" ? rule.mode.gain : 1.0,
                  min_threshold: v,
                },
              })
            }
          />
        </>
      ) : (
        <NumField
          label={t("haptics.pulse_ms")}
          value={rule.mode.pulse_ms}
          step={10}
          onChange={(v) => onChange({ mode: { kind: "pulse", pulse_ms: Math.round(v) } })}
        />
      )}
      <button
        type="button"
        onClick={onRemove}
        aria-label={t("haptics.remove")}
        className="ml-auto text-[var(--fg-muted)] hover:text-[var(--warn)]"
      >
        <TrashIcon size={15} />
      </button>
    </div>
  );
}

function NumField({
  label,
  value,
  step,
  onChange,
}: {
  label: string;
  value: number;
  step: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="flex items-center gap-1 text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
      {label}
      <input
        type="number"
        aria-label={label}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value) || 0)}
        className="w-16 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] px-1.5 py-1 font-mono text-xs text-[var(--fg-primary)]"
      />
    </label>
  );
}

function Tile({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="relative flex flex-col gap-3 overflow-hidden rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4">
      <h3 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
        {title}
      </h3>
      <div className="min-w-0 flex-1">{children}</div>
    </section>
  );
}
