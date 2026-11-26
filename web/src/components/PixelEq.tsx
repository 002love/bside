import { cn } from '@/lib/utils';

export interface PixelEqProps {
  active?: boolean;
  className?: string;
}

/**
 * Quiet, slender 3-bar studio audio level indicator.
 * Replaces chunky pixel/blinking indicators with subtle, calm motion.
 */
export function PixelEq({ active = true, className }: PixelEqProps) {
  return (
    <span
      className={cn('inline-flex h-3 items-end gap-[2px]', className)}
      aria-hidden="true"
    >
      <span
        className={cn(
          'w-[2px] rounded-full transition-colors',
          active
            ? 'h-full bg-[var(--accent-ember)] animate-audio-bar-1'
            : 'h-[3px] bg-[var(--ink-muted)]/40',
        )}
      />
      <span
        className={cn(
          'w-[2px] rounded-full transition-colors',
          active
            ? 'h-full bg-[var(--accent-ember)] animate-audio-bar-2'
            : 'h-[5px] bg-[var(--ink-muted)]/40',
        )}
      />
      <span
        className={cn(
          'w-[2px] rounded-full transition-colors',
          active
            ? 'h-full bg-[var(--accent-ember)] animate-audio-bar-3'
            : 'h-[3px] bg-[var(--ink-muted)]/40',
        )}
      />
    </span>
  );
}

export interface PixelDotProps {
  active?: boolean;
  tone?: 'sage' | 'amber' | 'rust' | 'muted';
  className?: string;
}

const TONE_MAP = {
  sage: 'bg-[var(--accent-ember)]',
  amber: 'bg-[var(--warning)]',
  rust: 'bg-[var(--danger)]',
  muted: 'bg-[var(--ink-muted)]',
} as const;

/**
 * Static, non-pulsing hairline indicator (retained only for legacy references).
 */
export function PixelDot({ tone = 'sage', className }: PixelDotProps) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'inline-block h-1.5 w-1.5 shrink-0 rounded-full',
        TONE_MAP[tone],
        className,
      )}
    />
  );
}
