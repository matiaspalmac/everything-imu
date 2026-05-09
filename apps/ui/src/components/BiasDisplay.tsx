/**
 * Live VQF gyro-bias estimate read-out. Bias is shown in mrad/s
 * (millirad / s) since values are typically small (< 0.05 rad/s for a
 * warmed-up Joy-Con).
 */
export function BiasDisplay({ bias }: { bias: [number, number, number] | null }) {
  if (!bias) {
    return (
      <div className="rounded-[var(--radius-md)] border border-dashed border-[var(--border-subtle)] p-3 text-center text-xs text-[var(--fg-muted)]">
        No bias estimate yet — VQF still warming up.
      </div>
    );
  }
  const labels = ["x", "y", "z"] as const;
  return (
    <div className="grid grid-cols-3 gap-2">
      {labels.map((axis, i) => {
        const mrad = bias[i] * 1000;
        return (
          <div
            key={axis}
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-2 py-1.5"
          >
            <div className="text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">
              gyr bias {axis}
            </div>
            <div className="font-mono text-sm text-[var(--fg-primary)]">
              {mrad.toFixed(2)}
              <span className="ml-1 text-[10px] text-[var(--fg-muted)]">mrad/s</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
