import type { Icon as PhosphorIcon } from "@phosphor-icons/react";

/**
 * Reusable empty-state primitive. Centered icon disc + heading + body
 * + optional CTA. Use anywhere a list / panel resolves to "nothing
 * here yet" so the surface still feels intentional instead of an
 * accidental blank rectangle.
 *
 * Density: `compact` shrinks padding for inline placements (inside a
 * card body), default has the breathing room for full-page empties.
 */
export function EmptyState({
  icon: Icon,
  title,
  description,
  cta,
  compact,
}: {
  icon: PhosphorIcon;
  title: string;
  description?: string;
  cta?: {
    label: string;
    onClick: () => void;
    tone?: "accent" | "neutral";
  };
  compact?: boolean;
}) {
  const pad = compact ? "p-6" : "p-10";
  return (
    <div
      className={`flex flex-col items-center gap-3 rounded-[var(--radius-md)] border border-dashed border-[var(--border-subtle)] text-center ${pad}`}
    >
      <div className="relative grid h-14 w-14 place-items-center rounded-full bg-[var(--bg-elevated)] text-[var(--accent)]">
        {/* faint accent halo so the disc reads as a soft focal point */}
        <span
          aria-hidden
          className="absolute inset-0 -z-10 rounded-full bg-[var(--accent-glow)] blur-xl"
        />
        <Icon size={24} weight="duotone" />
      </div>
      <div className="flex flex-col gap-1">
        <h4 className="text-sm font-semibold text-[var(--fg-primary)]">{title}</h4>
        {description && (
          <p className="max-w-sm text-[11px] leading-relaxed text-[var(--fg-muted)]">
            {description}
          </p>
        )}
      </div>
      {cta && (
        <button
          type="button"
          onClick={cta.onClick}
          className={
            cta.tone === "neutral"
              ? "mt-1 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-1.5 text-xs text-[var(--fg-secondary)] hover:border-[var(--border-strong)] hover:text-[var(--fg-primary)]"
              : "mt-1 rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 py-1.5 text-xs font-semibold text-[var(--fg-inverse)] transition-colors hover:bg-[var(--accent-bright)]"
          }
        >
          {cta.label}
        </button>
      )}
    </div>
  );
}
