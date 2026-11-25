import type { ComponentPropsWithoutRef } from 'react';
import * as RadixTooltip from '@radix-ui/react-tooltip';
import { cn } from '@/lib/utils';

export const TooltipProvider = RadixTooltip.Provider;
export const Tooltip = RadixTooltip.Root;
export const TooltipTrigger = RadixTooltip.Trigger;

export function TooltipContent({
  className,
  sideOffset = 6,
  collisionPadding = 12,
  ...props
}: ComponentPropsWithoutRef<typeof RadixTooltip.Content>) {
  return (
    <RadixTooltip.Portal>
      <RadixTooltip.Content
        sideOffset={sideOffset}
        collisionPadding={collisionPadding}
        className={cn(
          'z-50 max-w-[min(20rem,calc(100vw-1.5rem))] break-words bside-popover px-3 py-2 text-xs leading-relaxed text-[var(--ink-primary)]',
          className,
        )}
        {...props}
      />
    </RadixTooltip.Portal>
  );
}
