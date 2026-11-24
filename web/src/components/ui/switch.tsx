import type { ComponentPropsWithoutRef } from 'react';
import * as RadixSwitch from '@radix-ui/react-switch';
import { cn } from '@/lib/utils';

export function Switch({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof RadixSwitch.Root>) {
  return (
    <RadixSwitch.Root
      className={cn(
        'peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border border-zinc-700/80 bg-zinc-800 transition-colors data-[state=checked]:border-emerald-500/60 data-[state=checked]:bg-emerald-500 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500',
        className,
      )}
      {...props}
    >
      <RadixSwitch.Thumb className="pointer-events-none block h-3.5 w-3.5 translate-x-0.5 rounded-full bg-zinc-100 shadow-sm transition-transform data-[state=checked]:translate-x-[18px] data-[state=checked]:bg-zinc-950" />
    </RadixSwitch.Root>
  );
}
