import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';
import type { AvatarMap } from '@/types/api';

export interface UserAvatarProps {
  name: string;
  imageUrl?: string | null;
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl';
  isLive?: boolean;
  isSelf?: boolean;
  className?: string;
}

const SIZE_CLASSES = {
  xs: { box: 'h-[18px] w-[18px]', text: 'text-[8.5px]' },
  sm: { box: 'h-6 w-6', text: 'text-[10px]' },
  md: { box: 'h-8 w-8', text: 'text-[11px]' },
  lg: { box: 'h-10 w-10', text: 'text-xs' },
  xl: { box: 'h-14 w-14', text: 'text-sm' },
} as const;

const FALLBACK_PALETTE = [
  'bg-[#2b211c] text-[#e7b99c]',
  'bg-[#1e2633] text-[#b4ccf0]',
  'bg-[#222c26] text-[#b6d7c1]',
  'bg-[#2b241d] text-[#e6cca8]',
  'bg-[#252230] text-[#c7c0e8]',
  'bg-[#2d2025] text-[#e8b8c4]',
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
  className,
}: UserAvatarProps) {
  const [imgFailed, setImgFailed] = useState(false);

  useEffect(() => {
    setImgFailed(false);
  }, [imageUrl]);

  const cfg = SIZE_CLASSES[size];
  const paletteClass = FALLBACK_PALETTE[hashName(name || '?') % FALLBACK_PALETTE.length];
  const initials = getInitials(name);

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
        className={cn(
          'flex h-full w-full items-center justify-center overflow-hidden rounded-full bg-[var(--bg-elevated)] ring-1 ring-inset ring-white/10 transition-shadow',
          isLive && 'ring-[1.5px] ring-[var(--accent-ember)] ring-offset-[1.5px] ring-offset-[var(--bg-canvas)]',
          !isLive && isSelf && 'ring-white/25',
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
              'flex h-full w-full items-center justify-center font-medium leading-none tracking-tight',
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
