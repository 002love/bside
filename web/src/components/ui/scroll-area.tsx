import type { ComponentPropsWithoutRef } from 'react';
import * as RadixScrollArea from '@radix-ui/react-scroll-area';
import { cn } from '@/lib/utils';

export function ScrollArea({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof RadixScrollArea.Root>) {
  return (
    <RadixScrollArea.Root className={cn('relative overflow-hidden rounded-none', className)} {...props}>
      <RadixScrollArea.Viewport className="h-full w-full rounded-none">
        {children}
      </RadixScrollArea.Viewport>
      <RadixScrollArea.Scrollbar
        orientation="vertical"
        className="flex w-2 touch-none select-none p-0.5 transition-colors"
      >
        <RadixScrollArea.Thumb className="relative flex-1 rounded-none bg-[#2b2b27] hover:bg-[#45453e]" />
      </RadixScrollArea.Scrollbar>
      <RadixScrollArea.Corner />
    </RadixScrollArea.Root>
  );
}
