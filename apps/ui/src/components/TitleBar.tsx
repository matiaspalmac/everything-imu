import { Broadcast, Minus, Pause, Square, X } from "@phosphor-icons/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useEmissionStore } from "../stores/useEmissionStore";

export function TitleBar() {
  const { t } = useTranslation();
  const paused = useEmissionStore((s) => s.paused);
  const hydrated = useEmissionStore((s) => s.hydrated);
  const hydrate = useEmissionStore((s) => s.hydrate);
  const toggle = useEmissionStore((s) => s.toggle);

  useEffect(() => {
    if (!hydrated) void hydrate();
  }, [hydrated, hydrate]);

  // Guard: getCurrentWindow throws when running in plain browser (vite dev
  // outside Tauri webview). Keep titlebar render-safe for designers preview.
  const win = useMemo(() => {
    try {
      return getCurrentWindow();
    } catch {
      return null;
    }
  }, []);

  const bridgeLabel = paused ? t("bridge.resume") : t("bridge.pause");
  const bridgeState = paused ? t("bridge.off") : t("bridge.on");

  return (
    <header className="flex h-[var(--titlebar-h)] items-stretch border-b border-[var(--border-subtle)] bg-[var(--bg-panel)] text-[12px] text-[var(--fg-secondary)] select-none">
      <div className="flex items-center gap-2 px-3">
        <span className="font-semibold tracking-wide text-[var(--accent)]">eIMU</span>
        <span className="text-[var(--fg-muted)]">everything-imu</span>
      </div>

      <button
        type="button"
        onClick={() => void toggle()}
        title={`${bridgeLabel} · Ctrl+Shift+B`}
        aria-label={bridgeLabel}
        aria-pressed={!paused}
        className={`mx-2 my-1 flex items-center gap-1.5 rounded-[var(--radius-sm)] border px-2 py-0.5 text-[11px] font-medium transition-colors ${
          paused
            ? "border-[var(--danger)] bg-[var(--danger-soft,var(--warn-soft))] text-[var(--danger)] hover:brightness-110"
            : "border-[var(--success)]/40 bg-[var(--success-soft,transparent)] text-[var(--success)] hover:bg-[var(--success-soft,var(--accent-soft))]"
        }`}
      >
        {paused ? <Pause size={12} weight="fill" /> : <Broadcast size={12} weight="fill" />}
        <span>
          {t("bridge.label")}: {bridgeState}
        </span>
      </button>

      {/*
        Drag spacer — manual onMouseDown + startDragging() instead of
        data-tauri-drag-region. The HTML attribute forwards every mousemove
        through the IPC bridge (tauri-apps/tauri#8770). Manual handler only
        fires on mousedown / dblclick.
      */}
      {/* oxlint-disable-next-line jsx-a11y/no-static-element-interactions -- window drag region */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: window drag region is not a button */}
      <div
        className="flex-1"
        role="presentation"
        onMouseDown={(e) => {
          if (e.button === 0) void win?.startDragging();
        }}
        onDoubleClick={() => {
          void win?.toggleMaximize();
        }}
      />

      <button
        type="button"
        aria-label={t("window.minimize")}
        onClick={() => void win?.minimize()}
        className="grid w-11 place-items-center text-[var(--fg-muted)] transition-colors hover:bg-[var(--accent-soft)] hover:text-[var(--fg-primary)]"
      >
        <Minus size={14} weight="bold" />
      </button>
      <button
        type="button"
        aria-label={t("window.maximize")}
        onClick={() => void win?.toggleMaximize()}
        className="grid w-11 place-items-center text-[var(--fg-muted)] transition-colors hover:bg-[var(--accent-soft)] hover:text-[var(--fg-primary)]"
      >
        <Square size={12} weight="bold" />
      </button>
      <button
        type="button"
        aria-label={t("window.close")}
        onClick={() => void win?.close()}
        className="grid w-11 place-items-center text-[var(--fg-muted)] transition-colors hover:bg-[var(--danger)] hover:text-[var(--fg-inverse)]"
      >
        <X size={14} weight="bold" />
      </button>
    </header>
  );
}
