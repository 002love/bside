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
          'z-50 rounded-none border border-[#45453e] bg-[#181816] px-2.5 py-1.5 font-mono text-[11px] text-[#efeee8] shadow-edge',
          className,
        )}
        {...props}
      />
    </RadixTooltip.Portal>
  );
}
