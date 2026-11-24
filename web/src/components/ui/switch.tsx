import type { ComponentPropsWithoutRef } from 'react';
import * as RadixSwitch from '@radix-ui/react-switch';
import { cn } from '@/lib/utils';

export function Switch({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof RadixSwitch.Root>) {
  return (
    <RadixSwitch.Root
      className={cn(
        'peer inline-flex h-4 w-8 shrink-0 cursor-pointer items-center rounded-none border border-[#45453e] bg-[#141412] transition-colors data-[state=checked]:border-[#89b471] data-[state=checked]:bg-[#557445] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[#89b471]',
        className,
      )}
      {...props}
    >
      <RadixSwitch.Thumb className="pointer-events-none block h-2.5 w-2.5 translate-x-0.5 rounded-none bg-[#8d8c84] transition-transform data-[state=checked]:translate-x-[17px] data-[state=checked]:bg-[#efeee8]" />
    </RadixSwitch.Root>
  );
}
