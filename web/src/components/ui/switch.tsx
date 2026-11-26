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
        'peer inline-flex h-[18px] w-8 shrink-0 cursor-pointer items-center rounded-full border border-[var(--border-strong)] bg-[var(--bg-inset)] transition-colors data-[state=checked]:border-[var(--accent-ember)] data-[state=checked]:bg-[var(--accent-ember)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--border-focus)]',
        className,
      )}
      {...props}
    >
      <RadixSwitch.Thumb className="pointer-events-none block h-3 w-3 translate-x-[2px] rounded-full bg-[var(--ink-secondary)] transition-transform duration-150 data-[state=checked]:translate-x-[15px] data-[state=checked]:bg-[#09090b]" />
    </RadixSwitch.Root>
  );
}
