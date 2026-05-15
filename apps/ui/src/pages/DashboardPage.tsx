import { ArrowsClockwise, Crosshair, Target } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import { ConnectionStatusCard } from "../components/ConnectionStatusCard";
import { TrackerCard } from "../components/TrackerCard";
import { macKey as macKeyFn } from "../lib/macFormat";
import { useDeviceStore } from "../stores/useDeviceStore";
import { useTrackerStore } from "../stores/useTrackerStore";

export function DashboardPage() {
  const { t } = useTranslation();
  const trackers = useTrackerStore((s) => s.trackers);
  const devices = useDeviceStore((s) => s.devices);
  const list = Object.values(trackers);

  function broadcastReset(kind: "yaw" | "full" | "mounting") {
    for (const snap of list) {
      void api.requestReset(snap.mac, kind);
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <SectionPanel title={t("pages.connection")}>
        <ConnectionStatusCard />
      </SectionPanel>

      <SectionPanel title={t("pages.broadcast_actions")}>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <ResetButton
            label={t("actions.yaw_reset")}
            icon={<Crosshair size={20} weight="duotone" />}
            onClick={() => broadcastReset("yaw")}
            disabled={list.length === 0}
          />
          <ResetButton
            label={t("actions.full_reset")}
            icon={<ArrowsClockwise size={20} weight="duotone" />}
            onClick={() => broadcastReset("full")}
            disabled={list.length === 0}
          />
          <ResetButton
            label={t("actions.mounting_short")}
            icon={<Target size={20} weight="duotone" />}
            onClick={() => broadcastReset("mounting")}
            disabled={list.length === 0}
          />
        </div>
        <div className="mt-3 text-xs text-[var(--fg-muted)]">
          {t(list.length === 1 ? "hints.broadcast_actions" : "hints.broadcast_actions_plural", {
            count: list.length,
          })}
        </div>
      </SectionPanel>

      <SectionPanel title={t("pages.live_trackers")}>
        {list.length === 0 ? (
          <Empty>{t("hints.no_trackers")}</Empty>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
            {list.map((snap) => {
              const key = macKeyFn(snap.mac);
              const dev = devices[key];
              const targetHz = dev?.native_imu_rate_hz ?? 200;
              return (
                <Link
                  key={key}
                  to={`/devices/${key}`}
                  className="block rounded-[var(--radius-md)] outline-none ring-0 transition-transform hover:-translate-y-px hover:ring-1 hover:ring-[var(--accent-soft)]"
                >
                  <TrackerCard snap={snap} targetHz={targetHz} />
                </Link>
              );
            })}
          </div>
        )}
      </SectionPanel>
    </div>
  );
}

function SectionPanel({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
        {title}
      </h2>
      {children}
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
      className="flex items-center justify-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-4 py-4 text-sm font-medium text-[var(--fg-primary)] transition-colors hover:bg-[var(--warn-soft)] hover:text-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-40"
    >
      <span className="text-[var(--accent)]">{icon}</span>
      {label}
    </button>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-[var(--radius-md)] border border-dashed border-[var(--border-subtle)] p-8 text-center text-sm text-[var(--fg-muted)]">
      {children}
    </div>
  );
}
