import { cn } from '@/lib/utils';

export interface PixelEqProps {
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
          className="relative flex h-full w-[3px] items-end overflow-hidden rounded-[var(--radius-pill)] bg-[var(--border-subtle)]"
        >
          <span
            className={cn(
              'block w-[3px] overflow-hidden rounded-[var(--radius-pill)]',
              active ? animClass : 'h-[3px]',
            )}
          >
            {/* 4 stacked 3x3px equalizer bars */}
            <span className="flex flex-col gap-[1px]">
              <span
                className={cn(
                  'block h-[3px] w-[3px]',
                  active ? 'bg-[var(--accent-primary)]' : 'bg-[var(--border-strong)]',
                )}
              />
              <span
                className={cn(
                  'block h-[3px] w-[3px]',
                  active ? 'bg-[var(--accent-primary)]' : 'bg-[var(--border-strong)]',
                )}
              />
              <span
                className={cn(
                  'block h-[3px] w-[3px]',
                  active ? 'bg-[var(--accent-primary)]' : 'bg-[var(--border-strong)]',
                )}
              />
              <span
                className={cn(
                  'block h-[3px] w-[3px]',
                  active ? 'bg-[var(--accent-primary)]' : 'bg-[var(--border-strong)]',
                )}
              />
            </span>
          </span>
        </span>
      ))}
    </span>
  );
}

export interface PixelDotProps {
  active?: boolean;
  tone?: 'sage' | 'amber' | 'rust' | 'muted';
  className?: string;
}

const TONE_MAP = {
  sage: 'bg-[var(--accent-primary)]',
  amber: 'bg-[var(--warning)]',
  rust: 'bg-[var(--danger)]',
  muted: 'bg-[var(--ink-muted)]',
} as const;

export function PixelDot({ active = true, tone = 'sage', className }: PixelDotProps) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'inline-block h-[7px] w-[7px] shrink-0 rounded-[var(--radius-pill)]',
        TONE_MAP[tone],
        active && (tone === 'sage' ? 'animate-live-dot' : 'animate-pixel-blink'),
        className,
      )}
    />
  );
}
