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
    <div className="flex flex-col gap-6">
      <header className="flex items-end justify-between gap-4">
        <h1 className="text-xl font-semibold tracking-tight text-[var(--fg-primary)]">
          {t("pages.settings")}
        </h1>
        <div className="text-[11px] text-[var(--fg-muted)]">
          {saving ? (
            <span>{t("hints.saving")}</span>
          ) : msg ? (
            <span className="text-[var(--fg-secondary)]">{msg}</span>
          ) : (
            <span>{t("hints.autosave")}</span>
          )}
        </div>
      </header>

      {/*
        Bento grid. 3 columns on lg+, 2 on md, 1 on small. Cards opt into a
        wider footprint via className="lg:col-span-2" so dense cards (server
        address, developer pool) get the breathing room their inputs need.
        auto-rows-min so rows pack tightly when neighbour cards differ in
        height instead of stretching to match the tallest sibling.
      */}
      <div className="grid auto-rows-min grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        <Card title={t("cards.slime_connection")} span={2} feature>
          <Field label={t("labels.server_address")}>
            <input
              type="text"
              aria-label={t("labels.server_address")}
              value={serverAddrDraft}
              onChange={(e) => setServerAddrDraft(e.target.value)}
              onBlur={() => void commitServerAddr()}
              onKeyDown={(e) => {
                if (e.key === "Enter") void commitServerAddr();
              }}
              className="metric-num w-full rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
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
              className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
            >
              {t("actions.open_logs_folder")}
            </button>
            <button
              type="button"
              onClick={() => void api.openDataDir()}
              className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
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
              className="w-full rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
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
              aria-label={t("labels.launch_on_startup")}
              checked={autostart}
              disabled={saving}
              onChange={(e) => void toggleAutostart(e.target.checked)}
              className="size-4 accent-[var(--accent)]"
            />
            {t("labels.launch_on_startup")}
          </label>
          <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.autostart")}</p>

          <label className="mt-3 flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
            <input
              type="checkbox"
              aria-label={t("labels.close_to_tray")}
              checked={settings.close_to_tray}
              onChange={(e) => {
                setLocal({ close_to_tray: e.target.checked });
                void autosave(
                  "close_to_tray",
                  e.target.checked ? "1" : "0",
                  t("msg.close_to_tray_updated"),
                );
              }}
              className="size-4 accent-[var(--accent)]"
            />
            {t("labels.close_to_tray")}
          </label>
          <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.close_to_tray")}</p>

          <label className="mt-3 flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
            <input
              type="checkbox"
              aria-label={t("labels.crash_report")}
              checked={settings.crash_report_enabled}
              onChange={(e) => {
                setLocal({ crash_report_enabled: e.target.checked });
                void autosave(
                  "crash_report_enabled",
                  e.target.checked ? "1" : "0",
                  t("msg.crash_report_updated"),
                );
              }}
              className="size-4 accent-[var(--accent)]"
            />
            {t("labels.crash_report")}
          </label>
          <p className="text-[11px] text-[var(--fg-muted)]">{t("hints.crash_report")}</p>
        </Card>

        <Card title={t("cards.tips")}>
          <Row label={t("labels.command_palette")} value={t("tips.command_palette_keys")} mono />
          <Row label={t("tips.joycon_label")} value={t("tips.joycon_value")} />
        </Card>

        <Card title={t("cards.developer")} span={2}>
          <label className="flex items-center gap-2 text-sm text-[var(--fg-secondary)]">
            <input
              type="checkbox"
              aria-label={t("labels.auto_start_synthetic") ?? "auto-start synthetic"}
              checked={settings.auto_start_synthetic}
              onChange={(e) => {
                setLocal({ auto_start_synthetic: e.target.checked });
                void autosave(
                  "auto_start_synthetic",
                  e.target.checked ? "1" : "0",
                  t("msg.synth_auto_updated"),
                );
              }}
              className="size-4 accent-[var(--accent)]"
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
                className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-2 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-50"
              >
                {n === 0 ? t("hints.spawn_stop") : `${n}`}
              </button>
            ))}
          </div>
        </Card>

        <Card title={t("cards.about")} span={3}>
          <div className="grid grid-cols-1 gap-x-6 sm:grid-cols-2 lg:grid-cols-4">
            <Row label={t("labels.version")} value={VERSION} mono />
            <Row label={t("labels.repository")} value={t("repo")} mono />
            <Row label={t("labels.license")} value="MIT" />
            <Row label={t("labels.protocol")} value={t("protocol_value")} />
          </div>
          <p className="pt-2 text-[11px] text-[var(--fg-muted)]">{t("hints.about_app")}</p>
          <UpdaterPanel />
          <UdevPanel />
        </Card>
      </div>
    </div>
  );
}

function Card({
  title,
  children,
  span,
  feature,
}: {
  title: string;
  children: React.ReactNode;
  /// 1 = default. 2 / 3 sets `lg:col-span-N` so wide cards stretch on
  /// large screens while still collapsing to a single column on mobile.
  span?: 1 | 2 | 3;
  /// Marks the visually-anchored card in a row (soft accent border +
  /// glow). Used for the SlimeVR-Server connection block since that's
  /// the setting that most often draws the user to this page.
  feature?: boolean;
}) {
  const spanCls = span === 3 ? "lg:col-span-3" : span === 2 ? "lg:col-span-2" : "";
  const featureCls = feature
    ? "border-[var(--border-strong)] bg-[var(--bg-elevated)] before:absolute before:inset-x-0 before:top-0 before:h-[2px] before:bg-[var(--accent)] before:content-['']"
    : "border-[var(--border-subtle)] bg-[var(--bg-panel)] hover:border-[var(--border-strong)]";
  return (
    <section
      className={`relative flex flex-col gap-3 overflow-hidden rounded-[var(--radius-xl)] border p-5 transition-colors ${spanCls} ${featureCls}`}
    >
      <h3 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
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
                ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
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
                ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
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

/// Self-update entry. Calls the backend GitHub-release checker and, when a
/// newer release exists, lets the user download + swap the binary in place.
/// The card itself stays mounted in the About section so users always have
/// a one-click way to check, regardless of whether an update is pending.
function UpdaterPanel() {
  const { t } = useTranslation();
  const settings = useSettingsStore((s) => s.settings);
  const setLocal = useSettingsStore((s) => s.set);
  const [info, setInfo] = useState<{
    current: string;
    latest: string;
    update_available: boolean;
  } | null>(null);
  const [busy, setBusy] = useState<"check" | "apply" | null>(null);
  const [err, setErr] = useState<string | null>(null);

  async function toggleAutoCheck(next: boolean) {
    setLocal({ auto_update_on_startup: next });
    await api.setSetting("auto_update_on_startup", next ? "1" : "0");
  }
  async function toggleAutoInstall(next: boolean) {
    setLocal({ auto_install_on_startup: next });
    await api.setSetting("auto_install_on_startup", next ? "1" : "0");
  }

  async function check() {
    setBusy("check");
    setErr(null);
    const res = await api.checkForUpdate();
    setBusy(null);
    if (res.status === "ok") setInfo(res.data);
    else setErr("message" in res.error ? res.error.message : res.error.type);
  }

  async function apply() {
    setBusy("apply");
    setErr(null);
    const res = await api.applyUpdate();
    setBusy(null);
    if (res.status === "ok") setInfo({ ...res.data, update_available: false });
    else setErr("message" in res.error ? res.error.message : res.error.type);
  }

  return (
    <div className="mt-3 flex flex-col gap-2 border-t border-[var(--border-subtle)] pt-3">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[11px] uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("updater.title")}
        </span>
        <div className="flex gap-2">
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => void check()}
            className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-50"
          >
            {busy === "check" ? t("updater.checking") : t("updater.check")}
          </button>
          {info?.update_available && (
            <button
              type="button"
              disabled={busy !== null}
              onClick={() => void apply()}
              className="rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 py-1 text-xs font-semibold text-[var(--fg-inverse)] hover:bg-[var(--accent-bright)] disabled:opacity-50"
            >
              {busy === "apply" ? t("updater.applying") : t("updater.apply")}
            </button>
          )}
        </div>
      </div>
      {info && (
        <span className="text-[11px] text-[var(--fg-muted)]">
          {info.update_available
            ? t("updater.available", { current: info.current, latest: info.latest })
            : t("updater.up_to_date", { current: info.current })}
        </span>
      )}
      {err && <span className="text-[11px] text-[var(--warn)]">{err}</span>}
      <div className="flex flex-col gap-1 pt-1">
        <label className="flex items-center gap-2 text-[11px] text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            aria-label={t("updater.auto_check")}
            checked={settings.auto_update_on_startup}
            onChange={(e) => void toggleAutoCheck(e.target.checked)}
            className="size-3.5 accent-[var(--accent)]"
          />
          {t("updater.auto_check")}
        </label>
        <label className="flex items-center gap-2 text-[11px] text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            aria-label={t("updater.auto_install")}
            checked={settings.auto_install_on_startup}
            disabled={!settings.auto_update_on_startup}
            onChange={(e) => void toggleAutoInstall(e.target.checked)}
            className="size-3.5 accent-[var(--accent)]"
          />
          {t("updater.auto_install")}
        </label>
      </div>
    </div>
  );
}

/// "Install udev rules" entry. Always rendered so Windows / macOS users
/// see why it doesn't apply; clicking it on those platforms returns the
/// "Linux only" error which we surface verbatim.
function UdevPanel() {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  async function install() {
    setBusy(true);
    setMsg(null);
    const res = await api.installUdevRules();
    setBusy(false);
    if (res.status === "ok") setMsg(res.data);
    else setMsg("message" in res.error ? res.error.message : res.error.type);
  }

  return (
    <div className="mt-3 flex flex-col gap-2 border-t border-[var(--border-subtle)] pt-3">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[11px] uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("udev.title")}
        </span>
        <button
          type="button"
          disabled={busy}
          onClick={() => void install()}
          className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1 text-xs text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-50"
        >
          {busy ? t("udev.installing") : t("udev.install")}
        </button>
      </div>
      <p className="text-[11px] text-[var(--fg-muted)]">{t("udev.body")}</p>
      {msg && <span className="text-[11px] text-[var(--fg-secondary)]">{msg}</span>}
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 border-b border-[var(--border-subtle)]/50 py-1.5 last:border-b-0 sm:border-b-0">
      <span className="text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">{label}</span>
      <span
        className={`truncate text-sm text-[var(--fg-primary)] ${mono ? "metric-num font-mono" : ""}`}
      >
        {value}
      </span>
    </div>
  );
}
