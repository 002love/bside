import { Terminal } from 'lucide-react';

export interface EmptyStateProps {
  title: string;
  description: string;
  command?: string;
}

export function EmptyState({
  title,
  description,
  command = 'bside watch',
}: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center rounded-[var(--radius-card)] border border-dashed border-[var(--border-strong)] bg-[var(--bg-surface)]/50 px-6 py-16 text-center">
      <div className="mb-3.5 flex h-10 w-10 items-center justify-center rounded-[10px] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--ink-secondary)]">
        <Terminal className="h-4 w-4" />
      </div>
      <h3 className="font-display text-base font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
        {title}
      </h3>
      <p className="mt-1.5 max-w-sm text-xs leading-relaxed text-[var(--ink-muted)]">
        {description}
      </p>
      {command && (
        <div className="mt-4 inline-flex items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-3 py-1.5 font-mono text-xs text-[var(--ink-secondary)]">
          <span className="text-[var(--ink-muted)]">$</span>
          <span>{command}</span>
        </div>
      )}
    </div>
  );
}
