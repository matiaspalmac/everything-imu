type Shortcut = { keys: string; action: string };

const KEYBOARD: Shortcut[] = [
  { keys: "Ctrl + K", action: "Open the command palette" },
  { keys: "R", action: "Broadcast Yaw Reset to all trackers" },
  { keys: "Shift + R", action: "Broadcast Full Reset to all trackers" },
  { keys: "Esc", action: "Close palette / cancel" },
];

const FAQ: { q: string; a: string }[] = [
  {
    q: "Why does closing the window not exit?",
    a: "The bridge keeps running in the background to maintain the SlimeVR-Server connection. Use the tray menu → Quit to exit.",
  },
  {
    q: "Tracker shows up but does not move",
    a: "Check the tracker rate badge in the dashboard. If it sticks at 0 Hz, the device is paired but no IMU samples are flowing — typical causes: another app holds the HID handle, or the device went to sleep.",
  },
  {
    q: "Mounting orientation is wrong",
    a: "TrackerDetail page → Per-device configuration → Mounting orientation. Mounting changes apply live; fusion algo changes apply on the next reconnect.",
  },
  {
    q: "Where are the logs?",
    a: "Settings → Diagnostics → Open logs folder. Daily rotation, 7-day retention.",
  },
  {
    q: "What lives where?",
    a: "This bridge owns: device discovery, IMU fusion, mounting offset, packet emission to SlimeVR-Server. SlimeVR-Server owns: skeleton, body proportions, mounting calibration math, SteamVR. Don't expect the bridge to replicate SlimeVR features.",
  },
];

export function HelpPage() {
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
        Help
      </h2>

      <section>
        <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
          Keyboard shortcuts
        </h3>
        <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)]">
          <table className="w-full text-sm">
            <tbody>
              {KEYBOARD.map((s) => (
                <tr
                  key={s.keys}
                  className="border-b border-[var(--border-subtle)]/40 last:border-b-0"
                >
                  <td className="w-48 px-4 py-2 font-mono text-[var(--accent)]">{s.keys}</td>
                  <td className="px-4 py-2 text-[var(--fg-secondary)]">{s.action}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section>
        <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
          FAQ
        </h3>
        <div className="flex flex-col gap-3">
          {FAQ.map((item) => (
            <div
              key={item.q}
              className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4"
            >
              <div className="text-sm font-semibold text-[var(--fg-primary)]">{item.q}</div>
              <p className="mt-1 text-xs text-[var(--fg-secondary)]">{item.a}</p>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
