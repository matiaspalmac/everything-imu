import { Minus, Square, X } from "@phosphor-icons/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

export function TitleBar() {
  const { t } = useTranslation();
  // Guard: getCurrentWindow throws when running in plain browser (vite dev
  // outside Tauri webview). Keep titlebar render-safe for designers preview.
  const win = useMemo(() => {
    try {
      return getCurrentWindow();
    } catch {
      return null;
    }
  }, []);

  return (
    <header className="flex h-[var(--titlebar-h)] items-stretch border-b border-[var(--border-subtle)] bg-[var(--bg-panel)] text-[12px] text-[var(--fg-secondary)] select-none">
      <div className="flex items-center gap-2 px-3">
        <span className="font-semibold tracking-wide text-[var(--accent)]">eIMU</span>
        <span className="text-[var(--fg-muted)]">everything-imu</span>
      </div>

      {/*
        Drag spacer — manual onMouseDown + startDragging() instead of
        data-tauri-drag-region. The HTML attribute forwards every mousemove
        through the IPC bridge (tauri-apps/tauri#8770). Manual handler only
        fires on mousedown / dblclick.
      */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: window drag region is not a button */}
      <div
        className="flex-1"
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
