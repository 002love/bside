import type { ButtonHTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const buttonStyles = cva(
  'inline-flex items-center justify-center gap-1.5 rounded-none font-mono text-[11px] font-semibold uppercase tracking-wider transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[#89b471] disabled:pointer-events-none disabled:opacity-45 cursor-pointer',
  {
    variants: {
      variant: {
        primary:
          'border border-[#efeee8] bg-[#efeee8] text-[#111110] shadow-moss-interactive hover:bg-[#ffffff]',
        secondary:
          'border border-[#2b2b27] bg-[#181816] text-[#efeee8] hover:border-[#45453e] hover:bg-[#20201d]',
        ghost: 'text-[#8d8c84] hover:bg-[#20201d] hover:text-[#efeee8]',
        outline:
          'border border-[#45453e] bg-transparent text-[#efeee8] hover:border-[#89b471] hover:bg-[#20201d]',
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
