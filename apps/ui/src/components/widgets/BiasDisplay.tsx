/**
 * Live VQF gyro-bias estimate — needle gauge per axis.
 *
 * Bias is shown in millirad/s. Typical warmed-up values sit well under
 * ±50 mrad/s, so the gauge range is clamped to ±100 mrad/s with the
 * needle saturating at the edges and the wedge tinting toward warn when
 * the magnitude grows past 50 (a sign the filter is still settling, or
 * that the device drifted).
 */
const AXIS_HUE = {
  x: "var(--danger)",
  y: "var(--success)",
  z: "var(--info)",
} as const;
const MAX_MRAD = 100;
const WARN_MRAD = 50;

export function BiasDisplay({ bias }: { bias: [number, number, number] | null }) {
  if (!bias) {
    return (
      <div className="rounded-[var(--radius-md)] border border-dashed border-[var(--border-subtle)] p-3 text-center text-xs text-[var(--fg-muted)]">
        No bias estimate yet: VQF still warming up.
      </div>
    );
  }
  const labels = ["x", "y", "z"] as const;
  return (
    <div className="grid grid-cols-3 gap-2">
      {labels.map((axis, i) => {
        const mrad = bias[i] * 1000;
        const clamped = Math.max(-MAX_MRAD, Math.min(MAX_MRAD, mrad));
        // -90deg..+90deg sweep
        const angle = (clamped / MAX_MRAD) * 90;
        const warn = Math.abs(mrad) > WARN_MRAD;
        const tone = warn ? "var(--warn)" : AXIS_HUE[axis];
        return (
          <div
            key={axis}
            className="flex flex-col items-center gap-1 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-base)] p-2"
          >
            <Gauge angle={angle} tone={tone} axis={axis} />
            <div className="metric-num font-mono text-[11px] text-[var(--fg-primary)]">
              {mrad >= 0 ? "+" : ""}
              {mrad.toFixed(1)}
              <span className="ml-0.5 text-[9px] text-[var(--fg-muted)]">mrad/s</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function Gauge({ angle, tone, axis }: { angle: number; tone: string; axis: string }) {
  // Semi-circle gauge from -90° (left) to +90° (right). Needle is an
  // SVG line rotated around the bottom center.
  const size = 72;
  const cx = size / 2;
  const cy = size * 0.62;
  const r = size * 0.42;
  // Background arc path (start at 9 o'clock, sweep to 3 o'clock through top)
  const arc = `M ${cx - r} ${cy} A ${r} ${r} 0 0 1 ${cx + r} ${cy}`;
  // Active wedge: from center, draw arc proportional to |angle|/90, on
  // the same side as the needle. Skipped when angle ≈ 0.
  return (
    <svg width={size} height={size * 0.7} viewBox={`0 0 ${size} ${size * 0.7}`}>
      <title>{`gyr bias ${axis}`}</title>
      <path d={arc} fill="none" stroke="var(--border-subtle)" strokeWidth={3} />
      {Math.abs(angle) > 1 && <Wedge cx={cx} cy={cy} r={r} angle={angle} tone={tone} />}
      <line
        x1={cx}
        y1={cy}
        x2={cx}
        y2={cy - r}
        stroke={tone}
        strokeWidth={1.5}
        strokeLinecap="round"
        transform={`rotate(${angle} ${cx} ${cy})`}
        style={{ transition: "transform 0.3s ease, stroke 0.3s ease" }}
      />
      <circle cx={cx} cy={cy} r={2.5} fill={tone} />
      <text
        x={cx}
        y={cy + 11}
        textAnchor="middle"
        fontSize="9"
        fill="var(--fg-muted)"
        fontFamily="monospace"
      >
        {axis.toUpperCase()}
      </text>
    </svg>
  );
}

function Wedge({
  cx,
  cy,
  r,
  angle,
  tone,
}: {
  cx: number;
  cy: number;
  r: number;
  angle: number;
  tone: string;
}) {
  // Build the active arc between -90° and the needle angle (or needle to +90°).
  const a1 = (-90 * Math.PI) / 180;
  const a2 = ((angle - 90) * Math.PI) / 180;
  const start = { x: cx + r * Math.cos(a1), y: cy + r * Math.sin(a1) };
  const end = { x: cx + r * Math.cos(a2), y: cy + r * Math.sin(a2) };
  const sweep = angle >= 0 ? 1 : 0;
  const startPt = angle >= 0 ? start : end;
  const endPt = angle >= 0 ? end : start;
  return (
    <path
      d={`M ${startPt.x} ${startPt.y} A ${r} ${r} 0 0 ${sweep} ${endPt.x} ${endPt.y}`}
      fill="none"
      stroke={tone}
      strokeWidth={3}
      strokeLinecap="round"
      opacity={0.7}
    />
  );
}
