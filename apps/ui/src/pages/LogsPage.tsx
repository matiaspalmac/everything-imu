import { ArrowDown, ListBullets, MagnifyingGlass, Pause, Play } from "@phosphor-icons/react";
import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "../components/EmptyState";
import { LogRow } from "../components/LogRow";
import { useLogStore } from "../stores/useLogStore";

const LEVEL_ORDER = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"] as const;
type Level = (typeof LEVEL_ORDER)[number];

const LEVEL_TONE: Record<Level, string> = {
  TRACE: "var(--fg-muted)",
  DEBUG: "var(--info)",
  INFO: "var(--success)",
  WARN: "var(--warn)",
  ERROR: "var(--danger)",
};

// Cap how many DOM rows we render at once. The store keeps the full
// buffer; we only paint the tail. Cheap-and-correct windowing without
// pulling react-window. If anyone wants to scroll back further they
// should narrow the level filter.
const MAX_RENDERED_ROWS = 800;

export function LogsPage() {
  const { t } = useTranslation();
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
    const min = LEVEL_ORDER.indexOf(filterLevel.toUpperCase() as Level);
    const needle = filterText.toLowerCase();
    return entries.filter((e) => {
      const lv = LEVEL_ORDER.indexOf(e.level.toUpperCase() as Level);
      if (lv < min) return false;
      if (needle && !`${e.message} ${e.target}`.toLowerCase().includes(needle)) return false;
      return true;
    });
  }, [entries, filterLevel, filterText]);

  const trimmed =
    filtered.length > MAX_RENDERED_ROWS ? filtered.slice(-MAX_RENDERED_ROWS) : filtered;
  const hiddenAbove = filtered.length - trimmed.length;

  // biome-ignore lint/correctness/useExhaustiveDependencies: autoscroll triggered by content change
  useEffect(() => {
    if (!follow) return;
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [trimmed.length, follow]);

  // Level counts for the chip badges. O(N) but N is bounded by the
  // store's own ring buffer so it stays cheap.
  const counts = useMemo(() => {
    const out: Record<Level, number> = { TRACE: 0, DEBUG: 0, INFO: 0, WARN: 0, ERROR: 0 };
    for (const e of entries) {
      const k = e.level.toUpperCase() as Level;
      if (out[k] !== undefined) out[k] += 1;
    }
    return out;
  }, [entries]);

  return (
    <div className="flex h-full flex-col gap-3">
      <header className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("logs.title")}
        </h2>
        <div className="flex items-center gap-3 text-[11px] text-[var(--fg-muted)]">
          <span>{t("status.shown", { count: filtered.length })}</span>
          <span>·</span>
          <span>{t("status.captured", { count: entries.length })}</span>
        </div>
      </header>

      {/* Sticky toolbar — survives scroll so the user always has filters within reach. */}
      <div className="sticky top-0 z-10 flex flex-col gap-2 rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-2 shadow-[var(--shadow-card)] backdrop-blur">
        <div className="flex items-center gap-2">
          <div className="flex flex-1 items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3">
            <MagnifyingGlass size={13} className="text-[var(--fg-muted)]" />
            <input
              type="text"
              aria-label={t("labels.filter")}
              placeholder={t("labels.filter")}
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              className="flex-1 bg-transparent py-1.5 text-sm text-[var(--fg-primary)] placeholder:text-[var(--fg-muted)] focus:outline-none"
            />
          </div>
          <ToggleButton
            on={!paused}
            onClick={() => setPaused(!paused)}
            labelOn={t("labels.streaming")}
            labelOff={t("labels.paused")}
            iconOn={<Pause size={12} />}
            iconOff={<Play size={12} />}
          />
          <label className="flex items-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1.5 text-[11px] text-[var(--fg-secondary)]">
            <input
              type="checkbox"
              aria-label={t("labels.follow")}
              checked={follow}
              onChange={(e) => setFollow(e.target.checked)}
              className="size-3.5 accent-[var(--accent)]"
            />
            <ArrowDown size={11} />
            {t("labels.follow")}
          </label>
        </div>

        <div className="flex flex-wrap items-center gap-1.5">
          {LEVEL_ORDER.map((lv) => {
            const active = filterLevel.toUpperCase() === lv;
            return (
              <button
                key={lv}
                type="button"
                onClick={() => setFilterLevel(lv.toLowerCase())}
                style={{ ["--tone" as never]: LEVEL_TONE[lv] }}
                className={`flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide transition-colors ${
                  active
                    ? "border-[var(--tone)] bg-[var(--tone)]/15 text-[var(--tone)]"
                    : "border-[var(--border-subtle)] text-[var(--fg-muted)] hover:border-[var(--border-strong)] hover:text-[var(--fg-secondary)]"
                }`}
              >
                <span
                  aria-hidden
                  className="size-1.5 rounded-full"
                  style={{ background: LEVEL_TONE[lv] }}
                />
                {lv}
                <span className="metric-num font-normal text-[10px] opacity-70">{counts[lv]}</span>
              </button>
            );
          })}
        </div>
      </div>

      <div
        ref={scrollRef}
        className="flex-1 overflow-auto rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)]"
      >
        {hiddenAbove > 0 && (
          <div className="border-b border-dashed border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-1.5 text-center text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
            {t("logs.older_hidden", { count: hiddenAbove })}
          </div>
        )}
        {trimmed.map((e) => (
          // Composite key: timestamp + target + first 24 chars of message.
          // Log rows are append-only and never reordered, and React only
          // needs the key to be stable within the rendered list — exact
          // dupes within the same millisecond on the same target collapse
          // visually but don't break correctness.
          <LogRow key={`${e.ts_ms}-${e.target}-${e.message.slice(0, 24)}`} entry={e} />
        ))}
        {trimmed.length === 0 && (
          <div className="p-4">
            <EmptyState
              icon={ListBullets}
              title={t("empty.no_logs_title")}
              description={t("empty.no_logs_desc")}
              compact
            />
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
      className={`flex items-center gap-1.5 rounded-[var(--radius-sm)] border px-2 py-1.5 text-[11px] transition-colors ${
        on
          ? "border-[var(--success)]/30 bg-[var(--success-soft)] text-[var(--success)]"
          : "border-[var(--warn)]/30 bg-[var(--warn-soft)] text-[var(--warn)]"
      }`}
    >
      {on ? iconOn : iconOff}
      {on ? labelOn : labelOff}
    </button>
  );
}
