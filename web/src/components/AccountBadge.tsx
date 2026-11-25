import { useCallback, useEffect, useRef, useState } from 'react';
import { ExternalLink, Filter, Loader2, LogIn, LogOut, Radio, Users } from 'lucide-react';
import { UserAvatar } from '@/components/UserAvatar';
import { PixelEq } from '@/components/PixelEq';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { PopoverSurface } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import type { AccountProfile, LiveFriendItem } from '@/types/api';

export interface AccountBadgeProps {
  profile: AccountProfile | null;
  liveItem?: LiveFriendItem | null;
  isFilteredToSelf?: boolean;
  onFilterSelf?: () => void;
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
      <div className={cn('relative inline-block', className)}>
        <Button
          type="button"
          variant="primary"
          size="md"
          onClick={() => void handleConnectSpotify()}
          disabled={loggingIn}
          className="rounded-[var(--radius-pill)] border-[#1DB954] bg-[#1DB954] text-[#08130B] hover:border-[#1ed760] hover:bg-[#1ed760]"
        >
          {loggingIn ? (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span>Waiting for browser login...</span>
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
  const profileUrl = `https://open.spotify.com/user/${encodeURIComponent(profile.username)}`;

  return (
    <div ref={containerRef} className={cn('relative inline-block', className)}>
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        aria-haspopup="dialog"
        className={cn(
          'inline-flex h-8 items-center gap-2 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] py-1 pr-3 pl-1.5 text-left text-xs text-[var(--ink-primary)] shadow-edge transition-all hover:border-[var(--border-strong)] hover:bg-[var(--bg-hover)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]/70 cursor-pointer',
          open && 'border-[var(--border-accent)] bg-[var(--bg-hover)]',
        )}
      >
        <UserAvatar
          name={displayName}
          imageUrl={profile.image_url}
          size="sm"
          isSelf
          isLive={isListening}
        />
        <span className="max-w-[110px] truncate font-medium">{displayName}</span>
        {isListening && (
          <span className="hidden items-center gap-1.5 border-l border-[var(--border-subtle)] pl-2 sm:inline-flex">
            <PixelEq active className="h-[11px]" />
            <span className="max-w-[110px] truncate text-[11px] text-[var(--accent-primary)]">
              {liveItem?.track_name}
            </span>
          </span>
        )}
      </button>

      <PopoverSurface
        open={open}
        onClose={closePopover}
        anchorRef={containerRef}
        preferredWidth={288}
        role="dialog"
        aria-label="Connected Spotify account"
        className="overflow-y-auto p-3.5"
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
              <div className="flex items-center gap-1.5">
                <span className="truncate text-sm font-semibold text-[var(--ink-primary)]">
                  {displayName}
                </span>
                <Badge variant="default" className="px-1.5 py-0 text-[9px]">
                  YOU
                </Badge>
              </div>
              <p className="mt-0.5 truncate font-mono text-[10px] text-[var(--ink-muted)]">
                Connected Account · @{profile.username}
              </p>
            </div>
          </div>

          {liveItem && (
            <div className="mt-3 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] p-2.5">
              <div className="flex items-center justify-between gap-2">
                <span className="inline-flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider text-[var(--accent-primary)]">
                  {isListening ? (
                    <>
                      <PixelEq active className="h-[10px]" />
                      Listening now
                    </>
                  ) : (
                    <>
                      <Radio className="h-3 w-3 text-[var(--ink-muted)]" />
                      <span className="text-[var(--ink-muted)]">Last played</span>
                    </>
                  )}
                </span>
              </div>
              <div className="mt-1.5 truncate text-xs font-semibold text-[var(--ink-primary)]">
                {liveItem.track_name}
              </div>
              <div className="truncate text-[11px] text-[var(--ink-secondary)]">
                {liveItem.all_artists}
              </div>
            </div>
          )}

          <div className="mt-3 flex flex-col gap-1.5">
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
              className="inline-flex h-7 w-full items-center justify-center gap-1.5 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-transparent px-2.5 font-mono text-[11px] font-medium text-[var(--ink-secondary)] transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]"
            >
              Open Spotify Profile
              <ExternalLink className="h-3 w-3" />
            </a>
            <button
              type="button"
              onClick={() => void handleSignOut()}
              disabled={loggingOut}
              className="inline-flex h-7 w-full items-center justify-center gap-1.5 rounded-[var(--radius-control)] border border-transparent bg-transparent px-2.5 font-mono text-[11px] font-medium text-[var(--ink-muted)] transition-colors hover:border-[var(--border-subtle)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)] disabled:opacity-45 cursor-pointer"
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
