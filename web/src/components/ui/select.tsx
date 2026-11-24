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
        'flex h-8 items-center justify-between gap-2 rounded-none border border-[#2b2b27] bg-[#141412] px-3 font-mono text-[11px] text-[#efeee8] hover:border-[#45453e] hover:bg-[#20201d] focus:border-[#89b471] focus:outline-none cursor-pointer',
        className,
      )}
      {...props}
    >
      {children}
      <RadixSelect.Icon asChild>
        <ChevronDown className="h-3.5 w-3.5 text-[#8d8c84]" />
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
          'z-50 min-w-[12rem] overflow-hidden rounded-none border border-[#45453e] bg-[#181816] p-1 text-[#efeee8] shadow-edge',
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
        'relative flex w-full cursor-pointer select-none items-center rounded-none py-1.5 pr-8 pl-2.5 font-mono text-[11px] text-[#efeee8] outline-none focus:bg-[#20201d] focus:text-[#89b471] data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
        className,
      )}
      {...props}
    >
      <RadixSelect.ItemText>{children}</RadixSelect.ItemText>
      <span className="absolute right-2 flex h-3.5 w-3.5 items-center justify-center">
        <RadixSelect.ItemIndicator>
          <Check className="h-3.5 w-3.5 text-[#89b471]" />
        </RadixSelect.ItemIndicator>
      </span>
    </RadixSelect.Item>
  );
}
