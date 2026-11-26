import { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronsUpDown, ExternalLink, Filter, Loader2, LogIn, LogOut, Users } from 'lucide-react';
import { UserAvatar } from '@/components/UserAvatar';
import { PixelEq } from '@/components/PixelEq';
import { Button } from '@/components/ui/button';
import { PopoverSurface } from '@/components/ui/popover';
import { useCoverAccent } from '@/lib/extractColor';
import { cn } from '@/lib/utils';
import type { AccountProfile, LiveFriendItem } from '@/types/api';

export interface AccountBadgeProps {
  profile: AccountProfile | null;
  liveItem?: LiveFriendItem | null;
  isFilteredToSelf?: boolean;
  onFilterSelf?: () => void;
  layout?: 'compact' | 'sidebar';
  className?: string;
}

interface AuthStatusResponse {
  authenticated: boolean;
  in_progress: boolean;
}

export function AccountBadge({
  profile,
  liveItem,
  isFilteredToSelf = false,
  onFilterSelf,
  layout = 'sidebar',
  className,
}: AccountBadgeProps) {
  const [open, setOpen] = useState(false);
  const [loggingIn, setLoggingIn] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);
  const [signedOut, setSignedOut] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (profile && !signedOut) {
      setLoggingIn(false);
    }
  }, [profile, signedOut]);

  useEffect(() => {
    if (!loggingIn) return;
    let active = true;

    const pollStatus = async () => {
      try {
        const res = await fetch('/api/auth/status');
        if (!res.ok || !active) return;
        const data = (await res.json()) as AuthStatusResponse;
        if (data.authenticated) {
          setSignedOut(false);
          setLoggingIn(false);
        } else if (!data.in_progress) {
          setLoggingIn(false);
        }
      } catch {
        // Ignore transient errors while waiting for browser OAuth
      }
    };

    const id = window.setInterval(() => {
      void pollStatus();
    }, 2000);

    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [loggingIn]);

  const handleConnectSpotify = useCallback(async () => {
    if (loggingIn) return;
    setLoggingIn(true);
    try {
      const res = await fetch('/api/auth/login', { method: 'POST' });
      if (!res.ok) {
        setLoggingIn(false);
      }
    } catch {
      setLoggingIn(false);
    }
  }, [loggingIn]);

  const handleSignOut = useCallback(async () => {
    if (loggingOut) return;
    setLoggingOut(true);
    try {
      const res = await fetch('/api/auth/logout', { method: 'POST' });
      if (res.ok) {
        setSignedOut(true);
        setOpen(false);
      }
    } catch {
      // Keep current state if logout request fails
    } finally {
      setLoggingOut(false);
    }
  }, [loggingOut]);

  const closePopover = useCallback(() => setOpen(false), []);

  if (!profile || signedOut) {
    return (
      <div className={cn('relative', layout === 'sidebar' ? 'w-full' : 'inline-block', className)}>
        <Button
          type="button"
          size="md"
          onClick={() => void handleConnectSpotify()}
          disabled={loggingIn}
          className={cn(
            'border-transparent bg-[#1db954] text-[#051209] hover:bg-[#22cc5e]',
            layout === 'sidebar' && 'w-full justify-center',
          )}
        >
          {loggingIn ? (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span>Waiting for login…</span>
            </>
          ) : (
            <>
              <LogIn className="h-3.5 w-3.5" />
              <span>Connect Spotify</span>
            </>
          )}
        </Button>
      </div>
    );
  }

  const displayName = profile.display_name || profile.username;
  const isListening = Boolean(liveItem?.playing);
  const livePalette = useCoverAccent(liveItem?.cover_url, liveItem?.track_uri || liveItem?.track_name);
  const profileUrl = `https://open.spotify.com/user/${encodeURIComponent(profile.username)}`;

  return (
    <div
      ref={containerRef}
      className={cn('relative', layout === 'sidebar' ? 'w-full' : 'inline-block', className)}
    >
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        aria-haspopup="dialog"
        className={cn(
          'tactile-press flex items-center gap-2.5 rounded-[9px] text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--border-focus)] cursor-pointer',
          layout === 'sidebar'
            ? 'w-full px-2 py-2 hover:bg-[var(--bg-hover)]'
            : 'h-8 border border-[var(--border-subtle)] bg-[var(--bg-surface)] py-1 pr-2.5 pl-1.5 hover:border-[var(--border-strong)] hover:bg-[var(--bg-elevated)]',
          open && 'bg-[var(--bg-active)]',
        )}
      >
        <UserAvatar
          name={displayName}
          imageUrl={profile.image_url}
          size={layout === 'sidebar' ? 'md' : 'sm'}
          isSelf
          isLive={isListening}
          ringColor={isListening ? livePalette.accent : undefined}
        />
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-medium text-[var(--ink-primary)]">
            {displayName}
          </div>
          {layout === 'sidebar' && (
            <div className="mt-0.5 flex items-center gap-1.5 truncate text-[11px] text-[var(--ink-muted)]">
              {isListening ? (
                <>
                  <PixelEq active color={livePalette.accent} className="h-2.5 shrink-0" />
                  <span className="truncate text-[var(--ink-secondary)]">{liveItem?.track_name}</span>
                </>
              ) : (
                <span className="truncate">@{profile.username}</span>
              )}
            </div>
          )}
        </div>
        {layout === 'sidebar' && (
          <ChevronsUpDown className="h-3.5 w-3.5 shrink-0 text-[var(--ink-muted)]" />
        )}
      </button>

      <PopoverSurface
        open={open}
        onClose={closePopover}
        anchorRef={containerRef}
        align={layout === 'sidebar' ? 'start' : 'end'}
        preferredWidth={268}
        role="dialog"
        aria-label="Connected Spotify account"
        className="overflow-y-auto p-3"
      >
        <div className="flex items-center gap-3">
          <UserAvatar
            name={displayName}
            imageUrl={profile.image_url}
            size="lg"
            isSelf
            isLive={isListening}
          />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] font-semibold text-[var(--ink-primary)]">
              {displayName}
            </div>
            <p className="mt-0.5 truncate text-[11px] text-[var(--ink-muted)]">
              @{profile.username}
            </p>
          </div>
        </div>

        {liveItem && (
          <div className="mt-3 rounded-[8px] border border-[var(--border-subtle)] bg-[var(--bg-inset)] p-2.5">
            <div className="flex items-center gap-1.5 text-[11px] text-[var(--ink-muted)]">
              {isListening ? (
                <>
                  <PixelEq active className="h-2.5" />
                  <span className="text-[var(--accent-ember)]">Playing now</span>
                </>
              ) : (
                <span>Last played</span>
              )}
            </div>
            <div className="mt-1 truncate text-xs font-medium text-[var(--ink-primary)]">
              {liveItem.track_name}
            </div>
            <div className="truncate text-[11px] text-[var(--ink-secondary)]">
              {liveItem.all_artists}
            </div>
          </div>
        )}

        <div className="mt-3 flex flex-col gap-1">
          {onFilterSelf && (
            <Button
              type="button"
              variant={isFilteredToSelf ? 'secondary' : 'primary'}
              size="sm"
              className="w-full justify-center"
              onClick={() => {
                onFilterSelf();
                setOpen(false);
              }}
            >
              {isFilteredToSelf ? (
                <>
                  <Users className="h-3.5 w-3.5" />
                  Show everyone
                </>
              ) : (
                <>
                  <Filter className="h-3.5 w-3.5" />
                  Filter to my stats
                </>
              )}
            </Button>
          )}
          <a
            href={profileUrl}
            target="_blank"
            rel="noreferrer"
            className="tactile-press inline-flex h-7 w-full items-center justify-center gap-1.5 rounded-[7px] text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]"
          >
            Open Spotify profile
            <ExternalLink className="h-3 w-3" />
          </a>
          <button
            type="button"
            onClick={() => void handleSignOut()}
            disabled={loggingOut}
            className="tactile-press inline-flex h-7 w-full items-center justify-center gap-1.5 rounded-[7px] text-xs font-medium text-[var(--ink-muted)] transition-colors hover:bg-[var(--danger-soft)] hover:text-[var(--danger)] disabled:opacity-45 cursor-pointer"
          >
            {loggingOut ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <LogOut className="h-3 w-3" />
            )}
            Sign out
          </button>
        </div>
      </PopoverSurface>
    </div>
  );
}
