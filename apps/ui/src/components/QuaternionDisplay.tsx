import { quatToEulerDeg } from "../lib/quat";

const COMPONENTS = ["x", "y", "z", "w"] as const;

export function QuaternionDisplay({ quat }: { quat: [number, number, number, number] }) {
  const euler = quatToEulerDeg(quat);
  return (
    <div className="flex flex-col gap-3">
      <div>
        <div className="mb-2 text-[10px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
          Quaternion (XYZW)
        </div>
        <div className="grid grid-cols-4 gap-2">
          {COMPONENTS.map((label, i) => (
            <Bar key={label} label={label} value={quat[i] ?? 0} />
          ))}
        </div>
      </div>
      <div>
        <div className="mb-2 text-[10px] font-medium uppercase tracking-wide text-[var(--fg-muted)]">
          Euler (ZYX, deg)
        </div>
        <div className="grid grid-cols-3 gap-2 text-sm">
          <EulerCell label="roll" value={euler.roll} />
          <EulerCell label="pitch" value={euler.pitch} />
          <EulerCell label="yaw" value={euler.yaw} />
        </div>
      </div>
    </div>
  );
}

function Bar({ label, value }: { label: string; value: number }) {
  const clamped = Math.max(-1, Math.min(1, value));
  const pct = ((clamped + 1) / 2) * 100;
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between font-mono text-xs">
        <span className="text-[var(--fg-secondary)]">{label}</span>
        <span className="text-[var(--fg-primary)]">{value.toFixed(3)}</span>
      </div>
      <div className="relative h-1.5 overflow-hidden rounded bg-[var(--bg-elevated)]">
        <div className="absolute inset-y-0 left-1/2 w-px bg-[var(--border-strong)]" />
        <div
          className="absolute inset-y-0 bg-[var(--accent)]"
          style={
            clamped >= 0
              ? { left: "50%", width: `${(pct - 50).toFixed(1)}%` }
              : { right: "50%", width: `${(50 - pct).toFixed(1)}%` }
          }
        />
      </div>
    </div>
  );
}

function EulerCell({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-2 py-1.5">
      <div className="text-[10px] uppercase tracking-wide text-[var(--fg-muted)]">{label}</div>
      <div className="font-mono text-sm text-[var(--fg-primary)]">{value.toFixed(1)}°</div>
    </div>
  );
}
