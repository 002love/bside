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
        'flex h-8 items-center justify-between gap-2 rounded-lg border border-zinc-800 bg-zinc-900 px-3 text-xs text-zinc-100 hover:bg-zinc-800/80 focus:outline-none focus:ring-1 focus:ring-emerald-500/70 cursor-pointer',
        className,
      )}
      {...props}
    >
      {children}
      <RadixSelect.Icon asChild>
        <ChevronDown className="h-3.5 w-3.5 text-zinc-400" />
      </RadixSelect.Icon>
    </RadixSelect.Trigger>
  );
}

export function SelectContent({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof RadixSelect.Content>) {
  return (
    <RadixSelect.Portal>
      <RadixSelect.Content
        position="popper"
        sideOffset={4}
        className={cn(
          'z-50 min-w-[11rem] overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900/95 p-1 text-zinc-100 shadow-xl backdrop-blur-md',
          className,
        )}
        {...props}
      >
        <RadixSelect.Viewport className="max-h-72">{children}</RadixSelect.Viewport>
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
        'relative flex w-full cursor-pointer select-none items-center rounded-lg py-1.5 pr-8 pl-2.5 text-xs text-zinc-200 outline-none focus:bg-zinc-800 focus:text-zinc-50 data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
        className,
      )}
      {...props}
    >
      <RadixSelect.ItemText>{children}</RadixSelect.ItemText>
      <span className="absolute right-2 flex h-3.5 w-3.5 items-center justify-center">
        <RadixSelect.ItemIndicator>
          <Check className="h-3.5 w-3.5 text-emerald-400" />
        </RadixSelect.ItemIndicator>
      </span>
    </RadixSelect.Item>
  );
}
