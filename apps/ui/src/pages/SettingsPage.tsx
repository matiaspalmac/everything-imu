import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api/client";
import { type SupportedLocale, setLocale } from "../i18n";
import { useSettingsStore } from "../stores/useSettingsStore";
import { type Theme, useThemeStore } from "../stores/useThemeStore";

const VERSION = "1.0.0-alpha.0";

export function SettingsPage() {
  const settings = useSettingsStore((s) => s.settings);
  const replace = useSettingsStore((s) => s.replace);
  const setLocal = useSettingsStore((s) => s.set);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [serverAddrDraft, setServerAddrDraft] = useState<string>("");

  const refreshAutostart = useCallback(async () => {
    try {
      setAutostart(await isAutostartEnabled());
    } catch {
      // Plugin returns an error in vite-only browser preview; ignore.
    }
  }, []);

  useEffect(() => {
    api.getSettings().then((res) => {
      if (res.status === "ok") {
        replace(res.data);
        setServerAddrDraft(res.data.slime_server_addr);
      }
    });
    void refreshAutostart();
  }, [replace, refreshAutostart]);

  async function toggleAutostart(next: boolean) {
    setSaving(true);
    setMsg(null);
    try {
      if (next) await enableAutostart();
      else await disableAutostart();
      await refreshAutostart();
      setMsg(next ? "Will launch on Windows login." : "Autostart disabled.");
    } catch (e) {
      setMsg(`Autostart error: ${e}`);
    } finally {
      setSaving(false);
    }
  }

  /// Toggles + selects autosave; the server address commits on blur via
  /// commitServerAddr below.
  async function autosave(key: string, value: string, hint?: string) {
    setSaving(true);
    setMsg(null);
    try {
      await api.setSetting(key, value);
      setMsg(hint ?? "Saved.");
    } catch (e) {
      setMsg(`Error: ${e}`);
    } finally {
      setSaving(false);
    }
  }

  async function commitServerAddr() {
    if (serverAddrDraft === settings.slime_server_addr) return;
    setLocal({ slime_server_addr: serverAddrDraft });
    await autosave(
      "slime_server_addr",
      serverAddrDraft,
      "Saved. Restart app to apply server address change.",
    );
  }

  async function spawnSynthetic(count: number) {
    setSaving(true);
    setMsg(null);
    try {
      const res = await api.restartSynthetic(count);
      setMsg(
        res.status === "ok"
          ? `Synthetic Joy-Con count = ${count}`
          : `Error: ${JSON.stringify(res.error)}`,
      );
    } catch (e) {
      setMsg(`Error: ${e}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
        Settings
      </h2>

      <Card title="SlimeVR-Server connection">
        <Field label="Server address">
          <input
            type="text"
            value={serverAddrDraft}
            onChange={(e) => setServerAddrDraft(e.target.value)}
            onBlur={() => void commitServerAddr()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void commitServerAddr();
            }}
            className="w-full rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-2 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
            placeholder="127.0.0.1:6969"
          />
        </Field>
        <p className="text-[11px] text-[var(--fg-muted)]">
          UDP target. SlimeVR-Server listens on 6969 by default. Restart the app for the new address
          to take effect.
        </p>
      </Card>

      <Card title="Diagnostics">
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => void api.openLogsDir()}
            className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
          >
            Open logs folder
          </button>
          <button
            type="button"
            onClick={() => void api.openDataDir()}
            className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
          >
            Open data folder
          </button>
        </div>
        <Field label="Log level">
          <select
            value={settings.log_filter}
            onChange={(e) => {
              setLocal({ log_filter: e.target.value });
              void autosave("log_filter", e.target.value, "Log level updated.");
            }}
            className="w-full rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-2 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
          >
            {["error", "warn", "info", "debug", "trace"].map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </select>
        </Field>
      </Card>

      <Card title="Appearance">
        <ThemePicker />
        <LocalePicker />
      </Card>

      <Card title="Startup">
        <label className="flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            checked={autostart}
            disabled={saving}
            onChange={(e) => void toggleAutostart(e.target.checked)}
            className="h-4 w-4 accent-[var(--accent)]"
          />
          Launch on system startup
        </label>
        <p className="text-[11px] text-[var(--fg-muted)]">
          Registers a login-item entry. The app starts hidden.
        </p>
      </Card>

      <Card title="Tips">
        <Row label="Command palette" value="Ctrl + K" mono />
        <Row label="JoyCon latency" value="Rename BT adapter to 'Nintendo' & restart PC" />
      </Card>

      <Card title="Developer">
        <label className="flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            checked={settings.auto_start_synthetic}
            onChange={(e) => {
              setLocal({ auto_start_synthetic: e.target.checked });
              void autosave(
                "auto_start_synthetic",
                e.target.checked ? "1" : "0",
                "Synthetic auto-start updated.",
              );
            }}
            className="h-4 w-4 accent-[var(--accent)]"
          />
          Auto-start synthetic Joy-Con on launch
        </label>
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <span className="text-xs text-[var(--fg-muted)]">Spawn now:</span>
          {[0, 1, 2, 4].map((n) => (
            <button
              key={n}
              type="button"
              disabled={saving}
              onClick={() => void spawnSynthetic(n)}
              className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-2 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)] disabled:opacity-50"
            >
              {n === 0 ? "stop" : `${n}`}
            </button>
          ))}
        </div>
      </Card>

      <div className="flex items-center gap-3 text-xs text-[var(--fg-muted)]">
        {saving ? (
          <span>Saving…</span>
        ) : msg ? (
          <span className="text-[var(--fg-secondary)]">{msg}</span>
        ) : (
          <span>Changes are saved automatically.</span>
        )}
      </div>

      <Card title="About">
        <Row label="Version" value={VERSION} mono />
        <Row label="Repository" value="github.com/matiaspalmac/everything-imu" mono />
        <Row label="License" value="MIT" />
        <Row label="Protocol" value="SlimeIMU v0.4.x byte-exact" />
        <p className="pt-2 text-[11px] text-[var(--fg-muted)]">
          everything-imu is a bridge from consumer IMU controllers (Joy-Con, DualSense, Wii, …) to
          SlimeVR-Server. Body model, skeleton, and mounting calibration live on the server.
        </p>
      </Card>
    </div>
  );
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-5">
      <h3 className="text-[11px] font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
        {label}
      </div>
      {children}
    </div>
  );
}

const THEME_OPTIONS: { id: Theme; label: string; hint: string }[] = [
  { id: "dark", label: "Dark", hint: "Sumi-ink (default)" },
  { id: "light", label: "Light", hint: "Washi paper" },
  { id: "system", label: "System", hint: "Follow OS preference" },
];

function ThemePicker() {
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.set);
  return (
    <div className="flex flex-wrap gap-2">
      {THEME_OPTIONS.map((opt) => {
        const active = theme === opt.id;
        return (
          <button
            key={opt.id}
            type="button"
            onClick={() => setTheme(opt.id)}
            className={`flex flex-col items-start rounded-[var(--radius-sm)] border px-3 py-2 text-left transition-colors ${
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
  );
}

const LOCALE_OPTIONS: { id: SupportedLocale; label: string }[] = [
  { id: "en", label: "English" },
  { id: "es", label: "Español" },
];

function LocalePicker() {
  const { i18n } = useTranslation();
  const active: SupportedLocale = (i18n.language === "es" ? "es" : "en") as SupportedLocale;
  return (
    <div className="flex flex-col gap-2 pt-2">
      <div className="text-[10px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
        Language / Idioma
      </div>
      <div className="flex flex-wrap gap-2">
        {LOCALE_OPTIONS.map((opt) => (
          <button
            key={opt.id}
            type="button"
            onClick={() => setLocale(opt.id)}
            className={`rounded-[var(--radius-sm)] border px-3 py-1.5 text-xs transition-colors ${
              active === opt.id
                ? "border-[var(--accent)] bg-[var(--warn-soft)] text-[var(--accent)]"
                : "border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--fg-secondary)] hover:border-[var(--border-strong)]"
            }`}
          >
            {opt.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-3 border-b border-[var(--border-subtle)]/50 py-1 last:border-b-0">
      <span className="text-xs text-[var(--fg-muted)]">{label}</span>
      <span className={`text-sm text-[var(--fg-primary)] ${mono ? "font-mono" : ""}`}>{value}</span>
    </div>
  );
}
