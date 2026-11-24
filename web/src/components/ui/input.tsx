import type { InputHTMLAttributes } from 'react';
import { cn } from '@/lib/utils';

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        'flex h-8 w-full rounded-none border border-[#2b2b27] bg-[#141412] px-3 py-1 font-mono text-xs text-[#efeee8] placeholder:text-[#6f6e67] focus-visible:border-[#89b471] focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50',
        className,
      )}
      {...props}
    />
  );
}
