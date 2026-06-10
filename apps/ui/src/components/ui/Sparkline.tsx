import { useMemo } from "react";

/**
 * Inline SVG sparkline. Lightweight (no chart lib) — used inside cards
 * to show recent rate_hz history per tracker. Pass at most ~120 points.
 */
export function Sparkline({
  values,
  width = 120,
  height = 32,
  stroke = "currentColor",
}: {
  values: number[];
  width?: number;
  height?: number;
  stroke?: string;
}) {
  const path = useMemo(() => {
    if (values.length === 0) return "";
    const max = Math.max(1, ...values);
    const min = Math.min(0, ...values);
    const range = max - min || 1;
    const stepX = values.length === 1 ? 0 : width / (values.length - 1);
    return values
      .map((v, i) => {
        const x = i * stepX;
        const y = height - ((v - min) / range) * (height - 2) - 1;
        return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(" ");
  }, [values, width, height]);

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="text-[var(--accent)]"
      role="img"
      aria-label="rate sparkline"
    >
      <path d={path} fill="none" stroke={stroke} strokeWidth={1.5} />
    </svg>
  );
}
