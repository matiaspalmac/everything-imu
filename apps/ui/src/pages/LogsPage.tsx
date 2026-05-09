import { Pause, Play } from "@phosphor-icons/react";
import { useEffect, useMemo, useRef } from "react";
import { LogRow } from "../components/LogRow";
import { useLogStore } from "../stores/useLogStore";

const LEVEL_ORDER = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"];

export function LogsPage() {
  const entries = useLogStore((s) => s.entries);
  const filterLevel = useLogStore((s) => s.filterLevel);
  const filterText = useLogStore((s) => s.filterText);
  const paused = useLogStore((s) => s.paused);
  const follow = useLogStore((s) => s.follow);
  const setFilterLevel = useLogStore((s) => s.setFilterLevel);
  const setFilterText = useLogStore((s) => s.setFilterText);
  const setPaused = useLogStore((s) => s.setPaused);
  const setFollow = useLogStore((s) => s.setFollow);
  const scrollRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    const min = LEVEL_ORDER.indexOf(filterLevel.toUpperCase());
    return entries.filter((e) => {
      const lv = LEVEL_ORDER.indexOf(e.level.toUpperCase());
      if (lv < min) return false;
      if (filterText && !e.message.toLowerCase().includes(filterText.toLowerCase())) return false;
      return true;
    });
  }, [entries, filterLevel, filterText]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: autoscroll triggered by content change
  useEffect(() => {
    if (!follow) return;
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [filtered.length, follow]);

  return (
    <div className="flex h-full flex-col gap-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
          Logs
        </h2>
        <div className="flex items-center gap-3 text-xs text-[var(--fg-muted)]">
          <span>{filtered.length.toLocaleString()} shown</span>
          <span>·</span>
          <span>{entries.length.toLocaleString()} captured</span>
        </div>
      </div>

      <div className="flex flex-wrap gap-2">
        <select
          value={filterLevel}
          onChange={(e) => setFilterLevel(e.target.value)}
          className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-2 py-1.5 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
        >
          {LEVEL_ORDER.map((l) => (
            <option key={l} value={l.toLowerCase()}>
              {l.toLowerCase()}
            </option>
          ))}
        </select>
        <input
          type="text"
          placeholder="Filter…"
          value={filterText}
          onChange={(e) => setFilterText(e.target.value)}
          className="flex-1 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-1.5 text-sm text-[var(--fg-primary)] focus:border-[var(--accent)] focus:outline-none"
        />
        <ToggleButton
          on={!paused}
          onClick={() => setPaused(!paused)}
          labelOn="Streaming"
          labelOff="Paused"
          iconOn={<Pause size={14} />}
          iconOff={<Play size={14} />}
        />
        <label className="flex items-center gap-2 text-xs text-[var(--fg-secondary)]">
          <input
            type="checkbox"
            checked={follow}
            onChange={(e) => setFollow(e.target.checked)}
            className="accent-[var(--accent)]"
          />
          Follow
        </label>
      </div>

      <div
        ref={scrollRef}
        className="flex-1 overflow-auto rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)]"
      >
        {filtered.map((e, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: log rows are append-only and identical lines collide on content alone
          <LogRow key={`${e.ts_ms}-${i}`} entry={e} />
        ))}
        {filtered.length === 0 && (
          <div className="p-6 text-center text-sm text-[var(--fg-muted)]">
            No log entries match filter.
          </div>
        )}
      </div>
    </div>
  );
}

function ToggleButton({
  on,
  onClick,
  labelOn,
  labelOff,
  iconOn,
  iconOff,
}: {
  on: boolean;
  onClick: () => void;
  labelOn: string;
  labelOff: string;
  iconOn: React.ReactNode;
  iconOff: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1.5 rounded-[var(--radius-sm)] border px-2 py-1.5 text-xs transition-colors ${
        on
          ? "border-[var(--success)]/30 bg-[var(--success)]/10 text-[var(--success)]"
          : "border-[var(--warn)]/30 bg-[var(--warn-soft)] text-[var(--warn)]"
      }`}
    >
      {on ? iconOn : iconOff}
      {on ? labelOn : labelOff}
    </button>
  );
}
