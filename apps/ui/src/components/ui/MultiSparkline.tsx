import { useMemo } from "react";

type Series = { values: number[]; color: string; label: string };

/**
 * Multi-series inline SVG line chart. Lightweight (no chart lib).
 * Used for live gyro/accel graphs in the tracker detail page.
 *
 * All series share the y-axis range computed from union(min,max). Time
 * axis is implicit (sample index).
 */
export function MultiSparkline({
  series,
  width = 360,
  height = 80,
  showLegend = true,
}: {
  series: Series[];
  width?: number;
  height?: number;
  showLegend?: boolean;
}) {
  const { paths, range } = useMemo(() => {
    const all = series.flatMap((s) => s.values);
    if (all.length === 0) {
      return { paths: series.map(() => ""), range: [0, 0] as [number, number] };
    }
    const max = Math.max(...all);
    const min = Math.min(...all);
    const span = max - min || 1;
    const paths = series.map((s) => {
      if (s.values.length === 0) return "";
      const stepX = s.values.length === 1 ? 0 : width / (s.values.length - 1);
      return s.values
        .map((v, i) => {
          const x = i * stepX;
          const y = height - ((v - min) / span) * (height - 2) - 1;
          return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
        })
        .join(" ");
    });
    return { paths, range: [min, max] as [number, number] };
  }, [series, width, height]);

  return (
    <div className="flex flex-col gap-1">
      {/* Width/height props define the viewBox coordinate space; the SVG
          itself stretches to its container so charts survive narrow tiles. */}
      <svg
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label="multi-axis sparkline"
        className="h-auto w-full rounded-[var(--radius-sm)] bg-[var(--bg-elevated)]/50"
      >
        <line
          x1={0}
          x2={width}
          y1={height / 2}
          y2={height / 2}
          stroke="var(--border-subtle)"
          strokeDasharray="2 2"
        />
        {series.map((s, idx) => (
          <path key={s.label} d={paths[idx] ?? ""} fill="none" stroke={s.color} strokeWidth={1.4} />
        ))}
      </svg>
      {showLegend && (
        <div className="flex items-center gap-3 text-[10px] text-[var(--fg-muted)]">
          {series.map((s) => (
            <span key={s.label} className="flex items-center gap-1">
              <span className="inline-block size-2 rounded-full" style={{ background: s.color }} />
              {s.label}
            </span>
          ))}
          <span className="ml-auto font-mono text-[var(--fg-secondary)]">
            [{range[0].toFixed(2)}, {range[1].toFixed(2)}]
          </span>
        </div>
      )}
    </div>
  );
}
