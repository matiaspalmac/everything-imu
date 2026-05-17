export function StatusBadge({ rateHz, targetHz }: { rateHz: number; targetHz: number }) {
  let cls = "bg-rose-500/10 text-rose-300 ring-rose-500/30";
  let label = "no imu";
  if (rateHz >= targetHz * 0.9) {
    cls = "bg-[var(--accent-soft)] text-[var(--accent)] ring-[var(--accent)]/30";
    label = `${rateHz.toFixed(0)} Hz`;
  } else if (rateHz > 0) {
    cls =
      "bg-amber-100 text-amber-700 ring-amber-600/20 dark:bg-amber-500/10 dark:text-amber-300 dark:ring-amber-500/30";
    label = `${rateHz.toFixed(0)} Hz · laggy`;
  }
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-[11px] font-medium ring-1 ring-inset ${cls}`}
    >
      {label}
    </span>
  );
}
