import { useEffect, useState } from 'react';
import { Disc3, Lock, Music } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface PlaylistCoverProps {
  name: string;
  imageUrl?: string | null;
  contextType?: string | null;
  isPrivate?: boolean;
  size?: 'xs' | 'sm' | 'md' | 'lg';
  className?: string;
}

const SIZE_MAP = {
  xs: { box: 'h-5 w-5', icon: 'h-2.5 w-2.5' },
  sm: { box: 'h-8 w-8', icon: 'h-3.5 w-3.5' },
  md: { box: 'h-11 w-11', icon: 'h-4 w-4' },
  lg: { box: 'h-[72px] w-[72px]', icon: 'h-6 w-6' },
} as const;

const GRADIENT_STYLES = [
  'from-[#1c3829] to-[#122019]',
  'from-[#243248] to-[#151c29]',
  'from-[#3a2644] to-[#1f1526]',
  'from-[#3f3021] to-[#221a13]',
  'from-[#1e3a3a] to-[#122222]',
  'from-[#43252d] to-[#231418]',
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
}: PlaylistCoverProps) {
  const [imgFailed, setImgFailed] = useState(false);

  useEffect(() => {
    setImgFailed(false);
  }, [imageUrl]);

  const cfg = SIZE_MAP[size];
  const gradient = GRADIENT_STYLES[hashString(name || 'playlist') % GRADIENT_STYLES.length];
  const isAlbum = (contextType || '').toLowerCase() === 'album';

  const FallbackIcon = isPrivate ? Lock : isAlbum ? Disc3 : Music;

  return (
    <div
      className={cn(
        'relative shrink-0 overflow-hidden rounded-[calc(var(--radius-control)*0.75)] border border-[var(--border-subtle)] bg-[var(--bg-inset)]',
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
          className="h-full w-full object-cover"
        />
      ) : (
        <div
          className={cn(
            'flex h-full w-full items-center justify-center bg-gradient-to-br text-[var(--ink-secondary)]',
            gradient,
          )}
        >
          <FallbackIcon
            className={cn(
              cfg.icon,
              isPrivate ? 'text-[var(--warning)]' : 'text-[var(--accent-primary)]/85',
            )}
          />
        </div>
      )}
    </div>
  );
}
