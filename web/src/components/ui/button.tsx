import type { ButtonHTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const buttonStyles = cva(
  'inline-flex items-center justify-center gap-1.5 rounded-lg text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500 disabled:pointer-events-none disabled:opacity-45 cursor-pointer',
  {
    variants: {
      variant: {
        primary: 'bg-emerald-500 text-zinc-950 hover:bg-emerald-400',
        secondary: 'border border-zinc-800 bg-zinc-900 text-zinc-200 hover:bg-zinc-800 hover:text-zinc-50',
        ghost: 'text-zinc-400 hover:bg-zinc-800/70 hover:text-zinc-100',
        outline: 'border border-zinc-800 bg-transparent text-zinc-300 hover:bg-zinc-900 hover:text-zinc-100',
      },
      size: {
        sm: 'h-7 px-2.5 text-xs',
        md: 'h-8 px-3 text-xs',
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
