import { useEffect, useState } from 'react';
import { Disc3, Lock, Music2 } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface PlaylistCoverProps {
  name: string;
  imageUrl?: string | null;
  contextType?: string | null;
  isPrivate?: boolean;
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl' | 'fluid';
  className?: string;
  hoverZoom?: boolean;
}

const SIZE_MAP = {
  xs: { box: 'h-5 w-5 rounded-[4px]', icon: 'h-2.5 w-2.5' },
  sm: { box: 'h-9 w-9 rounded-[6px]', icon: 'h-3.5 w-3.5' },
  md: { box: 'h-12 w-12 rounded-[8px]', icon: 'h-4 w-4' },
  lg: { box: 'h-[72px] w-[72px] rounded-[10px]', icon: 'h-6 w-6' },
  xl: { box: 'h-32 w-32 rounded-[12px]', icon: 'h-8 w-8' },
  fluid: { box: 'aspect-square w-full rounded-[10px]', icon: 'h-7 w-7' },
} as const;

const GRADIENT_STYLES = [
  'from-[#2b201c] via-[#1c1614] to-[#121216]',
  'from-[#1e2636] via-[#161a24] to-[#121216]',
  'from-[#202c25] via-[#171d1a] to-[#121216]',
  'from-[#2d241b] via-[#1d1915] to-[#121216]',
  'from-[#252130] via-[#191720] to-[#121216]',
  'from-[#2d1f24] via-[#1c1618] to-[#121216]',
] as const;

function hashString(input: string): number {
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) >>> 0;
  }
  return hash;
}

export function PlaylistCover({
  name,
  imageUrl,
  contextType,
  isPrivate = false,
  size = 'md',
  className,
  hoverZoom = false,
}: PlaylistCoverProps) {
  const [imgFailed, setImgFailed] = useState(false);

  useEffect(() => {
    setImgFailed(false);
  }, [imageUrl]);

  const cfg = SIZE_MAP[size];
  const gradient = GRADIENT_STYLES[hashString(name || 'playlist') % GRADIENT_STYLES.length];
  const isAlbum = (contextType || '').toLowerCase() === 'album';
  const FallbackIcon = isPrivate ? Lock : isAlbum ? Disc3 : Music2;

  return (
    <div
      className={cn(
        'relative shrink-0 overflow-hidden bg-[var(--bg-inset)] ring-1 ring-inset ring-white/10',
        cfg.box,
        className,
      )}
      title={name}
    >
      {imageUrl && !imgFailed ? (
        <img
          src={imageUrl}
          alt={name}
          loading="lazy"
          onError={() => setImgFailed(true)}
          className={cn(
            'h-full w-full object-cover transition-transform duration-300 ease-out',
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
          <FallbackIcon
            className={cn(
              cfg.icon,
              isPrivate ? 'text-[var(--warning)]/80' : 'text-white/35',
            )}
          />
        </div>
      )}
    </div>
  );
}
