import {
  GearSix,
  House,
  ListBullets,
  type Icon as PhosphorIcon,
  Plugs,
  Pulse,
} from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { useTrackerStore } from "../stores/useTrackerStore";
import { CommandPalette } from "./CommandPalette";
import { EventBridge } from "./EventBridge";
import { KeyboardShortcuts } from "./KeyboardShortcuts";
import { StatusBar } from "./StatusBar";
import { TitleBar } from "./TitleBar";
import { ToastViewport } from "./ToastViewport";

type Item = { to: string; labelKey: string; icon: PhosphorIcon };

const PRIMARY: Item[] = [
  { to: "/", labelKey: "nav.dashboard", icon: House },
  { to: "/connection", labelKey: "nav.connection", icon: Pulse },
  { to: "/devices", labelKey: "nav.devices", icon: Plugs },
  { to: "/logs", labelKey: "nav.logs", icon: ListBullets },
];

export function AppShell() {
  const trackerCount = Object.keys(useTrackerStore((s) => s.trackers)).length;
  const location = useLocation();
  const navigate = useNavigate();
  const { t } = useTranslation();

  const isActive = (to: string): boolean =>
    to === "/" ? location.pathname === "/" : location.pathname.startsWith(to);

  return (
    <div className="flex h-screen w-screen flex-col bg-[var(--bg-base)] text-[var(--fg-primary)]">
      <EventBridge />
      <KeyboardShortcuts />
      <CommandPalette />
      <ToastViewport />
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <aside
          aria-label="Primary navigation"
          className="flex w-[var(--activitybar-w)] shrink-0 flex-col items-center gap-1 border-r border-[var(--border-subtle)] bg-[var(--bg-panel)] py-2"
        >
          {PRIMARY.map((it) => (
            <NavButton
              key={it.to}
              label={t(it.labelKey)}
              icon={<it.icon size={18} weight={isActive(it.to) ? "fill" : "regular"} />}
              active={isActive(it.to)}
              onClick={() => navigate(it.to)}
            />
          ))}
          <div className="flex-1" />
          <NavButton
            label={t("nav.settings")}
            icon={<GearSix size={18} weight={isActive("/settings") ? "fill" : "regular"} />}
            active={isActive("/settings")}
            onClick={() => navigate("/settings")}
          />
          <div className="px-1 pt-1 text-center text-[10px] text-[var(--fg-muted)]">
            {trackerCount}
          </div>
        </aside>
        <div className="flex min-w-0 flex-1 flex-col">
          <main className="min-w-0 flex-1 overflow-auto p-6">
            <Outlet />
          </main>
          <StatusBar />
        </div>
      </div>
    </div>
  );
}

function NavButton({
  label,
  icon,
  active,
  onClick,
}: {
  label: string;
  icon: React.ReactNode;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
      className={[
        "relative grid h-10 w-10 place-items-center rounded-[var(--radius-sm)] transition-colors",
        "after:absolute after:right-0 after:top-1.5 after:bottom-1.5 after:w-[2px] after:bg-transparent",
        active
          ? "bg-[var(--warn-soft)] text-[var(--accent)] after:bg-[var(--accent)]"
          : "text-[var(--fg-muted)] hover:bg-[var(--warn-soft)] hover:text-[var(--fg-primary)]",
      ].join(" ")}
    >
      {icon}
    </button>
  );
}
