import { useTranslation } from "react-i18next";
import { macKey } from "../../lib/macFormat";
import { useLatencyStore } from "../../stores/useLatencyStore";

/// Compact connection-quality bars + packet-loss% + rate. Reads the
/// latency stream (already running) and derives loss as
/// `dropped_estimate / (dropped_estimate + samples_window)`. The four
/// bar levels are quantized off the same loss number so the badge and
/// the numeric copy can never disagree.
export function SignalMeter({
  mac,
  rateHz,
  targetHz,
  compact = false,
}: {
  mac: [number, number, number, number, number, number];
  rateHz: number;
  targetHz: number;
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const entry = useLatencyStore((s) => s.perMac[macKey(mac)]);
  const dropped = entry?.dropped_estimate ?? 0;
  const window = entry?.samples_window ?? 0;
  const total = dropped + window;
  const lossFrac = total > 0 ? dropped / total : 0;
  const lossPct = lossFrac * 100;

  // Bar quality bins. Tuned for SlimeVR-style 60..200 Hz trackers — a
  // few dropped frames per second is unhealthy at 200 Hz but normal at
  // 60 Hz, so the rate-deficit ratio matters as much as raw loss.
  const rateRatio = targetHz > 0 ? rateHz / targetHz : 1;
  let level: 0 | 1 | 2 | 3 | 4 = 4;
  if (rateRatio < 0.4 || lossFrac > 0.2) level = 1;
  else if (rateRatio < 0.7 || lossFrac > 0.1) level = 2;
  else if (rateRatio < 0.9 || lossFrac > 0.03) level = 3;
  if (rateHz <= 0) level = 0;

  const color = level <= 1 ? "var(--warn)" : level === 2 ? "var(--accent)" : "var(--success)";

  return (
    <span
      className="inline-flex items-center gap-1.5 text-[11px] text-[var(--fg-secondary)]"
      title={t("signal.tooltip", {
        rate: rateHz.toFixed(0),
        target: targetHz,
        loss: lossPct.toFixed(1),
      })}
    >
      <span className="inline-flex items-end gap-[2px]" aria-hidden>
        {[1, 2, 3, 4].map((i) => (
          <span
            key={i}
            style={{
              display: "inline-block",
              width: 3,
              height: 4 + i * 2,
              background: level >= i ? color : "var(--border-subtle)",
              borderRadius: 1,
              transition: "background 200ms ease-out",
            }}
          />
        ))}
      </span>
      {!compact && (
        <>
          <span className="metric-num font-mono" style={{ color }}>
            {rateHz.toFixed(0)} Hz
          </span>
          {lossPct >= 0.5 && (
            <span className="metric-num font-mono text-[var(--warn)]">
              · {lossPct.toFixed(1)}% {t("signal.loss")}
            </span>
          )}
        </>
      )}
    </span>
  );
}
