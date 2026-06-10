import { memo } from "react";
import type { LogEntryDto } from "../../api/client";

// Level tones mirror the LogsPage filter chips (status palette tokens).
const LEVEL_CLS: Record<string, string> = {
  ERROR: "text-[var(--danger)]",
  WARN: "text-[var(--warn)]",
  INFO: "text-[var(--success)]",
  DEBUG: "text-[var(--info)]",
  TRACE: "text-[var(--fg-muted)]",
};

/**
 * Memoised: the log list renders up to 800 rows and appends at log-event
 * rate. Entries are immutable and keyed by a stable `seq`, so memo lets
 * every existing row skip re-render when a new line arrives — only the
 * appended rows mount.
 */
export const LogRow = memo(function LogRow({ entry }: { entry: LogEntryDto }) {
  const cls = LEVEL_CLS[entry.level] ?? "text-[var(--fg-secondary)]";
  const ts = new Date(entry.ts_ms).toISOString().slice(11, 23);
  return (
    <div className="flex gap-3 border-b border-[var(--border-subtle)]/40 px-3 py-1 font-mono text-xs hover:bg-[var(--bg-elevated)]">
      <span className="w-20 shrink-0 text-[var(--fg-muted)]">{ts}</span>
      <span className={`w-12 shrink-0 font-semibold ${cls}`}>{entry.level}</span>
      <span className="w-32 shrink-0 truncate text-[var(--fg-muted)]">{entry.target}</span>
      <span className="truncate text-[var(--fg-primary)]">{entry.message}</span>
    </div>
  );
});
