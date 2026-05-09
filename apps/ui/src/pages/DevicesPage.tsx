import { useNavigate } from "react-router-dom";
import type { DeviceMetadataDto } from "../api/client";
import { api } from "../api/client";
import { macHex, macKey as macKeyFn } from "../lib/macFormat";
import { useDeviceStore } from "../stores/useDeviceStore";

export function DevicesPage() {
  const devices = useDeviceStore((s) => s.devices);
  const navigate = useNavigate();
  const list = Object.values(devices);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
          Devices
        </h2>
        <span className="text-xs text-[var(--fg-muted)]">{list.length} known</span>
      </div>
      <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)]">
        <table className="w-full text-sm">
          <thead className="bg-[var(--bg-elevated)] text-xs uppercase tracking-wide text-[var(--fg-muted)]">
            <tr>
              <th className="px-4 py-3 text-left">MAC</th>
              <th className="px-4 py-3 text-left">Serial</th>
              <th className="px-4 py-3 text-left">Kind</th>
              <th className="px-4 py-3 text-left">Capabilities</th>
              <th className="px-4 py-3 text-right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {list.map((d) => (
              <Row
                key={macHex(d.mac)}
                d={d}
                onOpen={() => navigate(`/devices/${macKeyFn(d.mac)}`)}
              />
            ))}
            {list.length === 0 && (
              <tr>
                <td className="px-4 py-8 text-center text-[var(--fg-muted)]" colSpan={5}>
                  No devices detected. Pair a Joy-Con or enable synthetic mode.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Row({ d, onOpen }: { d: DeviceMetadataDto; onOpen: () => void }) {
  const caps: string[] = [];
  if (d.has_magnetometer) caps.push("mag");
  if (d.has_battery) caps.push("battery");
  if (d.has_rumble) caps.push("rumble");
  if (d.firmware) caps.push(d.firmware);
  return (
    <tr
      className="cursor-pointer border-t border-[var(--border-subtle)] transition-colors hover:bg-[var(--warn-soft)]"
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter") onOpen();
      }}
    >
      <td className="px-4 py-3 font-mono text-[var(--fg-primary)]">{macHex(d.mac)}</td>
      <td className="px-4 py-3 text-[var(--fg-secondary)]">{d.serial}</td>
      <td className="px-4 py-3 text-[var(--fg-secondary)]">{d.kind}</td>
      <td className="px-4 py-3 text-[var(--fg-secondary)]">
        {caps.length === 0 ? "—" : caps.join(" · ")}
      </td>
      <td className="space-x-2 px-4 py-3 text-right">
        <button
          type="button"
          className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs font-medium text-[var(--fg-primary)] transition-colors hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
          onClick={(e) => {
            e.stopPropagation();
            void api.requestReset(d.mac, "yaw");
          }}
        >
          Reset Yaw
        </button>
        <button
          type="button"
          className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs font-medium text-[var(--fg-primary)] transition-colors hover:bg-[var(--warn-soft)] hover:text-[var(--accent)]"
          onClick={(e) => {
            e.stopPropagation();
            void api.requestReset(d.mac, "full");
          }}
        >
          Reset Full
        </button>
      </td>
    </tr>
  );
}
