import { Terminal } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';

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
    <Card className="border-dashed border-[var(--border-strong)] bg-[var(--bg-inset)]">
      <CardContent className="flex flex-col items-center justify-center py-14 text-center">
        <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-[var(--radius-control)] border border-[var(--border-strong)] bg-[var(--bg-panel)] text-[var(--accent-primary)] shadow-edge">
          <Terminal className="h-4 w-4" />
        </div>
        <h3 className="font-display text-xl font-bold tracking-tight text-[var(--ink-primary)]">
          {title}
        </h3>
        <p className="mt-1.5 max-w-md text-xs leading-relaxed text-[var(--ink-muted)]">
          {description}
        </p>
        {command && (
          <div className="mt-4 inline-flex items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-3 py-1.5 font-mono text-xs text-[var(--accent-primary)] shadow-moss">
            <span className="text-[var(--ink-muted)]">$</span>
            <span>{command}</span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
