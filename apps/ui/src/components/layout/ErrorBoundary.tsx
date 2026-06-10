import { ArrowsClockwise, WarningOctagon } from "@phosphor-icons/react";
import { Component, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

/**
 * Last-resort catch for render-time exceptions. Without it a single
 * component crash blanks the whole window; with it the user gets a
 * readable fallback and a reload button while the error lands in the
 * console (surfaced in the backend log during dev).
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack?: string | null }) {
    console.error("[ui] render crash:", error, info.componentStack ?? "");
  }

  render() {
    if (this.state.error) {
      return <CrashFallback error={this.state.error} />;
    }
    return this.props.children;
  }
}

function CrashFallback({ error }: { error: Error }) {
  const { t } = useTranslation();
  return (
    <div className="grid h-screen w-screen place-items-center bg-[var(--bg-base)] p-6 text-[var(--fg-primary)]">
      <div className="flex w-full max-w-md flex-col items-center gap-4 rounded-[var(--radius-xl)] border border-[var(--border-strong)] bg-[var(--bg-panel)] p-8 text-center">
        <WarningOctagon size={40} className="text-[var(--danger)]" />
        <h1 className="text-lg font-semibold">{t("errors.boundary_title")}</h1>
        <p className="text-sm text-[var(--fg-secondary)]">{t("errors.boundary_body")}</p>
        <code className="max-h-24 w-full overflow-auto rounded-[var(--radius-md)] bg-[var(--bg-base)] p-3 text-left font-mono text-[11px] text-[var(--fg-muted)]">
          {error.message}
        </code>
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="flex items-center gap-2 rounded-[var(--radius-md)] bg-[var(--accent)] px-4 py-2 text-sm font-semibold text-[var(--fg-inverse)] transition-colors hover:bg-[var(--accent-bright)]"
        >
          <ArrowsClockwise size={16} />
          {t("errors.boundary_reload")}
        </button>
      </div>
    </div>
  );
}
