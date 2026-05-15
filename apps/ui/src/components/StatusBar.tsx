import { CircleNotch, Keyboard, Pause, Play, Plug, PlugsConnected } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import { useConnectionStore } from "../stores/useConnectionStore";
import { useTrackerStore } from "../stores/useTrackerStore";

const VERSION = __APP_VERSION__;

export function StatusBar() {
  const { t } = useTranslation();
  const status = useConnectionStore((s) => s.status);
  const trackers = useTrackerStore((s) => s.trackers);
  const list = Object.values(trackers);

  const [, force] = useState(0);
  const [paused, setPausedLocal] = useState(false);
  useEffect(() => {
    const id = window.setInterval(() => force((n) => n + 1), 1000);
    api.getEmissionPaused().then((res) => {
      if (res.status === "ok") setPausedLocal(res.data);
    });
    return () => window.clearInterval(id);
  }, []);

  async function togglePause() {
    const next = !paused;
    setPausedLocal(next);
    await api.setEmissionPaused(next);
  }

  const live = list.some((t) => t.rate_hz > 0);
  const meanRate =
    list.length === 0 ? 0 : list.reduce((acc, t) => acc + t.rate_hz, 0) / list.length;
  const lastSendMs = status?.last_send_ms_ago ?? null;
  const stale = lastSendMs == null || lastSendMs > 2000;

  return (
    <footer className="flex h-[var(--statusbar-h)] shrink-0 items-center gap-3 border-t border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 text-[11px] text-[var(--fg-muted)]">
      <span className="flex items-center gap-1">
        {live && !stale ? (
          <PlugsConnected size={12} className="text-[var(--success)]" />
        ) : list.length > 0 ? (
          <CircleNotch size={12} className="text-[var(--warn)]" />
        ) : (
          <Plug size={12} />
        )}
        <span className={live && !stale ? "text-[var(--success)]" : ""}>
          {live && !stale
            ? t("status.live")
            : list.length > 0
              ? t("status.stalled")
              : t("status.idle")}
        </span>
      </span>
      <span className="text-[var(--border-strong)]">·</span>
      <span>
        {t(list.length === 1 ? "status.tracker_count" : "status.tracker_count_plural", {
          count: list.length,
        })}
      </span>
      {list.length > 0 && (
        <>
          <span className="text-[var(--border-strong)]">·</span>
          <span className="font-mono">{meanRate.toFixed(0)} Hz</span>
        </>
      )}
      {status && (
        <>
          <span className="text-[var(--border-strong)]">·</span>
          <span className="font-mono">{t("status.pkts", { count: status.packets_sent })}</span>
          <span className="text-[var(--border-strong)]">·</span>
          <span className="font-mono">{status.server_addr}</span>
        </>
      )}
      <span className="ml-auto flex items-center gap-3">
        <button
          type="button"
          onClick={() => void togglePause()}
          title={paused ? t("status.resume_emission") : t("status.pause_emission")}
          className={`flex items-center gap-1 rounded-[var(--radius-sm)] px-1.5 py-0.5 transition-colors ${
            paused
              ? "bg-[var(--warn-soft)] text-[var(--warn)]"
              : "hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
          }`}
        >
          {paused ? <Play size={12} /> : <Pause size={12} />}
          {paused ? t("status.paused") : t("status.running")}
        </button>
        <span className="text-[var(--border-strong)]">·</span>
        <Link
          to="/help"
          className="flex items-center gap-1 hover:text-[var(--accent)]"
          title={t("status.help_title")}
        >
          <Keyboard size={12} />
          {t("status.help_short")}
        </Link>
        <span className="text-[var(--border-strong)]">·</span>
        <span className="font-mono">v{VERSION}</span>
      </span>
    </footer>
  );
}
