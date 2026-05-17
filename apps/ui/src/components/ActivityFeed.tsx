import { type ActivityEntry, useActivityStore } from "../stores/useActivityStore";

const LEVEL_CLS: Record<ActivityEntry["level"], string> = {
  info: "text-[var(--info)]",
  success: "text-[var(--success)]",
  warn: "text-[var(--warn)]",
  error: "text-[var(--danger)]",
};

export function ActivityFeed({ limit = 12 }: { limit?: number }) {
  const entries = useActivityStore((s) => s.entries).slice(0, limit);
  if (entries.length === 0) {
    return (
      <div className="rounded-[var(--radius-md)] border border-dashed border-[var(--border-subtle)] p-4 text-center text-xs text-[var(--fg-muted)]">
        No activity yet.
      </div>
    );
  }
  return (
    <ul className="flex flex-col gap-1">
      {entries.map((e) => (
        <li
          key={e.id}
          className="flex items-baseline gap-3 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-1.5"
        >
          <span className="font-mono text-[10px] text-[var(--fg-muted)]" suppressHydrationWarning>
            {/* oxlint-disable-next-line react-doctor/rendering-hydration-mismatch-time -- Tauri CSR, no SSR */}
            {new Date(e.ts).toLocaleTimeString([], { hour12: false })}
          </span>
          <span className={`text-[10px] font-semibold uppercase ${LEVEL_CLS[e.level]}`}>
            {e.level}
          </span>
          <span className="flex-1 truncate text-xs text-[var(--fg-primary)]">{e.message}</span>
        </li>
      ))}
    </ul>
  );
}
