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
        'inline-flex items-center gap-1 rounded-xl border border-zinc-800/80 bg-zinc-900/70 p-1 text-zinc-400',
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
        'inline-flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium whitespace-nowrap transition-all cursor-pointer hover:text-zinc-200 data-[state=active]:bg-zinc-800 data-[state=active]:text-emerald-400 data-[state=active]:shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500',
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
      className={cn('mt-5 focus-visible:outline-none', className)}
      {...props}
    />
  );
}
