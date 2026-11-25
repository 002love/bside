import type { ComponentPropsWithoutRef } from 'react';
import * as RadixScrollArea from '@radix-ui/react-scroll-area';
import { cn } from '@/lib/utils';

export function ScrollArea({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof RadixScrollArea.Root>) {
  return (
    <RadixScrollArea.Root
      className={cn('relative overflow-hidden rounded-[var(--radius-control)]', className)}
      {...props}
    >
      <RadixScrollArea.Viewport className="h-full w-full rounded-[inherit] [&>div]:!block">
        {children}
      </RadixScrollArea.Viewport>
      <RadixScrollArea.Scrollbar
        orientation="vertical"
        className="flex w-2 touch-none select-none p-0.5 transition-colors"
      >
        <RadixScrollArea.Thumb className="relative flex-1 rounded-[var(--radius-pill)] bg-[var(--border-strong)] hover:bg-[var(--ink-muted)]" />
      </RadixScrollArea.Scrollbar>
      <RadixScrollArea.Corner />
    </RadixScrollArea.Root>
  );
}
