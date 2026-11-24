import type { ComponentPropsWithoutRef } from 'react';
import * as RadixTooltip from '@radix-ui/react-tooltip';
import { cn } from '@/lib/utils';

export const TooltipProvider = RadixTooltip.Provider;
export const Tooltip = RadixTooltip.Root;
export const TooltipTrigger = RadixTooltip.Trigger;

export function TooltipContent({
  className,
  sideOffset = 6,
  ...props
}: ComponentPropsWithoutRef<typeof RadixTooltip.Content>) {
  return (
    <RadixTooltip.Portal>
      <RadixTooltip.Content
        sideOffset={sideOffset}
        className={cn(
          'z-50 rounded-lg border border-zinc-800 bg-zinc-900/95 px-2.5 py-1.5 text-xs text-zinc-100 shadow-lg backdrop-blur-md',
          className,
        )}
        {...props}
      />
    </RadixTooltip.Portal>
  );
}
