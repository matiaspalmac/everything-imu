import { Broadcast, FilmStrip, Keyboard, MagnifyingGlass } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";

type Shortcut = {
  keys: string[];
  action: string;
  icon?: React.ReactNode;
};

export function HelpPage() {
  const { t } = useTranslation();

  const shortcuts: Shortcut[] = [
    { keys: ["Ctrl", "K"], action: t("help.sc_palette"), icon: <Keyboard size={14} /> },
    { keys: ["Ctrl", "F"], action: t("help.sc_search"), icon: <MagnifyingGlass size={14} /> },
    {
      keys: ["Ctrl", "Shift", "B"],
      action: t("help.sc_bridge"),
      icon: <Broadcast size={14} />,
    },
    {
      keys: ["Ctrl", "Enter"],
      action: t("help.sc_cinema"),
      icon: <FilmStrip size={14} />,
    },
    { keys: ["R"], action: t("help.sc_yaw") },
    { keys: ["Shift", "R"], action: t("help.sc_full") },
    { keys: ["Esc"], action: t("help.sc_esc") },
  ];

  const faq: { q: string; a: string }[] = [
    { q: t("help.faq_close_q"), a: t("help.faq_close_a") },
    { q: t("help.faq_no_motion_q"), a: t("help.faq_no_motion_a") },
    { q: t("help.faq_mount_q"), a: t("help.faq_mount_a") },
    { q: t("help.faq_logs_q"), a: t("help.faq_logs_a") },
    { q: t("help.faq_scope_q"), a: t("help.faq_scope_a") },
  ];

  return (
    <div className="flex flex-col gap-5">
      <header className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("help.title")}
        </h2>
      </header>

      <section className="rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4">
        <h3 className="mb-4 text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("pages.keyboard_shortcuts")}
        </h3>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {shortcuts.map((sc) => (
            <ShortcutCard key={sc.keys.join("+")} {...sc} />
          ))}
        </div>
      </section>

      <section>
        <h3 className="mb-3 text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("pages.faq")}
        </h3>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {faq.map((item) => (
            <article
              key={item.q}
              className="flex flex-col gap-2 rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-4 transition-colors hover:border-[var(--border-strong)]"
            >
              <div className="text-sm font-semibold text-[var(--fg-primary)]">{item.q}</div>
              <p className="text-xs leading-relaxed text-[var(--fg-secondary)]">{item.a}</p>
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}

function ShortcutCard({ keys, action, icon }: Shortcut) {
  return (
    <div className="flex items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] p-3 transition-colors hover:border-[var(--accent-soft)]">
      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-[var(--radius-sm)] bg-[var(--bg-panel)] text-[var(--accent)]">
        {icon ?? <Keyboard size={14} />}
      </div>
      <div className="min-w-0 flex-1">
        <div className="mb-1 flex flex-wrap items-center gap-1">
          {keys.map((k, i) => (
            <span key={`${keys.slice(0, i + 1).join("+")}`} className="contents">
              <KeyCap label={k} />
              {i < keys.length - 1 && <span className="text-[10px] text-[var(--fg-muted)]">+</span>}
            </span>
          ))}
        </div>
        <div className="truncate text-[11px] text-[var(--fg-secondary)]">{action}</div>
      </div>
    </div>
  );
}

function KeyCap({ label }: { label: string }) {
  return (
    <kbd className="metric-num inline-grid h-6 min-w-[1.5rem] place-items-center rounded-[var(--radius-sm)] border border-[var(--border-strong)] bg-[var(--bg-panel)] px-1.5 font-mono text-[11px] font-semibold text-[var(--fg-primary)] shadow-[inset_0_-1px_0_var(--border-strong)]">
      {label}
    </kbd>
  );
}
