import { useLayoutEffect, useRef, useState } from 'react';
import { Disc3, Lock, Music2 } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface CoverArtProps {
  src?: string | null;
  alt: string;
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl' | 'fluid';
  kind?: 'track' | 'playlist' | 'album';
  isPrivate?: boolean;
  className?: string;
  hoverZoom?: boolean;
}

const SIZE_CLASSES = {
  xs: { box: 'h-6 w-6 rounded-[5px]', icon: 'h-3 w-3' },
  sm: { box: 'h-9 w-9 rounded-[6px]', icon: 'h-3.5 w-3.5' },
  md: { box: 'h-12 w-12 rounded-[8px]', icon: 'h-4 w-4' },
  lg: { box: 'h-20 w-20 rounded-[10px]', icon: 'h-6 w-6' },
  xl: { box: 'h-36 w-36 rounded-[12px]', icon: 'h-8 w-8' },
  fluid: { box: 'aspect-square w-full rounded-[10px]', icon: 'h-7 w-7' },
} as const;

const FALLBACK_GRADIENTS = [
  'from-[#2c211d] via-[#1c1716] to-[#121216]',
  'from-[#1e2636] via-[#161a24] to-[#121216]',
  'from-[#212d26] via-[#171d1a] to-[#121216]',
  'from-[#2d241b] via-[#1d1915] to-[#121216]',
  'from-[#262131] via-[#191720] to-[#121216]',
  'from-[#2d1f23] via-[#1c1618] to-[#121216]',
] as const;

function hashString(input: string): number {
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) >>> 0;
  }
  return hash;
}

export function CoverArt({
  src,
  alt,
  size = 'md',
  kind = 'track',
  isPrivate = false,
  className,
  hoverZoom = false,
}: CoverArtProps) {
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const [loadedSrc, setLoadedSrc] = useState<string | null>(null);

  const failed = Boolean(src && failedSrc === src);
  const loaded = Boolean(src && loadedSrc === src);

  useLayoutEffect(() => {
    const img = imgRef.current;
    if (src && img && img.complete && img.naturalWidth > 0) {
      setLoadedSrc(src);
    }
  }, [src]);

  const cfg = SIZE_CLASSES[size];
  const gradient = FALLBACK_GRADIENTS[hashString(alt || 'bside') % FALLBACK_GRADIENTS.length];
  const Icon = isPrivate ? Lock : kind === 'album' ? Disc3 : Music2;

  return (
    <div
      className={cn(
        'relative shrink-0 overflow-hidden bg-[var(--bg-inset)] ring-1 ring-inset ring-white/10',
        cfg.box,
        className,
      )}
    >
      {src && !failed ? (
        <img
          key={src}
          ref={imgRef}
          src={src}
          alt={alt}
          loading="lazy"
          onLoad={() => setLoadedSrc(src)}
          onError={() => setFailedSrc(src)}
          className={cn(
            'h-full w-full object-cover transition-all duration-300 ease-out',
            loaded ? 'opacity-100' : 'opacity-0',
            hoverZoom && 'group-hover:scale-[1.04]',
          )}
        />
      ) : (
        <div
          className={cn(
            'flex h-full w-full items-center justify-center bg-gradient-to-br',
            gradient,
          )}
        >
          <Icon
            className={cn(
              cfg.icon,
              isPrivate ? 'text-[var(--warning)]/80' : 'text-white/30',
            )}
          />
        </div>
      )}
    </div>
  );
}
