import { useTranslation } from "react-i18next";

export function HelpPage() {
  const { t } = useTranslation();
  const keyboard: { keys: string; action: string }[] = [
    { keys: "Ctrl + K", action: t("help.sc_palette") },
    { keys: "R", action: t("help.sc_yaw") },
    { keys: "Shift + R", action: t("help.sc_full") },
    { keys: "Esc", action: t("help.sc_esc") },
  ];
  const faq: { q: string; a: string }[] = [
    { q: t("help.faq_close_q"), a: t("help.faq_close_a") },
    { q: t("help.faq_no_motion_q"), a: t("help.faq_no_motion_a") },
    { q: t("help.faq_mount_q"), a: t("help.faq_mount_a") },
    { q: t("help.faq_logs_q"), a: t("help.faq_logs_a") },
    { q: t("help.faq_scope_q"), a: t("help.faq_scope_a") },
  ];

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
        {t("help.title")}
      </h2>

      <section>
        <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-[var(--fg-section-header)]">
          {t("pages.keyboard_shortcuts")}
        </h3>
        <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-panel)]">
          <table className="w-full text-sm">
            <tbody>
              {keyboard.map((s) => (
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
          {t("pages.faq")}
        </h3>
        <div className="flex flex-col gap-3">
          {faq.map((item) => (
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
