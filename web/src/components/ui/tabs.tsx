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
        'inline-flex items-center gap-0.5 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] p-0.5 text-[var(--ink-muted)]',
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
        'tactile-press inline-flex items-center justify-center gap-1.5 rounded-[6px] px-3 py-1.5 text-xs font-medium whitespace-nowrap transition-colors cursor-pointer hover:text-[var(--ink-primary)] data-[state=active]:bg-[var(--bg-elevated)] data-[state=active]:text-[var(--ink-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--border-focus)]',
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
      className={cn('mt-5 animate-fade-up focus-visible:outline-none', className)}
      {...props}
    />
  );
}
