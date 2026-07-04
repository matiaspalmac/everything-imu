import { ArrowsClockwise, Crosshair, Eye, Pause, Play, Plugs, Target } from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router-dom";
import { api, type Mac, type TrackerSnapshot } from "../api/client";
import { EmptyState } from "../components/ui/EmptyState";
import { ConnectionStatusCard } from "../components/widgets/ConnectionStatusCard";
import { LatencySummary } from "../components/widgets/LatencyPanel";
import { TrackerCard } from "../components/widgets/TrackerCard";
import { macKey as macKeyFn } from "../lib/macFormat";
import { useDeviceStore } from "../stores/useDeviceStore";
import { usePerDeviceSettingsStore } from "../stores/usePerDeviceSettingsStore";
import { useToastStore } from "../stores/useToastStore";
import { useTrackerStore } from "../stores/useTrackerStore";

export function DashboardPage() {
  const { t } = useTranslation();
  const trackers = useTrackerStore((s) => s.trackers);
  const devices = useDeviceStore((s) => s.devices);
  const perDevSettings = usePerDeviceSettingsStore((s) => s.perMac);
  const ensure = usePerDeviceSettingsStore((s) => s.ensure);
  const patch = usePerDeviceSettingsStore((s) => s.patch);
  const pushToast = useToastStore((s) => s.push);
  const navigate = useNavigate();

  // Live-view freeze: capturing the store snapshot by reference is enough —
  // tracker snapshots are immutable, so the frozen map keeps rendering the
  // exact poses from the moment of the click while the store keeps moving.
  const [frozenTrackers, setFrozenTrackers] = useState<typeof trackers | null>(null);
  const viewPaused = frozenTrackers !== null;
  const displayTrackers = frozenTrackers ?? trackers;

  const rawList = useMemo(() => Object.values(displayTrackers), [displayTrackers]);

  // Hydrate per-device settings once per tracker.
  useEffect(() => {
    for (const snap of rawList) void ensure(snap.mac);
  }, [rawList, ensure]);

  const visibleList = useMemo(() => {
    return rawList
      .filter((s) => !perDevSettings[macKeyFn(s.mac)]?.hidden)
      .sort((a, b) => {
        const oa = perDevSettings[macKeyFn(a.mac)]?.display_order ?? 0;
        const ob = perDevSettings[macKeyFn(b.mac)]?.display_order ?? 0;
        if (oa !== ob) return oa - ob;
        return macKeyFn(a.mac).localeCompare(macKeyFn(b.mac));
      });
  }, [rawList, perDevSettings]);

  const hiddenCount = rawList.length - visibleList.length;

  // Group buckets, preserving group order by first appearance.
  const groupedList = useMemo(() => {
    const buckets = new Map<string, TrackerSnapshot[]>();
    for (const snap of visibleList) {
      const g = perDevSettings[macKeyFn(snap.mac)]?.group?.trim() ?? "";
      const k = g || "__ungrouped";
      const existing = buckets.get(k);
      if (existing) {
        existing.push(snap);
      } else {
        buckets.set(k, [snap]);
      }
    }
    return Array.from(buckets.entries()).map(([k, items]) => ({
      group: k === "__ungrouped" ? "" : k,
      items,
    }));
  }, [visibleList, perDevSettings]);

  function broadcastReset(kind: "yaw" | "full" | "mounting", subset?: TrackerSnapshot[]) {
    const target = subset ?? visibleList;
    for (const snap of target) {
      void api.requestReset(snap.mac, kind);
    }
  }

  async function unhideAll() {
    const hidden = rawList.filter((s) => perDevSettings[macKeyFn(s.mac)]?.hidden);
    await Promise.all(
      hidden.map(async (s) => {
        patch(s.mac, { hidden: false });
        await api.setTrackerHidden(s.mac, false);
      }),
    );
  }

  return (
    <div className="flex flex-col gap-6">
      {/* Bento hero row — connection (1fr), latency (1.4fr), broadcast (1fr)
          on xl. At md the latency tile takes a full row (it's the widest
          content) and connection + broadcast pair up; below md everything
          stacks. Latency is the visual anchor because that's the bridge's
          real-time health signal. */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-[1fr_1.4fr_1fr]">
        <BentoTile title={t("pages.connection")} accent>
          <ConnectionStatusCard />
        </BentoTile>
        <BentoTile
          title={t("pages.bridge_latency")}
          feature
          className="md:col-span-2 xl:col-span-1"
        >
          <LatencySummary />
        </BentoTile>
        <BentoTile title={t("pages.broadcast_actions")}>
          <div className="grid grid-cols-1 gap-2">
            <ResetButton
              label={t("actions.yaw_reset")}
              icon={<Crosshair size={18} weight="duotone" />}
              onClick={() => broadcastReset("yaw")}
              disabled={visibleList.length === 0}
            />
            <ResetButton
              label={t("actions.full_reset")}
              icon={<ArrowsClockwise size={18} weight="duotone" />}
              onClick={() => broadcastReset("full")}
              disabled={visibleList.length === 0}
            />
            <ResetButton
              label={t("actions.mounting_short")}
              icon={<Target size={18} weight="duotone" />}
              onClick={() => broadcastReset("mounting")}
              disabled={visibleList.length === 0}
            />
          </div>
          <div className="mt-2 text-[11px] text-[var(--fg-muted)]">
            {t(
              visibleList.length === 1
                ? "hints.broadcast_actions"
                : "hints.broadcast_actions_plural",
              { count: visibleList.length },
            )}
          </div>
        </BentoTile>
      </div>

      <SectionPanel
        title={t("pages.live_trackers")}
        action={
          <div className="flex items-center gap-3">
            {hiddenCount > 0 && (
              <button
                type="button"
                onClick={() => void unhideAll()}
                className="flex items-center gap-1 text-[11px] text-[var(--fg-muted)] hover:text-[var(--accent)]"
              >
                <Eye size={12} /> {t("actions.unhide_count", { count: hiddenCount })}
              </button>
            )}
            {rawList.length > 0 && (
              <button
                type="button"
                aria-pressed={viewPaused}
                onClick={() => setFrozenTrackers(viewPaused ? null : trackers)}
                className={`flex items-center gap-1.5 rounded-full border px-3 py-1 text-[11px] transition-colors ${
                  viewPaused
                    ? "border-[var(--warn)]/40 bg-[var(--warn-soft)] text-[var(--warn)]"
                    : "border-[var(--border-subtle)] text-[var(--fg-secondary)] hover:border-[var(--accent)] hover:text-[var(--accent)]"
                }`}
              >
                {viewPaused ? <Play size={11} /> : <Pause size={11} />}
                {viewPaused ? t("actions.resume_view") : t("actions.pause_view")}
              </button>
            )}
          </div>
        }
      >
        {visibleList.length === 0 ? (
          <EmptyState
            icon={Plugs}
            title={t("empty.no_trackers_title")}
            description={t("empty.no_trackers_desc")}
            cta={{
              label: t("empty.no_trackers_cta"),
              onClick: () => navigate("/devices"),
            }}
          />
        ) : (
          <div className="flex flex-col gap-6">
            {groupedList.map(({ group, items }) => (
              <GroupBlock
                key={group || "__ungrouped"}
                group={group}
                items={items}
                devices={devices}
                onReorder={async (macs) => {
                  // Persist new order as contiguous indices; bump by 10 to
                  // leave room for future intra-group inserts without
                  // touching every other row.
                  for (let i = 0; i < macs.length; i++) {
                    const mac = macs[i];
                    const order = (i + 1) * 10;
                    patch(mac, { display_order: order });
                    void api.setTrackerOrder(mac, order);
                  }
                }}
                onBroadcastGroup={(kind) => {
                  if (!group) return;
                  broadcastReset(kind, items);
                  pushToast({
                    level: "info",
                    message: t("toast.group_reset_sent", { group, count: items.length }),
                    ttlMs: 2000,
                  });
                }}
              />
            ))}
          </div>
        )}
      </SectionPanel>
    </div>
  );
}

function GroupBlock({
  group,
  items,
  devices,
  onReorder,
  onBroadcastGroup,
}: {
  group: string;
  items: TrackerSnapshot[];
  devices: Record<string, { native_imu_rate_hz?: number }>;
  onReorder: (macsInOrder: Mac[]) => void | Promise<void>;
  onBroadcastGroup: (kind: "yaw" | "full" | "mounting") => void;
}) {
  const { t } = useTranslation();
  const dragMac = useRef<string | null>(null);
  // local order overrides perDevSettings during drag so the UI updates
  // instantly without waiting for the IPC round-trip
  // oxlint-disable-next-line react-doctor/rerender-state-only-in-handlers -- localOrder IS read at line below via ??
  const [localOrder, setLocalOrder] = useState<string[] | null>(null);

  // Single-pass O(n) lookup table beats two .find() scans in the
  // map/filter chain — matters when groups grow to dozens of trackers.
  const itemsByKey = new Map(items.map((s) => [macKeyFn(s.mac), s]));
  const displayOrder = localOrder ?? items.map((s) => macKeyFn(s.mac));
  const ordered: TrackerSnapshot[] = [];
  for (const k of displayOrder) {
    const s = itemsByKey.get(k);
    if (s) ordered.push(s);
  }

  function onDrop(targetKey: string) {
    const src = dragMac.current;
    dragMac.current = null;
    if (!src || src === targetKey) return;
    const keys = displayOrder.slice();
    const srcIdx = keys.indexOf(src);
    const tgtIdx = keys.indexOf(targetKey);
    if (srcIdx < 0 || tgtIdx < 0) return;
    keys.splice(srcIdx, 1);
    keys.splice(tgtIdx, 0, src);
    setLocalOrder(keys);
    const macs: Mac[] = [];
    for (const k of keys) {
      const m = itemsByKey.get(k)?.mac;
      if (m) macs.push(m);
    }
    void onReorder(macs);
  }

  // Reset local override if the underlying group composition changes
  // (a new tracker discovered while hovering): drop the stale ordering and
  // let server-derived order take over. Tracking via ref avoids a re-render
  // round-trip and the "set state on prop change" effect smell.
  const itemKeysSig = items
    .map((s) => macKeyFn(s.mac))
    .sort()
    .join(",");
  const prevKeysRef = useRef(itemKeysSig);
  if (prevKeysRef.current !== itemKeysSig) {
    prevKeysRef.current = itemKeysSig;
    if (localOrder !== null) setLocalOrder(null);
  }

  return (
    <div>
      {group && (
        <div className="mb-2 flex items-center gap-2">
          <span className="text-[11px] font-semibold uppercase tracking-wide text-[var(--accent)]">
            {group}
          </span>
          <span className="text-[10px] text-[var(--fg-muted)]">·</span>
          <span className="text-[10px] text-[var(--fg-muted)]">
            {t("status.tracker_count_plural", { count: items.length })}
          </span>
          <div className="ml-auto flex gap-1">
            <GroupResetBtn
              title={t("actions.yaw_reset")}
              onClick={() => onBroadcastGroup("yaw")}
              icon={<Crosshair size={11} />}
            />
            <GroupResetBtn
              title={t("actions.full_reset")}
              onClick={() => onBroadcastGroup("full")}
              icon={<ArrowsClockwise size={11} />}
            />
          </div>
        </div>
      )}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
        {ordered.map((snap) => {
          const key = macKeyFn(snap.mac);
          const dev = devices[key];
          const targetHz = dev?.native_imu_rate_hz ?? 200;
          return (
            <Link
              key={key}
              to={`/devices/${key}`}
              draggable
              onDragStart={() => {
                dragMac.current = key;
              }}
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => onDrop(key)}
              className="block rounded-[var(--radius-md)] outline-none ring-0 transition-transform hover:-translate-y-px hover:ring-1 hover:ring-[var(--accent-soft)]"
            >
              <TrackerCard snap={snap} targetHz={targetHz} />
            </Link>
          );
        })}
      </div>
    </div>
  );
}

function GroupResetBtn({
  title,
  onClick,
  icon,
}: {
  title: string;
  onClick: () => void;
  icon: React.ReactNode;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      className="grid size-5 place-items-center rounded-[var(--radius-sm)] border border-[var(--border-subtle)] text-[var(--fg-muted)] hover:border-[var(--accent)] hover:text-[var(--accent)]"
    >
      {icon}
    </button>
  );
}

function SectionPanel({
  title,
  children,
  action,
}: {
  title: string;
  children: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <section>
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
          {title}
        </h2>
        {action}
      </div>
      {children}
    </section>
  );
}

/**
 * Bento tile primitive. `feature` boosts the visual weight (elevated
 * surface + solid accent stripe) — used for whatever metric we want
 * the eye to land on first. `accent` adds a thin tinted top stripe for
 * the second-most-important tile in a row. Flat surfaces only: depth
 * comes from background steps and the stripe, never shadows.
 */
function BentoTile({
  title,
  children,
  feature,
  accent,
  className,
}: {
  title: string;
  children: React.ReactNode;
  feature?: boolean;
  accent?: boolean;
  className?: string;
}) {
  return (
    <section
      className={`relative flex min-w-0 flex-col overflow-hidden rounded-[var(--radius-xl)] border p-5 transition-colors ${
        feature
          ? "border-[var(--border-strong)] bg-[var(--bg-elevated)]"
          : "border-[var(--border-subtle)] bg-[var(--bg-panel)] hover:border-[var(--border-strong)]"
      } ${className ?? ""}`}
    >
      {(accent || feature) && (
        <span
          aria-hidden
          className={`absolute inset-x-0 top-0 h-[2px] bg-[var(--accent)] ${feature ? "" : "opacity-60"}`}
        />
      )}
      <h2 className="mb-3 text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
        {title}
      </h2>
      <div className="relative min-w-0 flex-1">{children}</div>
    </section>
  );
}

function ResetButton({
  label,
  icon,
  onClick,
  disabled,
}: {
  label: string;
  icon: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-4 py-3 text-sm font-medium text-[var(--fg-secondary)] transition-colors hover:border-[var(--accent)] hover:bg-[var(--accent-soft)] hover:text-[var(--fg-primary)] disabled:cursor-not-allowed disabled:opacity-40"
    >
      <span className="text-[var(--accent)]">{icon}</span>
      {label}
    </button>
  );
}
