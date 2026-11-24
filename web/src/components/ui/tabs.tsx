import type { ComponentPropsWithoutRef } from 'react';
import * as RadixTabs from '@radix-ui/react-tabs';
import { cn } from '@/lib/utils';

export const Tabs = RadixTabs.Root;

export function TabsList({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof RadixTabs.List>) {
  return (
    <RadixTabs.List
      className={cn(
        'inline-flex items-center gap-0 rounded-none border border-[#2b2b27] bg-[#141412] p-0 text-[#8d8c84]',
        className,
      )}
      {...props}
    />
  );
}

export function TabsTrigger({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof RadixTabs.Trigger>) {
  return (
    <RadixTabs.Trigger
      className={cn(
        'inline-flex items-center justify-center gap-1.5 rounded-none border-r border-[#2b2b27] last:border-r-0 px-3.5 py-2 font-mono text-[11px] font-semibold uppercase tracking-wider whitespace-nowrap transition-colors cursor-pointer hover:bg-[#20201d] hover:text-[#efeee8] data-[state=active]:bg-[#efeee8] data-[state=active]:text-[#111110] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[#89b471]',
        className,
      )}
      {...props}
    />
  );
}

export function TabsContent({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof RadixTabs.Content>) {
  return (
    <RadixTabs.Content
      className={cn('mt-6 focus-visible:outline-none', className)}
      {...props}
    />
  );
}
