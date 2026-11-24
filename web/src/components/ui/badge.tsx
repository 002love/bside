import type { HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeStyles = cva(
  'inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[11px] font-medium tracking-wide transition-colors',
  {
    variants: {
      variant: {
        default: 'border-emerald-500/30 bg-emerald-500/15 text-emerald-300',
        secondary: 'border-zinc-700/80 bg-zinc-800/80 text-zinc-300',
        outline: 'border-zinc-800 bg-transparent text-zinc-400',
        warning: 'border-amber-500/30 bg-amber-500/15 text-amber-300',
        destructive: 'border-rose-500/30 bg-rose-500/15 text-rose-300',
        info: 'border-sky-500/30 bg-sky-500/15 text-sky-300',
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
