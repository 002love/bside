import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';
import type { AvatarMap } from '@/types/api';

export interface UserAvatarProps {
  name: string;
  imageUrl?: string | null;
  size?: 'xs' | 'sm' | 'md' | 'lg';
  isLive?: boolean;
  isSelf?: boolean;
  className?: string;
}

const SIZE_CLASSES = {
  xs: { box: 'h-[18px] w-[18px]', text: 'text-[8px]', dot: 'h-[6px] w-[6px] -right-[1px] -bottom-[1px]' },
  sm: { box: 'h-[22px] w-[22px]', text: 'text-[9px]', dot: 'h-[7px] w-[7px] -right-[1px] -bottom-[1px]' },
  md: { box: 'h-[28px] w-[28px]', text: 'text-[10px]', dot: 'h-[8px] w-[8px] -right-[1px] -bottom-[1px]' },
  lg: { box: 'h-[40px] w-[40px]', text: 'text-xs', dot: 'h-[10px] w-[10px] right-0 bottom-0' },
} as const;

const FALLBACK_PALETTE = [
  'bg-[#1f4d35] text-[#b9f3ce]',
  'bg-[#233d5a] text-[#bfd9f8]',
  'bg-[#4a3254] text-[#e4c4f1]',
  'bg-[#523b26] text-[#f4d7b7]',
  'bg-[#254b4b] text-[#b8ecec]',
  'bg-[#552d36] text-[#f5c2cd]',
  'bg-[#3b4728] text-[#d5e8b7]',
  'bg-[#3a3656] text-[#d2ccf8]',
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
          'flex h-full w-full items-center justify-center overflow-hidden rounded-[var(--radius-avatar)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)]',
          isSelf && 'ring-2 ring-[var(--accent-primary)]/70 ring-offset-1 ring-offset-[var(--bg-canvas)]',
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
              'flex h-full w-full items-center justify-center font-mono font-semibold leading-none tracking-tight',
              cfg.text,
              paletteClass,
            )}
          >
            {initials}
          </span>
        )}
      </span>
      {isLive && (
        <span
          aria-label="Listening now"
          className={cn(
            'absolute rounded-full border-[1.5px] border-[var(--bg-panel)] bg-[var(--accent-primary)] animate-live-dot',
            cfg.dot,
          )}
        />
      )}
    </span>
  );
}
