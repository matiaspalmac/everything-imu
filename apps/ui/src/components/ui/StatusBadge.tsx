export function StatusBadge({ rateHz, targetHz }: { rateHz: number; targetHz: number }) {
  let cls = "bg-[var(--danger-soft)] text-[var(--danger)] ring-[var(--danger)]/30";
  let label = "no imu";
  if (rateHz >= targetHz * 0.9) {
    cls = "bg-[var(--success-soft)] text-[var(--success)] ring-[var(--success)]/30";
    label = `${rateHz.toFixed(0)} Hz`;
  } else if (rateHz > 0) {
    cls = "bg-[var(--warn-soft)] text-[var(--warn)] ring-[var(--warn)]/30";
    label = `${rateHz.toFixed(0)} / ${targetHz} Hz`;
  }
  return (
    <span
      className={`metric-num inline-flex items-center rounded-full px-2.5 py-0.5 text-[11px] font-medium ring-1 ring-inset ${cls}`}
    >
      {label}
    </span>
  );
}
