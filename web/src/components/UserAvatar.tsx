import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';
import type { AvatarMap } from '@/types/api';

export interface UserAvatarProps {
  name: string;
  imageUrl?: string | null;
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl' | '2xl';
  isLive?: boolean;
  isSelf?: boolean;
  /** Optional dynamic ring accent color (e.g. from useCoverAccent or Spotify profile color). */
  ringColor?: string;
  className?: string;
}

const SIZE_CLASSES = {
  xs: { box: 'h-[18px] w-[18px]', text: 'text-[8.5px]' },
  sm: { box: 'h-6 w-6', text: 'text-[10px]' },
  md: { box: 'h-8 w-8', text: 'text-[11px]' },
  lg: { box: 'h-11 w-11', text: 'text-xs' },
  xl: { box: 'h-16 w-16', text: 'text-base' },
  '2xl': { box: 'h-28 w-28', text: 'text-2xl' },
} as const;

const FALLBACK_PALETTE = [
  'bg-[#2b211c] text-[#f0c0a2]',
  'bg-[#1e2633] text-[#b4ccf0]',
  'bg-[#1f2e26] text-[#a7dec0]',
  'bg-[#2d241c] text-[#ebd1aa]',
  'bg-[#262033] text-[#cbbef0]',
  'bg-[#301e25] text-[#f0b4c4]',
] as const;

function hashName(input: string): number {
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) >>> 0;
  }
  return hash;
}

function getInitials(name: string): string {
  const cleaned = (name || '?').trim();
  if (!cleaned) return '?';
  const parts = cleaned.split(/[\s._-]+/).filter(Boolean);
  if (parts.length >= 2) {
    return `${parts[0][0] ?? ''}${parts[1][0] ?? ''}`.toUpperCase();
  }
  return cleaned.slice(0, 2).toUpperCase();
}

export function lookupAvatar(
  avatarMap: AvatarMap | undefined,
  userUri?: string | null,
  userName?: string | null,
): string | null {
  if (!avatarMap) return null;
  if (userUri && avatarMap[userUri]?.imageUrl) {
    return avatarMap[userUri].imageUrl;
  }
  if (userName) {
    if (avatarMap[userName]?.imageUrl) {
      return avatarMap[userName].imageUrl;
    }
    const lower = userName.toLowerCase();
    for (const entry of Object.values(avatarMap)) {
      if (entry.name.toLowerCase() === lower && entry.imageUrl) {
        return entry.imageUrl;
      }
    }
  }
  return null;
}

export function UserAvatar({
  name,
  imageUrl,
  size = 'md',
  isLive = false,
  isSelf = false,
  ringColor,
  className,
}: UserAvatarProps) {
  const [imgFailed, setImgFailed] = useState(false);

  useEffect(() => {
    setImgFailed(false);
  }, [imageUrl]);

  const cfg = SIZE_CLASSES[size];
  const paletteClass = FALLBACK_PALETTE[hashName(name || '?') % FALLBACK_PALETTE.length];
  const initials = getInitials(name);

  const customRingStyle =
    isLive && ringColor
      ? { boxShadow: `0 0 0 1.5px #09090b, 0 0 0 3px ${ringColor}` }
      : ringColor && !isLive
        ? { boxShadow: `0 0 0 1.5px ${ringColor}` }
        : undefined;

  return (
    <span
      className={cn(
        'relative inline-flex shrink-0 select-none items-center justify-center',
        cfg.box,
        className,
      )}
      title={name}
    >
      <span
        style={customRingStyle}
        className={cn(
          'flex h-full w-full items-center justify-center overflow-hidden rounded-full bg-[var(--bg-elevated)] ring-1 ring-inset ring-white/12 transition-shadow duration-300',
          isLive &&
            !ringColor &&
            'ring-[1.5px] ring-[var(--accent-ember)] ring-offset-[1.5px] ring-offset-[var(--bg-canvas)]',
          !isLive && !ringColor && isSelf && 'ring-white/25',
        )}
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
          <span
            className={cn(
              'flex h-full w-full items-center justify-center font-semibold leading-none tracking-tight',
              cfg.text,
              paletteClass,
            )}
          >
            {initials}
          </span>
        )}
      </span>
    </span>
  );
}
