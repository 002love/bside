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
        'peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-[var(--radius-pill)] border border-[var(--border-strong)] bg-[var(--bg-inset)] transition-colors data-[state=checked]:border-[var(--accent-primary)] data-[state=checked]:bg-[var(--accent-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]/70',
        className,
      )}
      {...props}
    >
      <RadixSwitch.Thumb className="pointer-events-none block h-3.5 w-3.5 translate-x-0.5 rounded-[var(--radius-pill)] bg-[var(--ink-muted)] transition-transform data-[state=checked]:translate-x-[17px] data-[state=checked]:bg-[var(--ink-on-accent)]" />
    </RadixSwitch.Root>
  );
}
