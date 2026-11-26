import type { HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeStyles = cva(
  'inline-flex items-center gap-1 rounded-[6px] border px-1.5 py-0.5 text-[11px] font-medium leading-none transition-colors',
  {
    variants: {
      variant: {
        default:
          'border-[var(--border-accent)] bg-[var(--accent-ember-soft)] text-[var(--accent-ember)]',
        secondary:
          'border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--ink-secondary)]',
        outline:
          'border-[var(--border-subtle)] bg-transparent text-[var(--ink-muted)]',
        warning:
          'border-[var(--warning)]/30 bg-[var(--warning-soft)] text-[var(--warning)]',
        destructive:
          'border-[var(--danger)]/30 bg-[var(--danger-soft)] text-[var(--danger)]',
        info:
          'border-[var(--border-subtle)] bg-white/[0.05] text-[var(--ink-secondary)]',
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
