import {
  ArrowsClockwise,
  Crosshair,
  EyeSlash,
  Funnel,
  MagnifyingGlass,
  Plugs,
} from "@phosphor-icons/react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import type { DeviceMetadataDto } from "../api/client";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { SteamBlacklistBanner } from "../components/SteamBlacklistBanner";
import { macHex, macKey as macKeyFn } from "../lib/macFormat";
import { useDeviceStore } from "../stores/useDeviceStore";
import { usePerDeviceSettingsStore } from "../stores/usePerDeviceSettingsStore";
import { useToastStore } from "../stores/useToastStore";
import { useTrackerStore } from "../stores/useTrackerStore";

type SortKey = "kind" | "mac" | "serial" | "rate";

export function DevicesPage() {
  const { t } = useTranslation();
  const devices = useDeviceStore((s) => s.devices);
  const trackers = useTrackerStore((s) => s.trackers);
  const perDev = usePerDeviceSettingsStore((s) => s.perMac);
  const pushToast = useToastStore((s) => s.push);
  const navigate = useNavigate();

  const [q, setQ] = useState("");
  const [kindFilter, setKindFilter] = useState<string>("");
  const [sort, setSort] = useState<SortKey>("kind");
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const list = useMemo(() => Object.values(devices), [devices]);

  const kinds = useMemo(() => {
    const seen = new Set<string>();
    for (const d of list) seen.add(d.kind);
    return Array.from(seen).sort();
  }, [list]);

  const filtered = useMemo(() => {
    const needle = q.toLowerCase();
    const out = list.filter((d) => {
      if (kindFilter && d.kind !== kindFilter) return false;
      if (!needle) return true;
      const k = macKeyFn(d.mac);
      const label = perDev[k]?.label ?? "";
      return `${label} ${d.kind} ${d.serial} ${macHex(d.mac)} ${d.firmware ?? ""}`
        .toLowerCase()
        .includes(needle);
    });
    out.sort((a, b) => {
      if (sort === "mac") return macHex(a.mac).localeCompare(macHex(b.mac));
      if (sort === "serial") return a.serial.localeCompare(b.serial);
      if (sort === "rate") {
        const ra = trackers[macKeyFn(a.mac)]?.rate_hz ?? 0;
        const rb = trackers[macKeyFn(b.mac)]?.rate_hz ?? 0;
        return rb - ra;
      }
      return a.kind.localeCompare(b.kind);
    });
    return out;
  }, [list, q, kindFilter, sort, trackers, perDev]);

  const allSelected = filtered.length > 0 && filtered.every((d) => selected.has(macKeyFn(d.mac)));
  function toggleAll() {
    if (allSelected) {
      setSelected(new Set());
    } else {
      setSelected(new Set(filtered.map((d) => macKeyFn(d.mac))));
    }
  }
  function toggle(macKey: string) {
    const next = new Set(selected);
    if (next.has(macKey)) next.delete(macKey);
    else next.add(macKey);
    setSelected(next);
  }

  async function bulkReset(kind: "yaw" | "full") {
    const targets = filtered.filter((d) => selected.has(macKeyFn(d.mac)));
    await Promise.all(targets.map((d) => api.requestReset(d.mac, kind)));
    pushToast({
      level: "info",
      message:
        kind === "yaw"
          ? t("shortcuts.broadcast_yaw_done_plural", { count: targets.length })
          : t("shortcuts.broadcast_full_done_plural", { count: targets.length }),
      ttlMs: 2000,
    });
  }

  async function bulkHide() {
    const macs = filtered.filter((d) => selected.has(macKeyFn(d.mac)));
    await Promise.all(macs.map((d) => api.setTrackerHidden(d.mac, true)));
    pushToast({
      level: "info",
      message: t("toast.tracker_hidden", { mac: `${macs.length}` }),
      ttlMs: 2000,
    });
    setSelected(new Set());
  }

  return (
    <div className="flex flex-col gap-4">
      <SteamBlacklistBanner />
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <h2 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
            {t("pages.devices")}
          </h2>
          <span className="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-[10px] text-[var(--fg-muted)]">
            {t("status.known", { count: list.length })} ·{" "}
            {t("status.shown", { count: filtered.length })}
          </span>
        </div>
        {selected.size > 0 && (
          <div className="flex items-center gap-2 text-[11px] text-[var(--fg-secondary)]">
            <span>{selected.size} selected</span>
            <BulkBtn
              onClick={() => void bulkReset("yaw")}
              icon={<Crosshair size={12} />}
              label={t("actions.reset_yaw")}
            />
            <BulkBtn
              onClick={() => void bulkReset("full")}
              icon={<ArrowsClockwise size={12} />}
              label={t("actions.reset_full")}
            />
            <BulkBtn
              onClick={() => void bulkHide()}
              icon={<EyeSlash size={12} />}
              label={t("actions.hide")}
              tone="danger"
            />
          </div>
        )}
      </header>

      <div className="flex flex-wrap items-center gap-2 rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-2">
        <label className="flex items-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1.5 text-[11px] text-[var(--fg-muted)]">
          <input
            type="checkbox"
            aria-label={allSelected ? t("devices.deselect_all") : t("devices.select_all")}
            checked={allSelected}
            onChange={toggleAll}
            className="size-3.5 accent-[var(--accent)]"
          />
          {allSelected ? t("devices.deselect_all") : t("devices.select_all")}
        </label>
        <div className="flex flex-1 items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3">
          <MagnifyingGlass size={13} className="text-[var(--fg-muted)]" />
          <input
            type="text"
            aria-label={t("devices.search_placeholder")}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={t("devices.search_placeholder")}
            className="flex-1 bg-transparent py-1.5 text-sm text-[var(--fg-primary)] placeholder:text-[var(--fg-muted)] focus:outline-none"
          />
        </div>
        <select
          value={kindFilter}
          onChange={(e) => setKindFilter(e.target.value)}
          className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1.5 text-xs text-[var(--fg-secondary)]"
        >
          <option value="">{t("devices.all_kinds")}</option>
          {kinds.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </select>
        <select
          value={sort}
          onChange={(e) => setSort(e.target.value as SortKey)}
          className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1.5 text-xs text-[var(--fg-secondary)]"
        >
          <option value="kind">{t("devices.sort_kind")}</option>
          <option value="mac">{t("devices.sort_mac")}</option>
          <option value="serial">{t("devices.sort_serial")}</option>
          <option value="rate">{t("devices.sort_rate")}</option>
        </select>
      </div>

      {filtered.length === 0 ? (
        list.length === 0 ? (
          <EmptyState
            icon={Plugs}
            title={t("empty.no_devices_title")}
            description={t("empty.no_devices_desc")}
          />
        ) : (
          <EmptyState
            icon={Funnel}
            title={t("empty.no_match_title")}
            description={t("empty.no_match_desc")}
            cta={{
              label: t("empty.clear_filters"),
              tone: "neutral",
              onClick: () => {
                setQ("");
                setKindFilter("");
              },
            }}
          />
        )
      ) : (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map((d) => {
            const key = macKeyFn(d.mac);
            const snap = trackers[key];
            const label = perDev[key]?.label ?? "";
            const isSelected = selected.has(key);
            return (
              <DeviceCard
                key={key}
                d={d}
                label={label}
                rateHz={snap?.rate_hz ?? 0}
                selected={isSelected}
                onToggle={() => toggle(key)}
                onOpen={() => navigate(`/devices/${key}`)}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

function DeviceCard({
  d,
  label,
  rateHz,
  selected,
  onToggle,
  onOpen,
}: {
  d: DeviceMetadataDto;
  label: string;
  rateHz: number;
  selected: boolean;
  onToggle: () => void;
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  const caps: string[] = [];
  if (d.has_magnetometer) caps.push("mag");
  if (d.has_battery) caps.push("battery");
  if (d.has_rumble) caps.push("rumble");
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") onOpen();
      }}
      className={`group/dev relative flex cursor-pointer flex-col gap-2 rounded-[var(--radius-md)] border bg-[var(--bg-panel)] p-3 transition ${
        selected
          ? "border-[var(--accent)] shadow-[var(--shadow-card)]"
          : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]"
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <input
          type="checkbox"
          aria-label={t("devices.select")}
          checked={selected}
          onClick={(e) => e.stopPropagation()}
          onChange={onToggle}
          className="size-3.5 accent-[var(--accent)]"
        />
        <span className="ml-auto rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-[10px] uppercase tracking-wide text-[var(--accent)]">
          {d.kind}
        </span>
      </div>
      <div className="flex flex-col gap-0.5">
        <span className="truncate text-sm font-semibold text-[var(--fg-primary)]">
          {label || <span className="metric-num font-mono">{macHex(d.mac)}</span>}
        </span>
        <span className="metric-num truncate font-mono text-[10px] text-[var(--fg-muted)]">
          {label ? macHex(d.mac) : d.serial}
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-1.5 text-[10px] text-[var(--fg-secondary)]">
        {caps.map((c) => (
          <span
            key={c}
            className="rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-1.5 py-0.5 uppercase tracking-wide"
          >
            {c}
          </span>
        ))}
        {d.firmware && (
          <span className="metric-num rounded-[var(--radius-sm)] bg-[var(--bg-elevated)] px-1.5 py-0.5 font-mono">
            {d.firmware}
          </span>
        )}
      </div>
      <div className="flex items-center justify-between gap-2 pt-1">
        <span className="metric-num text-[11px] text-[var(--fg-muted)]">
          {rateHz > 0 ? `${Math.round(rateHz)} Hz` : t("status.idle")}
        </span>
        <div className="flex gap-1 opacity-0 transition-opacity group-hover/dev:opacity-100">
          <InlineBtn
            title={t("actions.reset_yaw")}
            onClick={(e) => {
              e.stopPropagation();
              void api.requestReset(d.mac, "yaw");
            }}
            icon={<Crosshair size={11} />}
          />
          <InlineBtn
            title={t("actions.reset_full")}
            onClick={(e) => {
              e.stopPropagation();
              void api.requestReset(d.mac, "full");
            }}
            icon={<ArrowsClockwise size={11} />}
          />
        </div>
      </div>
    </div>
  );
}

function InlineBtn({
  title,
  onClick,
  icon,
}: {
  title: string;
  onClick: (e: React.MouseEvent) => void;
  icon: React.ReactNode;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      className="grid size-6 place-items-center rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] text-[var(--fg-muted)] hover:border-[var(--accent)] hover:text-[var(--accent)]"
    >
      {icon}
    </button>
  );
}

function BulkBtn({
  onClick,
  icon,
  label,
  tone,
}: {
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  tone?: "danger";
}) {
  const cls =
    tone === "danger"
      ? "border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--danger-soft)]"
      : "border-[var(--border-subtle)] text-[var(--fg-secondary)] hover:border-[var(--accent)] hover:text-[var(--accent)]";
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1 rounded-[var(--radius-sm)] border bg-[var(--bg-panel)] px-2 py-1 text-[11px] ${cls}`}
    >
      {icon} {label}
    </button>
  );
}
