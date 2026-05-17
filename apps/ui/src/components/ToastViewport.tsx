import { CheckCircle, Info, Warning, WarningOctagon, X } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { type Toast, useToastStore } from "../stores/useToastStore";

const ICON: Record<Toast["level"], React.ComponentType<{ size?: number }>> = {
  info: Info,
  success: CheckCircle,
  warn: Warning,
  error: WarningOctagon,
};

const LEVEL_CLS: Record<Toast["level"], string> = {
  info: "border-[var(--info)]/40 text-[var(--info)]",
  success: "border-[var(--success)]/40 text-[var(--success)]",
  warn: "border-[var(--warn)]/40 text-[var(--warn)]",
  error: "border-[var(--danger)]/40 text-[var(--danger)]",
};

export function ToastViewport() {
  const { t } = useTranslation();
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  if (toasts.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-40 flex w-80 flex-col gap-2">
      {toasts.map((toast) => {
        const Icon = ICON[toast.level];
        return (
          <div
            key={toast.id}
            className={`pointer-events-auto flex items-start gap-3 rounded-[var(--radius-md)] border bg-[var(--bg-panel)] p-3 shadow-lg ${LEVEL_CLS[toast.level]}`}
          >
            <span className="pt-0.5">
              <Icon size={18} />
            </span>
            <div className="min-w-0 flex-1">
              {toast.title && (
                <div className="text-sm font-semibold text-[var(--fg-primary)]">{toast.title}</div>
              )}
              <div className="text-xs text-[var(--fg-secondary)]">{toast.message}</div>
              {toast.action && (
                <button
                  type="button"
                  onClick={() => {
                    void toast.action?.run();
                    dismiss(toast.id);
                  }}
                  className="mt-1.5 inline-flex items-center rounded-[var(--radius-sm)] border border-current px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide hover:bg-current/10"
                >
                  {toast.action.label}
                </button>
              )}
            </div>
            <button
              type="button"
              onClick={() => dismiss(toast.id)}
              aria-label={t("window.dismiss")}
              className="text-[var(--fg-muted)] hover:text-[var(--fg-primary)]"
            >
              <X size={14} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
