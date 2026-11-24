import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';

export interface VinylSleeveProps {
  coverUrl: string | null | undefined;
  title: string;
  spinning?: boolean;
  size?: 'sm' | 'md' | 'lg';
  className?: string;
}

const SIZE_CONFIG = {
  sm: {
    wrapper: 'h-12 w-[66px]',
    sleeve: 'h-12 w-12',
    vinyl: 'h-[44px] w-[44px] top-[2px]',
    tuckedLeft: 'left-[14px]',
    extendedLeft: 'left-[14px] translate-x-2',
    fallbackText: 'text-[9px]',
  },
  md: {
    wrapper: 'h-16 w-[92px]',
    sleeve: 'h-16 w-16',
    vinyl: 'h-[58px] w-[58px] top-[3px]',
    tuckedLeft: 'left-[20px]',
    extendedLeft: 'left-[20px] translate-x-3',
    fallbackText: 'text-[10px]',
  },
  lg: {
    wrapper: 'h-24 w-[136px]',
    sleeve: 'h-24 w-24',
    vinyl: 'h-[88px] w-[88px] top-[4px]',
    tuckedLeft: 'left-[30px]',
    extendedLeft: 'left-[30px] translate-x-4',
    fallbackText: 'text-xs',
  },
} as const;

export function VinylSleeve({
  coverUrl,
  title,
  spinning = false,
  size = 'md',
  className,
}: VinylSleeveProps) {
  const [imgFailed, setImgFailed] = useState(false);

  useEffect(() => {
    setImgFailed(false);
  }, [coverUrl]);

  const cfg = SIZE_CONFIG[size];
  const initials = (title || 'BS')
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? '')
    .join('') || 'BS';

  return (
    <div
      className={cn('relative shrink-0 select-none', cfg.wrapper, className)}
      aria-label={spinning ? `${title} (spinning)` : title}
    >
      {/* 12" Vinyl Disc emerging from right side of sleeve */}
      <div
        className={cn(
          'absolute z-0 transition-transform duration-500 ease-out',
          cfg.vinyl,
          spinning ? cfg.extendedLeft : cfg.tuckedLeft,
        )}
      >
        <svg
          viewBox="0 0 100 100"
          className={cn('h-full w-full drop-shadow-[0_2px_4px_rgba(0,0,0,0.65)]', spinning && 'animate-vinyl-spin')}
          aria-hidden="true"
        >
          {/* Outer matte charcoal vinyl disc */}
          <circle cx="50" cy="50" r="49" fill="#121210" stroke="#2b2b27" strokeWidth="1.2" />
          {/* Subtle concentric groove rings */}
          <circle cx="50" cy="50" r="44" fill="none" stroke="#1e1e1b" strokeWidth="0.9" />
          <circle cx="50" cy="50" r="39" fill="none" stroke="#1a1a17" strokeWidth="0.7" />
          <circle cx="50" cy="50" r="34" fill="none" stroke="#22221e" strokeWidth="0.8" />
          <circle cx="50" cy="50" r="29" fill="none" stroke="#1b1b18" strokeWidth="0.7" />
          <circle cx="50" cy="50" r="24" fill="none" stroke="#23231f" strokeWidth="0.8" />
          {/* Soft diagonal sheen arcs */}
          <path
            d="M 18 26 A 42 42 0 0 1 72 16"
            fill="none"
            stroke="rgba(239, 238, 232, 0.14)"
            strokeWidth="3.5"
            strokeLinecap="butt"
          />
          <path
            d="M 82 74 A 42 42 0 0 1 28 84"
            fill="none"
            stroke="rgba(239, 238, 232, 0.08)"
            strokeWidth="2.5"
            strokeLinecap="butt"
          />
          {/* Moss-green center label */}
          <circle cx="50" cy="50" r="17" fill="#557445" stroke="#2b2b27" strokeWidth="1" />
          {/* Subtle inner label ring & alignment mark */}
          <circle cx="50" cy="50" r="12" fill="none" stroke="rgba(239, 238, 232, 0.25)" strokeWidth="0.75" />
          <rect x="48.5" y="36" width="3" height="4" fill="#efeee8" opacity="0.65" />
          {/* Center spindle hole */}
          <circle cx="50" cy="50" r="3.4" fill="#111110" stroke="#2b2b27" strokeWidth="0.8" />
        </svg>
      </div>

      {/* Square 0px-radius album cover sleeve */}
      <div
        className={cn(
          'relative z-10 overflow-hidden rounded-none border border-[#2b2b27] bg-[#141412] shadow-[2px_0_6px_rgba(0,0,0,0.55)]',
          cfg.sleeve,
        )}
      >
        {coverUrl && !imgFailed ? (
          <img
            src={coverUrl}
            alt={title}
            loading="lazy"
            onError={() => setImgFailed(true)}
            className="h-full w-full rounded-none object-cover"
          />
        ) : (
          <div className="relative flex h-full w-full flex-col items-center justify-center bg-[#141412] p-1">
            <div className="pointer-events-none absolute inset-1 border border-[#2b2b27]/80" />
            <div className="h-1.5 w-1.5 bg-[#557445] mb-1" />
            <span className={cn('font-mono font-semibold uppercase tracking-widest text-[#8d8c84]', cfg.fallbackText)}>
              {initials}
            </span>
          </div>
        )}
        {/* Subtle left spine crease */}
        <div className="pointer-events-none absolute inset-y-0 left-0 w-[2px] bg-[#efeee8]/10" />
      </div>
    </div>
  );
}
