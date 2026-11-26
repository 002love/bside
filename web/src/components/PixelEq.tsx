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
