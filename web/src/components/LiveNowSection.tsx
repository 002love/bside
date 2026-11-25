import { useCallback, useEffect, useState } from 'react';
import {
  ArrowRight,
  ChevronDown,
  ChevronUp,
  ExternalLink,
  Filter,
  Loader2,
  Lock,
  LogIn,
  Radio,
  RefreshCw,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { InfoTooltip } from '@/components/InfoTooltip';
import { PixelDot, PixelEq } from '@/components/PixelEq';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { VinylSleeve } from '@/components/VinylSleeve';
import type { UseLiveFeedResult } from '@/hooks/useLiveFeed';
import { cn, formatContextLabel, formatDuration, formatRelativeTime } from '@/lib/utils';
import type { AvatarMap, LiveFriendItem, LiveSnapshot } from '@/types/api';

export interface LiveNowSectionProps {
  live: UseLiveFeedResult;
  mode?: 'full' | 'compact';
  selectedUsers: string[];
  avatarMap: AvatarMap;
  onOpenFull?: () => void;
  onSelectPlaylist?: (contextUri: string) => void;
  onFilterByUser?: (userUri: string) => void;
}

function formatContextTag(item: LiveFriendItem): string {
  const base =
    item.context_name ||
    (item.context_type ? formatContextLabel(item.context_type) : null);
  if (!base) return item.context_is_private ? 'Private' : 'Direct Play';
  return base;
}

function getStatusBanner(snapshot: LiveSnapshot | null, loading: boolean) {
  if (loading && !snapshot) {
    return {
      tone: 'muted' as const,
      label: 'CONNECTING',
      text: 'Opening realtime stream to local Spotify Friend Activity daemon...',
    };
  }
  if (!snapshot) {
    return {
      tone: 'muted' as const,
      label: 'OFFLINE',
      text: 'Realtime live feed is unavailable. Historical SQLite analytics remain active.',
    };
  }
  if (snapshot.connected && snapshot.status === 'live') {
    return null;
  }
  switch (snapshot.status) {
    case 'connecting':
      return {
        tone: 'amber' as const,
        label: 'CONNECTING',
        text: snapshot.message || 'Connecting to Spotify via librespot...',
      };
    case 'unauthenticated':
      return {
        tone: 'amber' as const,
        label: 'UNAUTHENTICATED',
        text: snapshot.message || 'Connect your Spotify account to enable live friend activity.',
      };
    case 'error':
      return {
        tone: 'rust' as const,
        label: 'STREAM ERROR',
        text: snapshot.message || 'Live feed encountered an upstream error. Retrying automatically.',
      };
    default:
      return {
        tone: 'muted' as const,
        label: snapshot.status ? snapshot.status.toUpperCase() : 'OFFLINE',
        text: snapshot.message || 'Live polling is currently paused or offline.',
      };
  }
}

export function LiveNowSection({
  live,
  mode = 'full',
  selectedUsers,
  avatarMap,
  onOpenFull,
  onSelectPlaylist,
  onFilterByUser,
}: LiveNowSectionProps) {
  const [collapsed, setCollapsed] = useState(false);
  const [loggingIn, setLoggingIn] = useState(false);

  const { snapshot, loading, polling, now, pollNow } = live;

  useEffect(() => {
    if (snapshot?.connected) {
      setLoggingIn(false);
    }
  }, [snapshot?.connected]);

  useEffect(() => {
    if (!loggingIn) return;
    let active = true;

    const pollAuthStatus = async () => {
      try {
        const res = await fetch('/api/auth/status');
        if (!res.ok || !active) return;
        const data = (await res.json()) as { authenticated?: boolean; in_progress?: boolean };
        if (data.authenticated || !data.in_progress) {
          setLoggingIn(false);
        }
      } catch {
        // Ignore transient errors while waiting for browser OAuth
      }
    };

    const id = window.setInterval(() => {
      void pollAuthStatus();
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

  const isUnauthenticated = snapshot?.status === 'unauthenticated';
  const showConnectCta = !snapshot?.connected && (isUnauthenticated || loggingIn);

  const allFriends = snapshot?.friends ?? [];
  const hasFilter = selectedUsers.length > 0;
  const selectedLookup = hasFilter
    ? new Set(selectedUsers.map((u) => u.trim().toLowerCase()))
    : null;

  const visibleFriends = selectedLookup
    ? allFriends.filter(
        (f) =>
          selectedLookup.has(f.user_uri.toLowerCase()) ||
          selectedLookup.has(f.user_name.toLowerCase()),
      )
    : allFriends;

  const hiddenCount = allFriends.length - visibleFriends.length;
  const spinningFriends = visibleFriends.filter((f) => f.playing);
  const recentFriends = visibleFriends.filter((f) => !f.playing);
  const totalSpinningCount = allFriends.filter((f) => f.playing).length;

  const banner = getStatusBanner(snapshot, loading);
  const lastSyncLabel = snapshot?.updated_at_ms
    ? formatRelativeTime(snapshot.updated_at_ms)
    : 'Awaiting sync';

  const resolveAvatar = (friend: LiveFriendItem) =>
    friend.user_image_url ?? lookupAvatar(avatarMap, friend.user_uri, friend.user_name);

  const renderContextChip = (friend: LiveFriendItem, compact = false) => {
    const label = formatContextTag(friend);
    const canOpenPlaylist = Boolean(friend.context_uri && onSelectPlaylist);

    const inner = (
      <>
        <PlaylistCover
          name={label}
          imageUrl={friend.context_image_url}
          contextType={friend.context_type}
          isPrivate={friend.context_is_private}
          size="xs"
        />
        {friend.context_is_private && <Lock className="h-2.5 w-2.5 shrink-0 text-[var(--warning)]" />}
        <span className="truncate">{label}</span>
        {friend.context_is_private && (
          <span className="shrink-0 font-mono text-[9px] uppercase text-[var(--warning)]">
            Private
          </span>
        )}
      </>
    );

    const baseClasses = cn(
      'inline-flex items-center gap-1.5 rounded-[var(--radius-pill)] border px-2 py-0.5 text-[11px] font-medium transition-colors',
      compact ? 'max-w-[180px]' : 'max-w-[240px]',
      friend.context_is_private
        ? 'border-[var(--warning)]/40 bg-[var(--warning-soft)] text-[var(--ink-primary)]'
        : 'border-[var(--border-subtle)] bg-[var(--bg-inset)] text-[var(--ink-secondary)]',
    );

    if (canOpenPlaylist && friend.context_uri) {
      return (
        <InfoTooltip content="Open playlist in Playlists tab" side="top">
          <button
            type="button"
            onClick={() => onSelectPlaylist?.(friend.context_uri!)}
            className={cn(
              baseClasses,
              'cursor-pointer hover:border-[var(--border-accent)] hover:bg-[var(--bg-hover)] hover:text-[var(--accent-primary)]',
            )}
          >
            {inner}
          </button>
        </InfoTooltip>
      );
    }

    return <span className={baseClasses}>{inner}</span>;
  };

  if (mode === 'compact') {
    return (
      <section className="bside-card overflow-hidden">
        {/* Compact Strip Header */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-subtle)] bg-[var(--bg-inset)] px-4 py-2.5">
          <div className="flex flex-wrap items-center gap-2.5">
            <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
              On Air
            </span>
            <span className="h-3 w-px bg-[var(--border-subtle)]" />
            <span className="inline-flex items-center gap-2 font-mono text-[11px] font-semibold uppercase tracking-wider text-[var(--ink-primary)]">
              <PixelDot
                active={spinningFriends.length > 0}
                tone={spinningFriends.length > 0 ? 'sage' : banner ? banner.tone : 'muted'}
              />
              {spinningFriends.length > 0
                ? `${spinningFriends.length} spinning now`
                : banner
                  ? banner.label
                  : '0 spinning now'}
            </span>
            {hasFilter && hiddenCount > 0 && (
              <Badge variant="info" className="gap-1">
                <Filter className="h-2.5 w-2.5" />
                Filtered ({visibleFriends.length}/{allFriends.length})
              </Badge>
            )}
            {snapshot?.updated_at_ms && (
              <span className="hidden font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)] sm:inline">
                &middot; Synced {lastSyncLabel}
              </span>
            )}
          </div>

          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={pollNow}
              disabled={polling}
              className="h-6 px-2 text-[10px]"
            >
              <RefreshCw className={cn('h-3 w-3', polling && 'animate-spin')} />
              Poll
            </Button>
            {onOpenFull && (
              <Button
                variant="outline"
                size="sm"
                onClick={onOpenFull}
                className="h-6 px-2 text-[10px]"
              >
                Live Deck
                <ArrowRight className="h-3 w-3" />
              </Button>
            )}
            <button
              type="button"
              onClick={() => setCollapsed((c) => !c)}
              className="inline-flex h-6 w-6 items-center justify-center rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] text-[var(--ink-muted)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--ink-primary)] cursor-pointer"
              title={collapsed ? 'Expand Live Now strip' : 'Collapse Live Now strip'}
              aria-label={collapsed ? 'Expand Live Now strip' : 'Collapse Live Now strip'}
            >
              {collapsed ? (
                <ChevronDown className="h-3.5 w-3.5" />
              ) : (
                <ChevronUp className="h-3.5 w-3.5" />
              )}
            </button>
          </div>
        </div>

        {!collapsed && (
          <div className="p-4">
            {banner && (
              <div className="flex flex-wrap items-center justify-between gap-3 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-3 py-2 font-mono text-[11px] text-[var(--ink-muted)]">
                <span className="flex items-center gap-2">
                  <PixelDot active={banner.tone === 'amber'} tone={banner.tone} />
                  <strong className="text-[var(--ink-primary)]">{banner.label}:</strong>{' '}
                  {banner.text}
                </span>
                {showConnectCta && (
                  <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    onClick={() => void handleConnectSpotify()}
                    disabled={loggingIn}
                    className="rounded-[var(--radius-pill)] border-[#1DB954] bg-[#1DB954] text-[#08130B] hover:border-[#1ed760] hover:bg-[#1ed760]"
                  >
                    {loggingIn ? (
                      <>
                        <Loader2 className="h-3 w-3 animate-spin" />
                        <span>Waiting for browser login...</span>
                      </>
                    ) : (
                      <>
                        <LogIn className="h-3 w-3" />
                        <span>Connect Spotify</span>
                      </>
                    )}
                  </Button>
                )}
              </div>
            )}

            {!banner && spinningFriends.length === 0 && (
              <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-[var(--ink-muted)]">
                <span>
                  {hasFilter && totalSpinningCount > 0
                    ? `None of the selected listeners are playing right now (${totalSpinningCount} other ${totalSpinningCount === 1 ? 'listener is' : 'listeners are'} active).`
                    : 'No listeners are actively playing right now.'}
                </span>
                {recentFriends.length > 0 && onOpenFull && (
                  <button
                    type="button"
                    onClick={onOpenFull}
                    className="font-mono text-[11px] text-[var(--accent-primary)] underline underline-offset-4 transition-colors hover:text-[var(--accent-hover)] cursor-pointer"
                  >
                    View {recentFriends.length} recently played &rarr;
                  </button>
                )}
              </div>
            )}

            {spinningFriends.length > 0 && (
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                {spinningFriends.map((friend) => {
                  const elapsed =
                    friend.duration_ms != null && friend.duration_ms > 0
                      ? Math.min(Math.max(0, now - friend.timestamp_ms), friend.duration_ms)
                      : null;
                  const pct =
                    elapsed != null && friend.duration_ms
                      ? Math.min(100, (elapsed / friend.duration_ms) * 100)
                      : 0;

                  return (
                    <div
                      key={friend.user_uri}
                      className="bside-card-interactive flex items-center gap-3 border-[var(--border-accent)] bg-[var(--bg-inset)] p-3"
                    >
                      <VinylSleeve
                        coverUrl={friend.cover_url}
                        title={friend.track_name}
                        spinning
                        size="sm"
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center justify-between gap-2">
                          <button
                            type="button"
                            onClick={() => onFilterByUser?.(friend.user_uri)}
                            className="flex min-w-0 items-center gap-1.5 text-left transition-colors hover:text-[var(--accent-primary)] cursor-pointer"
                            title={`Filter analytics by ${friend.user_name}`}
                          >
                            <UserAvatar
                              name={friend.user_name}
                              imageUrl={resolveAvatar(friend)}
                              size="xs"
                              isLive
                              isSelf={friend.is_self}
                            />
                            <span className="truncate font-mono text-[10px] font-semibold uppercase tracking-wider text-[var(--accent-primary)] hover:underline">
                              {friend.user_name}
                            </span>
                            {friend.is_self && (
                              <Badge variant="default" className="px-1 py-0 text-[8px]">
                                YOU
                              </Badge>
                            )}
                          </button>
                          <PixelEq active />
                        </div>
                        <a
                          href={friend.spotify_url}
                          target="_blank"
                          rel="noreferrer"
                          className="mt-1 block truncate text-xs font-semibold text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)]"
                        >
                          {friend.track_name}
                        </a>
                        <div className="truncate text-[11px] text-[var(--ink-muted)]">
                          {friend.all_artists}
                        </div>
                        {friend.duration_ms != null && friend.duration_ms > 0 && (
                          <div className="mt-2">
                            <div className="h-[3px] w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--border-subtle)]">
                              <div
                                className="h-full rounded-[var(--radius-pill)] bg-[var(--accent-primary)] transition-[width] duration-1000 ease-linear"
                                style={{ width: `${pct}%` }}
                              />
                            </div>
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </section>
    );
  }

  // FULL LIVE NOW DECK
  return (
    <div className="space-y-6 animate-fade-up">
      {/* Header Bar */}
      <div className="flex flex-wrap items-end justify-between gap-4 border-b border-[var(--border-subtle)] pb-4">
        <div className="space-y-1.5">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="font-display text-3xl font-extrabold leading-none tracking-tight text-[var(--ink-primary)] sm:text-4xl">
              Live Now
            </h2>
            <div className="inline-flex items-center gap-2 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2.5 py-1 font-mono text-[11px] font-semibold uppercase tracking-wider">
              <PixelDot
                active={spinningFriends.length > 0}
                tone={spinningFriends.length > 0 ? 'sage' : banner ? banner.tone : 'muted'}
              />
              <span
                className={
                  spinningFriends.length > 0
                    ? 'text-[var(--accent-primary)]'
                    : 'text-[var(--ink-muted)]'
                }
              >
                {spinningFriends.length} on air
              </span>
            </div>
            {hasFilter && hiddenCount > 0 && (
              <Badge variant="info" className="gap-1">
                <Filter className="h-2.5 w-2.5" />
                Filtered ({visibleFriends.length}/{allFriends.length})
              </Badge>
            )}
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <div className="text-right font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
            <div>Synced {lastSyncLabel}</div>
            {snapshot?.poll_interval_secs ? (
              <div>SSE &middot; {snapshot.poll_interval_secs}s poll</div>
            ) : null}
          </div>
          <Button variant="primary" size="md" onClick={pollNow} disabled={polling}>
            <RefreshCw className={cn('h-3.5 w-3.5', polling && 'animate-spin')} />
            Poll Now
          </Button>
        </div>
      </div>

      {/* Status Banner when not connected/live */}
      {banner && (
        <div className="bside-card flex flex-wrap items-center justify-between gap-3 border-[var(--border-strong)] px-4 py-3">
          <div className="flex items-center gap-3 font-mono text-xs">
            <PixelDot active={banner.tone === 'amber'} tone={banner.tone} />
            <div>
              <span className="font-semibold uppercase tracking-wider text-[var(--ink-primary)]">
                {banner.label}
              </span>
              <span className="mx-2 text-[var(--border-strong)]">/</span>
              <span className="text-[var(--ink-muted)]">{banner.text}</span>
            </div>
          </div>
          {showConnectCta ? (
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
          ) : (
            <Button variant="outline" size="sm" onClick={pollNow} disabled={polling}>
              Retry
            </Button>
          )}
        </div>
      )}

      {/* On Air Grid */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 font-mono text-[11px] font-semibold uppercase tracking-widest text-[var(--ink-primary)]">
            <Radio className="h-3.5 w-3.5 text-[var(--accent-primary)]" />
            <span>On Air</span>
            <span className="text-[var(--ink-muted)]">({spinningFriends.length})</span>
          </div>
        </div>

        {spinningFriends.length === 0 ? (
          <div className="bside-card border-dashed px-6 py-10 text-center">
            <div className="mx-auto mb-3 flex justify-center">
              <PixelEq active={false} />
            </div>
            <p className="font-display text-xl font-bold tracking-tight text-[var(--ink-primary)]">
              {hasFilter && totalSpinningCount > 0
                ? 'No selected listeners on air'
                : 'All turntables idle'}
            </p>
            <p className="mt-1 text-xs text-[var(--ink-muted)]">
              {hasFilter && totalSpinningCount > 0
                ? `${totalSpinningCount} other ${totalSpinningCount === 1 ? 'listener is' : 'listeners are'} playing outside the active filter.`
                : 'Active playback will appear here automatically over the live stream.'}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            {spinningFriends.map((friend) => {
              const hasDuration = friend.duration_ms != null && friend.duration_ms > 0;
              const elapsed = hasDuration
                ? Math.min(Math.max(0, now - friend.timestamp_ms), friend.duration_ms!)
                : null;
              const pct =
                elapsed != null && friend.duration_ms
                  ? Math.min(100, (elapsed / friend.duration_ms) * 100)
                  : 0;

              return (
                <article
                  key={friend.user_uri}
                  className="bside-card-interactive group relative flex flex-col justify-between border-[var(--border-accent)] p-4"
                >
                  {/* Top metadata row: Avatar + Name + Live EQ */}
                  <div className="mb-3 flex items-center justify-between gap-2 border-b border-[var(--border-subtle)] pb-2.5">
                    <button
                      type="button"
                      onClick={() => onFilterByUser?.(friend.user_uri)}
                      className="flex min-w-0 items-center gap-2 text-left transition-colors hover:text-[var(--accent-primary)] cursor-pointer"
                      title={`Filter analytics by ${friend.user_name}`}
                    >
                      <UserAvatar
                        name={friend.user_name}
                        imageUrl={resolveAvatar(friend)}
                        size="sm"
                        isLive
                        isSelf={friend.is_self}
                      />
                      <span className="truncate text-xs font-semibold text-[var(--ink-primary)] group-hover:text-[var(--accent-primary)]">
                        {friend.user_name}
                      </span>
                      {friend.is_self && (
                        <Badge variant="default" className="px-1.5 py-0 text-[9px]">
                          YOU
                        </Badge>
                      )}
                    </button>
                    <div className="flex shrink-0 items-center gap-2">
                      <PixelEq active />
                      <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--accent-primary)]">
                        Live
                      </span>
                    </div>
                  </div>

                  {/* Center body: Spinning Vinyl + Track Metadata */}
                  <div className="flex flex-col gap-4 sm:flex-row sm:items-center">
                    <VinylSleeve
                      coverUrl={friend.cover_url}
                      title={friend.track_name}
                      spinning
                      size="lg"
                    />

                    <div className="min-w-0 flex-1 space-y-2">
                      <div>
                        <a
                          href={friend.spotify_url}
                          target="_blank"
                          rel="noreferrer"
                          className="group/link inline-flex max-w-full items-center gap-1.5 font-display text-2xl font-bold leading-tight tracking-tight text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)]"
                        >
                          <span className="truncate">{friend.track_name}</span>
                          <ExternalLink className="h-3.5 w-3.5 shrink-0 opacity-50 group-hover/link:opacity-100" />
                        </a>
                        <div className="truncate text-xs text-[var(--ink-secondary)]">
                          {friend.all_artists}
                        </div>
                        {friend.album_name && (
                          <div className="mt-0.5 truncate font-mono text-[10px] text-[var(--ink-muted)]">
                            {friend.album_name}
                            {friend.release_date ? ` · ${friend.release_date.slice(0, 4)}` : ''}
                          </div>
                        )}
                      </div>

                      {/* Badges: Context (with cover & click), Explicit, Popularity */}
                      <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                        {renderContextChip(friend)}
                        {friend.explicit && <Badge variant="outline">E</Badge>}
                        {friend.popularity != null && (
                          <InfoTooltip
                            content="Spotify popularity score (0–100) based on recent stream volume"
                            side="top"
                          >
                            <Badge variant="outline">POP {friend.popularity}</Badge>
                          </InfoTooltip>
                        )}
                      </div>
                    </div>
                  </div>

                  {/* Bottom Live Playback Progress Bar */}
                  <div className="mt-4 border-t border-[var(--border-subtle)] pt-3">
                    <div className="mb-1.5 flex items-center justify-between font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
                      <span>Playback</span>
                      <span className="tabular-nums text-[var(--ink-primary)]">
                        {elapsed != null && friend.duration_ms
                          ? `${formatDuration(elapsed)} / ${formatDuration(friend.duration_ms)}`
                          : 'Live stream'}
                      </span>
                    </div>
                    <div className="h-[3px] w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--bg-inset)]">
                      <div
                        className="h-full rounded-[var(--radius-pill)] bg-[var(--accent-primary)] transition-[width] duration-1000 ease-linear"
                        style={{ width: hasDuration ? `${pct}%` : '100%' }}
                      />
                    </div>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </section>

      {/* Recently Played Row */}
      {recentFriends.length > 0 && (
        <section className="space-y-3 pt-2">
          <div className="flex items-center gap-2 font-mono text-[11px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
            <span>Recently Played</span>
            <span>({recentFriends.length})</span>
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {recentFriends.map((friend) => (
              <article
                key={friend.user_uri}
                className="bside-card-interactive flex items-center gap-3.5 p-3"
              >
                <VinylSleeve
                  coverUrl={friend.cover_url}
                  title={friend.track_name}
                  spinning={false}
                  size="sm"
                />

                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between gap-2">
                    <button
                      type="button"
                      onClick={() => onFilterByUser?.(friend.user_uri)}
                      className="flex min-w-0 items-center gap-1.5 text-left transition-colors hover:text-[var(--accent-primary)] cursor-pointer"
                      title={`Filter analytics by ${friend.user_name}`}
                    >
                      <UserAvatar
                        name={friend.user_name}
                        imageUrl={resolveAvatar(friend)}
                        size="xs"
                        isSelf={friend.is_self}
                      />
                      <span className="truncate text-xs font-semibold text-[var(--ink-primary)] hover:text-[var(--accent-primary)]">
                        {friend.user_name}
                      </span>
                      {friend.is_self && (
                        <Badge variant="default" className="px-1 py-0 text-[8px]">
                          YOU
                        </Badge>
                      )}
                    </button>
                    <span className="shrink-0 font-mono text-[10px] text-[var(--ink-muted)]">
                      {formatRelativeTime(friend.timestamp_ms)}
                    </span>
                  </div>

                  <a
                    href={friend.spotify_url}
                    target="_blank"
                    rel="noreferrer"
                    className="mt-1 block truncate text-xs font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)]"
                  >
                    {friend.track_name}
                  </a>
                  <div className="truncate text-[11px] text-[var(--ink-muted)]">
                    {friend.all_artists}
                  </div>
                  {(friend.context_name || friend.context_uri) && (
                    <div className="mt-1.5">{renderContextChip(friend, true)}</div>
                  )}
                </div>
              </article>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
