import type { ButtonHTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const buttonStyles = cva(
  'inline-flex items-center justify-center gap-1.5 rounded-[var(--radius-control)] font-mono text-[11px] font-semibold uppercase tracking-wider transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]/70 disabled:pointer-events-none disabled:opacity-45 cursor-pointer select-none',
  {
    variants: {
      variant: {
        primary:
          'border border-[var(--accent-primary)] bg-[var(--accent-primary)] text-[var(--ink-on-accent)] bside-btn-layered hover:bg-[var(--accent-hover)] hover:border-[var(--accent-hover)]',
        secondary:
          'border border-[var(--border-subtle)] bg-[var(--bg-panel)] text-[var(--ink-primary)] shadow-edge hover:border-[var(--border-strong)] hover:bg-[var(--bg-hover)] active:translate-y-[1px]',
        ghost:
          'text-[var(--ink-muted)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
        outline:
          'border border-[var(--border-strong)] bg-transparent text-[var(--ink-primary)] hover:border-[var(--accent-primary)] hover:bg-[var(--bg-hover)]',
      },
      size: {
        sm: 'h-7 px-2.5 text-[11px]',
        md: 'h-8 px-3 text-[11px]',
        icon: 'h-8 w-8',
      },
    },
    defaultVariants: {
      variant: 'secondary',
      size: 'md',
    },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonStyles> {}

export function Button({ className, variant, size, ...props }: ButtonProps) {
  return <button className={cn(buttonStyles({ variant, size }), className)} {...props} />;
}
