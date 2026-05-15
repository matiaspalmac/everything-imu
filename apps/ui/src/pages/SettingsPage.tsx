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

const VERSION = __APP_VERSION__;

export function SettingsPage() {
  const { t } = useTranslation();
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
      setMsg(next ? t("msg.autostart_on") : t("msg.autostart_off"));
    } catch (e) {
      setMsg(t("msg.autostart_error", { err: String(e) }));
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
      setMsg(hint ?? t("msg.generic_saved"));
    } catch (e) {
      setMsg(t("msg.error_generic", { err: String(e) }));
    } finally {
      setSaving(false);
    }
  }

  async function commitServerAddr() {
    if (serverAddrDraft === settings.slime_server_addr) return;
    setLocal({ slime_server_addr: serverAddrDraft });
    await autosave("slime_server_addr", serverAddrDraft, t("msg.server_addr_saved"));
  }

  async function spawnSynthetic(count: number) {
    setSaving(true);
    setMsg(null);
    try {
      const res = await api.restartSynthetic(count);
      setMsg(
        res.status === "ok"
          ? t("msg.synth_count", { count })
          : t("msg.error_generic", { err: JSON.stringify(res.error) }),
      );
    } catch (e) {
      setMsg(t("msg.error_generic", { err: String(e) }));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
        {t("pages.settings")}
      </h2>

      <Card title={t("cards.slime_connection")}>
        <Field label={t("labels.server_address")}>
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
        <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.server_address")}</p>
      </Card>

      <Card title={t("cards.diagnostics")}>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => void api.openLogsDir()}
            className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
          >
            {t("actions.open_logs_folder")}
          </button>
          <button
            type="button"
            onClick={() => void api.openDataDir()}
            className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
          >
            {t("actions.open_data_folder")}
          </button>
        </div>
        <Field label={t("labels.log_level")}>
          <select
            value={settings.log_filter}
            onChange={(e) => {
              setLocal({ log_filter: e.target.value });
              void autosave("log_filter", e.target.value, t("msg.log_level_updated"));
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

      <Card title={t("cards.appearance")}>
        <ThemePicker />
        <LocalePicker />
      </Card>

      <Card title={t("cards.startup")}>
        <label className="flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            checked={autostart}
            disabled={saving}
            onChange={(e) => void toggleAutostart(e.target.checked)}
            className="h-4 w-4 accent-[var(--accent)]"
          />
          {t("labels.launch_on_startup")}
        </label>
        <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.autostart")}</p>
      </Card>

      <Card title={t("cards.tips")}>
        <Row label={t("labels.command_palette")} value={t("tips.command_palette_keys")} mono />
        <Row label={t("tips.joycon_label")} value={t("tips.joycon_value")} />
      </Card>

      <Card title={t("cards.developer")}>
        <label className="flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            checked={settings.auto_start_synthetic}
            onChange={(e) => {
              setLocal({ auto_start_synthetic: e.target.checked });
              void autosave(
                "auto_start_synthetic",
                e.target.checked ? "1" : "0",
                t("msg.synth_auto_updated"),
              );
            }}
            className="h-4 w-4 accent-[var(--accent)]"
          />
          {t("hints.auto_start_synthetic")}
        </label>
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <span className="text-xs text-[var(--fg-muted)]">{t("hints.spawn_now")}</span>
          {[0, 1, 2, 4].map((n) => (
            <button
              key={n}
              type="button"
              disabled={saving}
              onClick={() => void spawnSynthetic(n)}
              className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-2 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--warn-soft)] hover:text-[var(--accent)] disabled:opacity-50"
            >
              {n === 0 ? t("hints.spawn_stop") : `${n}`}
            </button>
          ))}
        </div>
      </Card>

      <div className="flex items-center gap-3 text-xs text-[var(--fg-muted)]">
        {saving ? (
          <span>{t("hints.saving")}</span>
        ) : msg ? (
          <span className="text-[var(--fg-secondary)]">{msg}</span>
        ) : (
          <span>{t("hints.autosave")}</span>
        )}
      </div>

      <Card title={t("cards.about")}>
        <Row label={t("labels.version")} value={VERSION} mono />
        <Row label={t("labels.repository")} value={t("repo")} mono />
        <Row label={t("labels.license")} value="MIT" />
        <Row label={t("labels.protocol")} value={t("protocol_value")} />
        <p className="pt-2 text-[11px] text-[var(--fg-muted)]">{t("hints.about_app")}</p>
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

function ThemePicker() {
  const { t } = useTranslation();
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.set);
  const options: { id: Theme; labelKey: string; hintKey: string }[] = [
    { id: "dark", labelKey: "theme.dark", hintKey: "theme.dark_hint" },
    { id: "light", labelKey: "theme.light", hintKey: "theme.light_hint" },
    { id: "system", labelKey: "theme.system", hintKey: "theme.system_hint" },
  ];
  return (
    <div className="flex flex-wrap gap-2">
      {options.map((opt) => {
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
            <span className="text-sm font-semibold">{t(opt.labelKey)}</span>
            <span className="text-[10px] text-[var(--fg-muted)]">{t(opt.hintKey)}</span>
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
  const { i18n, t } = useTranslation();
  const active: SupportedLocale = (i18n.language === "es" ? "es" : "en") as SupportedLocale;
  return (
    <div className="flex flex-col gap-2 pt-2">
      <div className="text-[10px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
        {t("labels.language")}
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
