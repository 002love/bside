import type { ComponentPropsWithoutRef } from 'react';
import * as RadixSelect from '@radix-ui/react-select';
import { Check, ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';

export const Select = RadixSelect.Root;
export const SelectValue = RadixSelect.Value;

export function SelectTrigger({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof RadixSelect.Trigger>) {
  return (
    <RadixSelect.Trigger
      className={cn(
        'tactile-press flex h-8 items-center justify-between gap-2 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2.5 text-xs text-[var(--ink-primary)] transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--bg-surface)] focus:border-[var(--border-focus)] focus:outline-none cursor-pointer',
        className,
      )}
      {...props}
    >
      {children}
      <RadixSelect.Icon asChild>
        <ChevronDown className="h-3.5 w-3.5 text-[var(--ink-muted)]" />
      </RadixSelect.Icon>
    </RadixSelect.Trigger>
  );
}

export function SelectContent({
  className,
  children,
  collisionPadding = 12,
  ...props
}: ComponentPropsWithoutRef<typeof RadixSelect.Content>) {
  return (
    <RadixSelect.Portal>
      <RadixSelect.Content
        position="popper"
        sideOffset={6}
        collisionPadding={collisionPadding}
        className={cn(
          'z-50 min-w-[11rem] max-w-[calc(100vw-1.5rem)] overflow-hidden bside-popover p-1',
          className,
        )}
        {...props}
      >
        <RadixSelect.Viewport className="max-h-[min(18rem,var(--radix-select-content-available-height))]">
          {children}
        </RadixSelect.Viewport>
      </RadixSelect.Content>
    </RadixSelect.Portal>
  );
}

export function SelectItem({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof RadixSelect.Item>) {
  return (
    <RadixSelect.Item
      className={cn(
        'relative flex w-full cursor-pointer select-none items-center rounded-[6px] py-1.5 pr-8 pl-2.5 text-xs text-[var(--ink-secondary)] outline-none transition-colors focus:bg-[var(--bg-hover)] focus:text-[var(--ink-primary)] data-[state=checked]:text-[var(--ink-primary)] data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
        className,
      )}
      {...props}
    >
      <RadixSelect.ItemText>{children}</RadixSelect.ItemText>
      <span className="absolute right-2 flex h-3.5 w-3.5 items-center justify-center">
        <RadixSelect.ItemIndicator>
          <Check className="h-3.5 w-3.5 text-[var(--accent-ember)]" />
        </RadixSelect.ItemIndicator>
      </span>
    </RadixSelect.Item>
  );
}
