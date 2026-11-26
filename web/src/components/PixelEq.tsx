import { cn } from '@/lib/utils';

export interface PixelEqProps {
  active?: boolean;
  /** Dynamic track/playlist accent color (e.g. from useCoverAccent). Defaults to --accent-ember. */
  color?: string;
  className?: string;
}

/**
 * Quiet, slender 3-bar studio audio level indicator that dynamically adopts
 * the active track's extracted cover color.
 */
export function PixelEq({ active = true, color, className }: PixelEqProps) {
  const barStyle = active && color ? { backgroundColor: color } : undefined;

  return (
    <span
      className={cn('inline-flex h-3 items-end gap-[2px]', className)}
      aria-hidden="true"
    >
      <span
        style={barStyle}
        className={cn(
          'w-[2px] rounded-full transition-colors duration-300',
          active
            ? cn('h-full animate-audio-bar-1', !color && 'bg-[var(--accent-ember)]')
            : 'h-[3px] bg-white/25',
        )}
      />
      <span
        style={barStyle}
        className={cn(
          'w-[2px] rounded-full transition-colors duration-300',
          active
            ? cn('h-full animate-audio-bar-2', !color && 'bg-[var(--accent-ember)]')
            : 'h-[5px] bg-white/25',
        )}
      />
      <span
        style={barStyle}
        className={cn(
          'w-[2px] rounded-full transition-colors duration-300',
          active
            ? cn('h-full animate-audio-bar-3', !color && 'bg-[var(--accent-ember)]')
            : 'h-[3px] bg-white/25',
        )}
      />
    </span>
  );
}
