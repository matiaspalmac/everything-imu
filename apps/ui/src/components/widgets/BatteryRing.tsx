import { BatteryHigh, BatteryLow } from "@phosphor-icons/react";

/// Compact battery indicator. Ring fills counter-clockwise from 12 o'clock
/// using stroke-dasharray, so it animates smoothly when fraction changes.
/// Renders nothing for non-finite / zero fractions (lots of HID drivers
/// surface NaN until the first battery feature report lands).
export function BatteryRing({
  fraction,
  size = 22,
  showPct = false,
}: {
  fraction: number;
  size?: number;
  showPct?: boolean;
}) {
  if (!Number.isFinite(fraction) || fraction <= 0) return null;
  const clamped = Math.max(0, Math.min(1, fraction));
  const pct = Math.round(clamped * 100);
  const stroke = 2.5;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const dash = clamped * c;
  const low = clamped < 0.15;
  const mid = clamped < 0.4;
  const color = low ? "var(--warn)" : mid ? "var(--accent)" : "var(--success)";

  return (
    <span className="inline-flex items-center gap-1.5" title={`${pct}%`}>
      <span className="relative inline-flex" style={{ width: size, height: size }}>
        <svg
          width={size}
          height={size}
          viewBox={`0 0 ${size} ${size}`}
          className="-rotate-90"
          role="img"
          aria-label={`${pct}%`}
        >
          <title>{`${pct}%`}</title>
          <circle
            cx={size / 2}
            cy={size / 2}
            r={r}
            fill="none"
            stroke="var(--border-subtle)"
            strokeWidth={stroke}
          />
          <circle
            cx={size / 2}
            cy={size / 2}
            r={r}
            fill="none"
            stroke={color}
            strokeWidth={stroke}
            strokeDasharray={`${dash} ${c - dash}`}
            strokeLinecap="round"
            style={{ transition: "stroke-dasharray 240ms ease-out" }}
          />
        </svg>
        <span className="absolute inset-0 flex items-center justify-center">
          {low ? (
            <BatteryLow size={size * 0.55} color={color} weight="duotone" />
          ) : (
            <BatteryHigh size={size * 0.55} color={color} weight="duotone" />
          )}
        </span>
      </span>
      {showPct && (
        <span className="metric-num font-mono text-[11px]" style={{ color }}>
          {pct}%
        </span>
      )}
    </span>
  );
}
