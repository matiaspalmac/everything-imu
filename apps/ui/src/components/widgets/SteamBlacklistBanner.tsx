import { Warning } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SteamBlacklistStatus } from "../../api/client";
import { api } from "../../api/client";
import { useToastStore } from "../../stores/useToastStore";

/// Warns the user when Steam Input is grabbing Joy-Con / Pro Controllers
/// (and offers a one-click fix that edits Steam's controller_blacklist).
/// Renders nothing on Linux, when Steam is not installed, or when the
/// blacklist already covers the supported devices.
export function SteamBlacklistBanner() {
  const { t } = useTranslation();
  const pushToast = useToastStore((s) => s.push);
  const [status, setStatus] = useState<SteamBlacklistStatus | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    const res = await api.steamBlacklistCheck();
    if (res.status === "ok") setStatus(res.data);
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional mount-only check; refresh is stable and re-running on each render would hammer the IPC.
  useEffect(() => {
    void refresh();
  }, []);

  if (!status || status.steam_not_found || !status.needs_fix) return null;

  async function fix() {
    setBusy(true);
    const res = await api.steamBlacklistApplyFix();
    setBusy(false);
    if (res.status === "ok") {
      pushToast({
        level: "info",
        title: t("steam_blacklist.title"),
        message: t("steam_blacklist.fixed"),
      });
      await refresh();
    } else {
      const detail = "message" in res.error ? res.error.message : res.error.type;
      pushToast({
        level: "warn",
        title: t("steam_blacklist.title"),
        message: detail,
      });
    }
  }

  return (
    <div className="flex items-start gap-3 rounded-[var(--radius-md)] border border-[var(--warn)] bg-[var(--warn-soft)] p-3">
      <Warning size={20} className="mt-0.5 shrink-0 text-[var(--warn)]" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-xs font-semibold uppercase tracking-[0.12em] text-[var(--warn)]">
          {t("steam_blacklist.title")}
        </span>
        <span className="text-[12px] leading-relaxed text-[var(--fg-secondary)]">
          {t("steam_blacklist.body")}
        </span>
        <span className="text-[11px] text-[var(--fg-muted)]">
          {t("steam_blacklist.restart_hint")}
        </span>
      </div>
      <button
        type="button"
        disabled={busy}
        onClick={() => void fix()}
        className="shrink-0 rounded-[var(--radius-sm)] bg-[var(--warn)] px-3 py-1 text-xs font-semibold text-[var(--fg-inverse)] transition-colors hover:opacity-90 disabled:opacity-50"
      >
        {busy ? t("steam_blacklist.fixing") : t("steam_blacklist.fix")}
      </button>
    </div>
  );
}
