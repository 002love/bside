import type { HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeStyles = cva(
  'inline-flex items-center gap-1 rounded-none border px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-wider transition-colors',
  {
    variants: {
      variant: {
        default: 'border-[#557445] bg-[#557445]/20 text-[#89b471]',
        secondary: 'border-[#2b2b27] bg-[#141412] text-[#efeee8]',
        outline: 'border-[#2b2b27] bg-transparent text-[#8d8c84]',
        warning: 'border-[#c48a47]/60 bg-[#c48a47]/15 text-[#c48a47]',
        destructive: 'border-[#b85d4f]/60 bg-[#b85d4f]/15 text-[#d47a6c]',
        info: 'border-[#45453e] bg-[#20201d] text-[#c7c6bc]',
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
