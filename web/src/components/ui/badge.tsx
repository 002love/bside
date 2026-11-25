import type { HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeStyles = cva(
  'inline-flex items-center gap-1 rounded-[var(--radius-pill)] border px-2 py-0.5 font-mono text-[10px] font-medium uppercase tracking-wider transition-colors',
  {
    variants: {
      variant: {
        default:
          'border-[var(--border-accent)] bg-[var(--accent-soft)] text-[var(--accent-primary)]',
        secondary:
          'border-[var(--border-subtle)] bg-[var(--bg-inset)] text-[var(--ink-primary)]',
        outline:
          'border-[var(--border-subtle)] bg-transparent text-[var(--ink-muted)]',
        warning:
          'border-[var(--warning)]/50 bg-[var(--warning-soft)] text-[var(--warning)]',
        destructive:
          'border-[var(--danger)]/50 bg-[var(--danger-soft)] text-[var(--danger)]',
        info:
          'border-[var(--border-strong)] bg-[var(--bg-elevated)] text-[var(--ink-secondary)]',
      },
    },
    defaultVariants: {
      variant: 'secondary',
    },
  },
);

export interface BadgeProps
  extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeStyles> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeStyles({ variant }), className)} {...props} />;
}
