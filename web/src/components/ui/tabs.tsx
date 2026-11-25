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
        'inline-flex items-center gap-1 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] p-1 text-[var(--ink-muted)]',
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
        'inline-flex items-center justify-center gap-1.5 rounded-[calc(var(--radius-control)*0.75)] px-3.5 py-1.5 font-mono text-[11px] font-semibold uppercase tracking-wider whitespace-nowrap transition-all cursor-pointer hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)] data-[state=active]:bg-[var(--accent-primary)] data-[state=active]:text-[var(--ink-on-accent)] data-[state=active]:shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]/70',
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
      className={cn('mt-6 animate-fade-up focus-visible:outline-none', className)}
      {...props}
    />
  );
}
