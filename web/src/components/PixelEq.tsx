import { cn } from '@/lib/utils';

interface PixelEqProps {
  active?: boolean;
  className?: string;
}

const COLUMNS = [
  'animate-pixel-eq-1',
  'animate-pixel-eq-2',
  'animate-pixel-eq-3',
  'animate-pixel-eq-4',
] as const;

export function PixelEq({ active = true, className }: PixelEqProps) {
  return (
    <span
      className={cn('inline-flex h-[15px] items-end gap-[2px]', className)}
      aria-hidden="true"
    >
      {COLUMNS.map((animClass, colIdx) => (
        <span
          key={colIdx}
          className="relative flex h-full w-[3px] items-end overflow-hidden bg-[#2b2b27]/40"
        >
          <span
            className={cn(
              'block w-[3px] overflow-hidden',
              active ? animClass : 'h-[3px]',
            )}
          >
            {/* 4 stacked crisp 3x3px pixel blocks with 1px vertical gaps */}
            <span className="flex flex-col gap-[1px]">
              <span className={cn('block h-[3px] w-[3px]', active ? 'bg-[#89b471]' : 'bg-[#45453e]')} />
              <span className={cn('block h-[3px] w-[3px]', active ? 'bg-[#89b471]' : 'bg-[#45453e]')} />
              <span className={cn('block h-[3px] w-[3px]', active ? 'bg-[#89b471]' : 'bg-[#45453e]')} />
              <span className={cn('block h-[3px] w-[3px]', active ? 'bg-[#89b471]' : 'bg-[#45453e]')} />
            </span>
          </span>
        </span>
      ))}
    </span>
  );
}

interface PixelDotProps {
  active?: boolean;
  tone?: 'sage' | 'amber' | 'rust' | 'muted';
  className?: string;
}

const TONE_MAP = {
  sage: 'bg-[#89b471]',
  amber: 'bg-[#c48a47]',
  rust: 'bg-[#b85d4f]',
  muted: 'bg-[#5c5b55]',
} as const;

export function PixelDot({ active = true, tone = 'sage', className }: PixelDotProps) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'inline-block h-[6px] w-[6px] shrink-0 rounded-none',
        TONE_MAP[tone],
        active && 'animate-pixel-blink',
        className,
      )}
    />
  );
}
