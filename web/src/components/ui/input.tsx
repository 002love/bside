import type { InputHTMLAttributes } from 'react';
import { cn } from '@/lib/utils';

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        'flex h-8 w-full rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-3 py-1 text-xs text-[var(--ink-primary)] placeholder:text-[var(--ink-muted)] transition-colors hover:border-[var(--border-strong)] focus-visible:border-[var(--border-focus)] focus-visible:bg-[var(--bg-surface)] focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50',
        className,
      )}
      {...props}
    />
  );
}
