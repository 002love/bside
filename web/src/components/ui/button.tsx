import type { ButtonHTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const buttonStyles = cva(
  'tactile-press inline-flex items-center justify-center gap-1.5 rounded-[var(--radius-control)] text-xs font-medium tracking-[-0.01em] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--border-focus)] disabled:pointer-events-none disabled:opacity-45 cursor-pointer select-none',
  {
    variants: {
      variant: {
        primary:
          'border border-transparent bg-[var(--accent-cta)] text-[var(--ink-on-accent)] hover:bg-[var(--accent-cta-hover)]',
        accent:
          'border border-[var(--border-accent)] bg-[var(--accent-ember-soft)] text-[var(--accent-ember)] hover:bg-[var(--accent-ember)]/20',
        secondary:
          'border border-[var(--border-subtle)] bg-[var(--bg-surface)] text-[var(--ink-primary)] hover:border-[var(--border-strong)] hover:bg-[var(--bg-elevated)]',
        ghost:
          'text-[var(--ink-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
        outline:
          'border border-[var(--border-strong)] bg-transparent text-[var(--ink-primary)] hover:border-[var(--border-focus)] hover:bg-[var(--bg-hover)]',
      },
      size: {
        sm: 'h-7 px-2.5 text-[11.5px]',
        md: 'h-8 px-3 text-xs',
        lg: 'h-9 px-4 text-[13px]',
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
