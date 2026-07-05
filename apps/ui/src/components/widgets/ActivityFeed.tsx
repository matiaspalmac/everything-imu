import { useTranslation } from "react-i18next";
import { type ActivityEntry, useActivityStore } from "../../stores/useActivityStore";

const LEVEL_DOT: Record<ActivityEntry["level"], string> = {
  info: "var(--info)",
  success: "var(--success)",
  warn: "var(--warn)",
  error: "var(--danger)",
};

export function ActivityFeed({ limit = 12 }: { limit?: number }) {
  const { t } = useTranslation();
  const entries = useActivityStore((s) => s.entries).slice(0, limit);
  if (entries.length === 0) {
    return (
      <div className="rounded-[var(--radius-md)] border border-dashed border-[var(--border-subtle)] p-4 text-center text-xs text-[var(--fg-muted)]">
        {t("pages.activity_empty")}
      </div>
    );
  }
  return (
    <ul className="flex flex-col">
      {entries.map((e) => (
        <li
          key={e.id}
          className="flex items-center gap-3 rounded-[var(--radius-md)] px-2 py-1.5 transition-colors hover:bg-[var(--bg-elevated)]"
        >
          <span
            aria-hidden
            title={e.level}
            className="size-1.5 shrink-0 rounded-full"
            style={{ background: LEVEL_DOT[e.level] }}
          />
          <span className="flex-1 truncate text-xs text-[var(--fg-primary)]">{e.message}</span>
          <span
            className="shrink-0 font-mono text-[10px] text-[var(--fg-muted)]"
            suppressHydrationWarning
          >
            {/* oxlint-disable-next-line react-doctor/rendering-hydration-mismatch-time -- Tauri CSR, no SSR */}
            {new Date(e.ts).toLocaleTimeString([], { hour12: false })}
          </span>
        </li>
      ))}
    </ul>
  );
}
