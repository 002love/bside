import { useCallback, useEffect, useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import {
  AlertCircle,
  ArrowRight,
  ChevronDown,
  ExternalLink,
  Loader2,
  Lock,
  LogIn,
  RefreshCw,
  WifiOff,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  AmbientArtGlow,
  EASE_OUT_EXPO,
  SPRING_SMOOTH,
  StaggerItem,
} from '@/components/ui/motion';
import { CoverArt } from '@/components/CoverArt';
import { InfoTooltip } from '@/components/InfoTooltip';
import { PixelEq } from '@/components/PixelEq';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
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

interface TransportBadgeInfo {
  label: string;
  shortLabel: string;
  tone: 'sage' | 'amber' | 'muted';
  isPush: boolean;
  tooltip: string;
  hasExplicitTransport: boolean;
}

export function formatTransportBadge(snapshot: LiveSnapshot | null): TransportBadgeInfo {
  if (!snapshot) {
    return {
      label: 'Offline',
      shortLabel: 'Offline',
      tone: 'muted',
      isPush: false,
      tooltip: 'Realtime transport is offline',
      hasExplicitTransport: false,
    };
  }

  const transport = snapshot.transport?.trim();
  switch (transport) {
    case 'dealer_push':
      return {
        label: 'Realtime stream',
        shortLabel: 'Realtime',
        tone: 'sage',
        isPush: true,
        tooltip: 'Sub-second Spotify Dealer WebSocket push (with automatic fallback)',
        hasExplicitTransport: true,
      };
    case 'mercury_push':
      return {
        label: 'Realtime stream',
        shortLabel: 'Realtime',
        tone: 'sage',
        isPush: true,
        tooltip: 'Spotify Mercury pub/sub push (with automatic HTTP polling fallback)',
        hasExplicitTransport: true,
      };
    case 'http_polling': {
      const hasInterval = snapshot.poll_interval_secs > 0;
      return {
        label: hasInterval
          ? `Polling every ${snapshot.poll_interval_secs}s`
          : 'Polling fallback',
        shortLabel: hasInterval ? `${snapshot.poll_interval_secs}s poll` : 'Polling',
        tone: 'amber',
        isPush: false,
        tooltip: 'Automatic HTTP polling fallback',
        hasExplicitTransport: true,
      };
    }
    case 'offline':
      return {
        label: 'Offline',
        shortLabel: 'Offline',
        tone: 'muted',
        isPush: false,
        tooltip: 'Realtime transport is offline',
        hasExplicitTransport: false,
      };
    default:
      if (transport) {
        const normalized = transport.replace(/_/g, ' ');
        const formatted = normalized.charAt(0).toUpperCase() + normalized.slice(1);
        return {
          label: formatted,
          shortLabel: formatted,
          tone: 'muted',
          isPush: false,
          tooltip: `Realtime transport: ${transport}`,
          hasExplicitTransport: true,
        };
      }
      if (snapshot.poll_interval_secs) {
        return {
          label: `Live stream (${snapshot.poll_interval_secs}s)`,
          shortLabel: `${snapshot.poll_interval_secs}s stream`,
          tone: 'muted',
          isPush: false,
          tooltip: 'Server-Sent Events live stream',
          hasExplicitTransport: false,
        };
      }
      return {
        label: 'Offline',
        shortLabel: 'Offline',
        tone: 'muted',
        isPush: false,
        tooltip: 'Realtime transport is offline',
        hasExplicitTransport: false,
      };
  }
}

function formatContextTag(item: LiveFriendItem): string {
  const base =
    item.context_name ||
    (item.context_type ? formatContextLabel(item.context_type) : null);
  if (!base) return item.context_is_private ? 'Private' : 'Direct play';
  return base;
}

function getStatusBanner(snapshot: LiveSnapshot | null, loading: boolean) {
  if (loading && !snapshot) {
    return {
      tone: 'muted' as const,
      kind: 'connecting' as const,
      label: 'Connecting',
      text: 'Opening realtime stream to local Spotify Friend Activity daemon...',
    };
  }
  if (!snapshot) {
    return {
      tone: 'muted' as const,
      kind: 'offline' as const,
      label: 'Live feed offline',
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
        kind: 'connecting' as const,
        label: 'Connecting',
        text: snapshot.message || 'Connecting to Spotify via librespot...',
      };
    case 'unauthenticated':
      return {
        tone: 'amber' as const,
        kind: 'auth' as const,
        label: 'Spotify not connected',
        text: snapshot.message || 'Connect your Spotify account to enable live friend activity.',
      };
    case 'error':
      return {
        tone: 'rust' as const,
        kind: 'error' as const,
        label: 'Stream error',
        text: snapshot.message || 'Live feed encountered an upstream error. Retrying automatically.',
      };
    default: {
      const statusLabel = snapshot.status
        ? snapshot.status.charAt(0).toUpperCase() + snapshot.status.slice(1)
        : 'Offline';
      return {
        tone: 'muted' as const,
        kind: 'offline' as const,
        label: statusLabel,
        text: snapshot.message || 'Live polling is currently paused or offline.',
      };
    }
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
  const isLive = Boolean(snapshot?.connected && snapshot?.status === 'live');
  const transport = formatTransportBadge(snapshot);
  const lastSyncLabel = snapshot?.updated_at_ms
    ? formatRelativeTime(snapshot.updated_at_ms)
    : 'Awaiting sync';

  const resolveAvatar = (friend: LiveFriendItem) =>
    friend.user_image_url ?? lookupAvatar(avatarMap, friend.user_uri, friend.user_name);

  const renderContextButton = (friend: LiveFriendItem, compact = false) => {
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
        {friend.context_is_private && (
          <Lock className="h-3 w-3 shrink-0 text-[var(--warning)]/85" />
        )}
        <span className="truncate">{label}</span>
      </>
    );

    const sharedClasses = cn(
      'inline-flex items-center gap-1.5 rounded-[6px] px-1.5 py-0.5 text-[11.5px] font-medium transition-colors',
      compact ? 'max-w-[170px]' : 'max-w-[240px]',
      'text-[var(--ink-secondary)]',
    );

    if (canOpenPlaylist && friend.context_uri) {
      return (
        <InfoTooltip content="Open playlist in Playlists tab" side="top">
          <button
            type="button"
            onClick={() => onSelectPlaylist?.(friend.context_uri!)}
            className={cn(
              sharedClasses,
              'tactile-press -ml-1.5 cursor-pointer hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
            )}
          >
            {inner}
          </button>
        </InfoTooltip>
      );
    }

    return <span className={cn(sharedClasses, '-ml-1.5')}>{inner}</span>;
  };

  const renderConnectCta = (size: 'sm' | 'md') => (
    <Button
      type="button"
      size={size}
      onClick={() => void handleConnectSpotify()}
      disabled={loggingIn}
      className="border-transparent bg-[#1db954] font-medium text-[#051209] hover:bg-[#1ed760]"
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
  );

  const renderBannerIcon = (kind: 'connecting' | 'auth' | 'error' | 'offline') => {
    switch (kind) {
      case 'connecting':
        return <Loader2 className="h-4 w-4 shrink-0 animate-spin text-[var(--warning)]" />;
      case 'error':
        return <AlertCircle className="h-4 w-4 shrink-0 text-[var(--danger)]" />;
      case 'auth':
        return <LogIn className="h-4 w-4 shrink-0 text-[var(--ink-secondary)]" />;
      default:
        return <WifiOff className="h-4 w-4 shrink-0 text-[var(--ink-muted)]" />;
    }
  };

  if (mode === 'compact') {
    return (
      <section className="overflow-hidden rounded-[var(--radius-card)] border border-[var(--border-subtle)] bg-[var(--bg-surface)]">
        {/* Compact Header */}
        <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-2.5">
          <div className="flex min-w-0 flex-wrap items-center gap-2 text-xs">
            <PixelEq active={spinningFriends.length > 0} />
            <span className="font-medium text-[var(--ink-primary)]">
              {spinningFriends.length > 0
                ? `${spinningFriends.length} listening now`
                : banner
                  ? banner.label
                  : 'No one listening'}
            </span>
            {hasFilter && hiddenCount > 0 && (
              <span className="text-[var(--ink-muted)] tabular-nums">
                &middot; Filtered ({visibleFriends.length}/{allFriends.length})
              </span>
            )}
            {snapshot?.updated_at_ms && (
              <InfoTooltip content={transport.tooltip} side="bottom">
                <span className="hidden cursor-default text-[var(--ink-muted)] sm:inline">
                  &middot; {isLive ? transport.shortLabel : 'Offline'} &middot;{' '}
                  <span className="font-mono text-[11px] tabular-nums">{lastSyncLabel}</span>
                </span>
              </InfoTooltip>
            )}
          </div>

          <div className="flex items-center gap-1.5">
            <Button
              variant="ghost"
              size="sm"
              onClick={pollNow}
              disabled={polling}
              className="h-7 px-2 text-[11.5px]"
            >
              <RefreshCw className={cn('h-3 w-3', polling && 'animate-spin')} />
              <span>{isLive && transport.isPush ? 'Sync' : 'Refresh'}</span>
            </Button>
            {onOpenFull && (
              <Button
                variant="ghost"
                size="sm"
                onClick={onOpenFull}
                className="h-7 px-2 text-[11.5px] text-[var(--ink-primary)]"
              >
                <span>View all live</span>
                <ArrowRight className="h-3 w-3 text-[var(--ink-muted)]" />
              </Button>
            )}
            <button
              type="button"
              onClick={() => setCollapsed((c) => !c)}
              className="tactile-press inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-[var(--radius-control)] text-[var(--ink-muted)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]"
              title={collapsed ? 'Expand Live Now' : 'Collapse Live Now'}
              aria-label={collapsed ? 'Expand Live Now' : 'Collapse Live Now'}
              aria-expanded={!collapsed}
            >
              <motion.span
                animate={{ rotate: collapsed ? -90 : 0 }}
                transition={SPRING_SMOOTH}
                className="inline-flex"
              >
                <ChevronDown className="h-3.5 w-3.5" />
              </motion.span>
            </button>
          </div>
        </div>

        <AnimatePresence initial={false}>
          {!collapsed && (
            <motion.div
              key="compact-live-body"
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: 'auto', opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.22, ease: EASE_OUT_EXPO }}
              className="overflow-hidden border-t border-[var(--border-subtle)]"
            >
              <div className="p-3.5">
                {banner && (
                  <div className="flex flex-wrap items-center justify-between gap-3 rounded-[var(--radius-control)] bg-[var(--bg-inset)] px-3.5 py-2.5 text-xs">
                    <div className="flex min-w-0 items-center gap-2.5">
                      {renderBannerIcon(banner.kind)}
                      <div className="min-w-0">
                        <span className="font-medium text-[var(--ink-primary)]">
                          {banner.label}
                        </span>
                        <span className="mx-1.5 text-[var(--ink-muted)]">&middot;</span>
                        <span className="text-[var(--ink-secondary)]">{banner.text}</span>
                      </div>
                    </div>
                    {showConnectCta && renderConnectCta('sm')}
                  </div>
                )}

                {!banner && spinningFriends.length === 0 && (
                  <div className="flex flex-wrap items-center justify-between gap-2 px-0.5 py-1 text-xs text-[var(--ink-muted)]">
                    <span>
                      {hasFilter && totalSpinningCount > 0
                        ? `None of the selected listeners are playing right now (${totalSpinningCount} other ${totalSpinningCount === 1 ? 'listener is' : 'listeners are'} active).`
                        : 'No listeners are actively playing right now.'}
                    </span>
                    {recentFriends.length > 0 && onOpenFull && (
                      <button
                        type="button"
                        onClick={onOpenFull}
                        className="tactile-press inline-flex cursor-pointer items-center gap-1 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:text-[var(--ink-primary)]"
                      >
                        <span>View {recentFriends.length} recently played</span>
                        <ArrowRight className="h-3 w-3" />
                      </button>
                    )}
                  </div>
                )}

                {spinningFriends.length > 0 && (
                  <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
                    {spinningFriends.map((friend, idx) => {
                      const hasDuration = friend.duration_ms != null && friend.duration_ms > 0;
                      const elapsed = hasDuration
                        ? Math.min(Math.max(0, now - friend.timestamp_ms), friend.duration_ms!)
                        : null;
                      const pct =
                        elapsed != null && friend.duration_ms
                          ? Math.min(100, (elapsed / friend.duration_ms) * 100)
                          : 0;

                      return (
                        <StaggerItem key={friend.user_uri} index={idx}>
                          <div className="group flex items-center gap-3 rounded-[9px] border border-[var(--border-subtle)] bg-[var(--bg-inset)]/75 p-2.5 transition-colors duration-150 hover:border-[var(--border-strong)] hover:bg-[var(--bg-elevated)]/65">
                            <CoverArt
                              src={friend.cover_url}
                              alt={friend.track_name}
                              size="sm"
                              hoverZoom
                            />
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center justify-between gap-2">
                                <button
                                  type="button"
                                  onClick={() => onFilterByUser?.(friend.user_uri)}
                                  className="tactile-press flex min-w-0 cursor-pointer items-center gap-1.5 text-left"
                                  title={`Filter analytics by ${friend.user_name}`}
                                >
                                  <UserAvatar
                                    name={friend.user_name}
                                    imageUrl={resolveAvatar(friend)}
                                    size="xs"
                                    isLive
                                    isSelf={friend.is_self}
                                  />
                                  <span className="truncate text-[11.5px] font-medium text-[var(--ink-secondary)] transition-colors hover:text-[var(--ink-primary)]">
                                    {friend.user_name}
                                    {friend.is_self ? ' · You' : ''}
                                  </span>
                                </button>
                                <PixelEq active className="shrink-0" />
                              </div>

                              <a
                                href={friend.spotify_url}
                                target="_blank"
                                rel="noreferrer"
                                className="mt-0.5 block truncate text-[13px] font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-ember)]"
                              >
                                {friend.track_name}
                              </a>
                              <div className="truncate text-[11.5px] text-[var(--ink-muted)]">
                                {friend.all_artists}
                              </div>

                              {hasDuration && (
                                <div className="mt-2 h-[2px] w-full overflow-hidden rounded-full bg-white/[0.08]">
                                  <div
                                    className="h-full rounded-full bg-[var(--accent-ember)] transition-[width] duration-1000 ease-linear"
                                    style={{ width: `${pct}%` }}
                                  />
                                </div>
                              )}
                            </div>
                          </div>
                        </StaggerItem>
                      );
                    })}
                  </div>
                )}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </section>
    );
  }

  // FULL LIVE NOW VIEW
  return (
    <div className="space-y-7">
      {/* Calm Header */}
      <div className="flex flex-wrap items-end justify-between gap-4 border-b border-[var(--border-subtle)] pb-4">
        <div className="space-y-1">
          <h2 className="font-display text-[22px] font-semibold leading-tight tracking-[-0.025em] text-[var(--ink-primary)] sm:text-[24px]">
            Live Now
          </h2>
          <div className="flex flex-wrap items-center gap-2 text-xs text-[var(--ink-secondary)]">
            <span className="inline-flex items-center gap-2">
              <PixelEq active={spinningFriends.length > 0} />
              <span className="tabular-nums">
                {spinningFriends.length}{' '}
                {spinningFriends.length === 1 ? 'listener' : 'listeners'} active
              </span>
            </span>
            {hasFilter && hiddenCount > 0 && (
              <span className="text-[var(--ink-muted)] tabular-nums">
                &middot; Filtered ({visibleFriends.length} of {allFriends.length})
              </span>
            )}
            <InfoTooltip content={transport.tooltip} side="bottom">
              <span className="cursor-default text-[var(--ink-muted)]">
                &middot;{' '}
                <span
                  className={cn(
                    isLive && transport.tone === 'amber' && 'text-[var(--warning)]',
                  )}
                >
                  {isLive ? transport.label : 'Offline'}
                </span>
                {snapshot?.updated_at_ms && (
                  <>
                    {' '}
                    &middot; Synced{' '}
                    <span className="font-mono text-[11px] tabular-nums">
                      {lastSyncLabel}
                    </span>
                  </>
                )}
              </span>
            </InfoTooltip>
          </div>
        </div>

        <Button
          variant="secondary"
          size="md"
          onClick={pollNow}
          disabled={polling}
        >
          <RefreshCw className={cn('h-3.5 w-3.5', polling && 'animate-spin')} />
          <span>{isLive && transport.isPush ? 'Sync' : 'Refresh'}</span>
        </Button>
      </div>

      {/* Unauthenticated / Error Banner */}
      {banner && (
        <motion.div
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2, ease: EASE_OUT_EXPO }}
          className="flex flex-wrap items-center justify-between gap-4 rounded-[var(--radius-card)] border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-4 py-3.5"
        >
          <div className="flex min-w-0 items-start gap-3">
            <div className="mt-0.5">{renderBannerIcon(banner.kind)}</div>
            <div className="min-w-0 space-y-0.5">
              <div className="text-[13px] font-medium text-[var(--ink-primary)]">
                {banner.label}
              </div>
              <p className="text-xs text-[var(--ink-secondary)]">{banner.text}</p>
            </div>
          </div>
          {showConnectCta ? (
            renderConnectCta('md')
          ) : (
            <Button variant="secondary" size="sm" onClick={pollNow} disabled={polling}>
              Retry
            </Button>
          )}
        </motion.div>
      )}

      {/* Active Now Spotlight Grid */}
      <section className="space-y-3.5">
        <div className="flex items-baseline justify-between gap-2">
          <div className="flex items-baseline gap-2">
            <h3 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
              Active now
            </h3>
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {spinningFriends.length}
            </span>
          </div>
        </div>

        {spinningFriends.length === 0 ? (
          <div className="rounded-[var(--radius-card)] border border-dashed border-[var(--border-subtle)] bg-[var(--bg-surface)]/45 px-6 py-12 text-center">
            <div className="mx-auto mb-3 flex justify-center">
              <PixelEq active={false} />
            </div>
            <p className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
              {hasFilter && totalSpinningCount > 0
                ? 'No selected listeners active'
                : 'No active playback right now'}
            </p>
            <p className="mx-auto mt-1 max-w-sm text-xs leading-relaxed text-[var(--ink-muted)]">
              {hasFilter && totalSpinningCount > 0
                ? `${totalSpinningCount} other ${totalSpinningCount === 1 ? 'listener is' : 'listeners are'} playing outside the active filter.`
                : 'Active listening sessions will appear here automatically as friends start playing.'}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            {spinningFriends.map((friend, idx) => {
              const hasDuration = friend.duration_ms != null && friend.duration_ms > 0;
              const elapsed = hasDuration
                ? Math.min(Math.max(0, now - friend.timestamp_ms), friend.duration_ms!)
                : null;
              const pct =
                elapsed != null && friend.duration_ms
                  ? Math.min(100, (elapsed / friend.duration_ms) * 100)
                  : 0;

              const releaseYear = friend.release_date?.slice(0, 4);

              return (
                <motion.article
                  key={friend.user_uri}
                  layout
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{
                    duration: 0.24,
                    delay: Math.min(idx * 0.03, 0.18),
                    ease: EASE_OUT_EXPO,
                  }}
                  whileHover={{ y: -2, transition: { duration: 0.16, ease: EASE_OUT_EXPO } }}
                  className="group relative overflow-hidden rounded-[var(--radius-card)] border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 transition-colors duration-150 hover:border-[var(--border-strong)]"
                >
                  <AmbientArtGlow imageUrl={friend.cover_url} intensity="subtle" />

                  <div className="relative z-10 flex h-full flex-col justify-between gap-4">
                    {/* Listener Bar */}
                    <div className="flex items-center justify-between gap-2">
                      <button
                        type="button"
                        onClick={() => onFilterByUser?.(friend.user_uri)}
                        className="tactile-press group/user flex min-w-0 cursor-pointer items-center gap-2 text-left"
                        title={`Filter analytics by ${friend.user_name}`}
                      >
                        <UserAvatar
                          name={friend.user_name}
                          imageUrl={resolveAvatar(friend)}
                          size="sm"
                          isLive
                          isSelf={friend.is_self}
                        />
                        <span className="truncate text-[13px] font-medium text-[var(--ink-primary)] transition-colors group-hover/user:text-[var(--accent-ember)]">
                          {friend.user_name}
                        </span>
                        {friend.is_self && (
                          <span className="shrink-0 text-xs text-[var(--ink-muted)]">
                            &middot; You
                          </span>
                        )}
                      </button>

                      <div className="flex shrink-0 items-center gap-2">
                        <PixelEq active />
                      </div>
                    </div>

                    {/* Artwork + Track Metadata */}
                    <div className="flex items-center gap-4">
                      <CoverArt
                        src={friend.cover_url}
                        alt={friend.track_name}
                        size="lg"
                        hoverZoom
                        className="shadow-[var(--shadow-art)]"
                      />

                      <div className="min-w-0 flex-1 space-y-1">
                        <a
                          href={friend.spotify_url}
                          target="_blank"
                          rel="noreferrer"
                          className="group/link inline-flex max-w-full items-center gap-1.5 text-[15px] font-semibold leading-snug tracking-[-0.015em] text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-ember)]"
                        >
                          <span className="truncate">{friend.track_name}</span>
                          <ExternalLink className="h-3.5 w-3.5 shrink-0 opacity-0 transition-opacity duration-150 group-hover:opacity-55 group-hover/link:opacity-100" />
                        </a>

                        <div className="truncate text-xs text-[var(--ink-secondary)]">
                          {friend.all_artists}
                          {friend.album_name && (
                            <span className="text-[var(--ink-muted)]">
                              {' '}
                              &middot; {friend.album_name}
                              {releaseYear ? ` (${releaseYear})` : ''}
                            </span>
                          )}
                        </div>

                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 pt-1">
                          {renderContextButton(friend)}
                          {friend.explicit && (
                            <InfoTooltip content="Explicit content" side="top">
                              <span className="cursor-default rounded bg-white/[0.06] px-1 py-px text-[10px] font-medium leading-tight text-[var(--ink-muted)]">
                                E
                              </span>
                            </InfoTooltip>
                          )}
                        </div>
                      </div>
                    </div>

                    {/* Sleek 2px Warm Ember Progress Bar */}
                    <div className="space-y-1.5 pt-0.5">
                      <div className="h-[2px] w-full overflow-hidden rounded-full bg-white/[0.08]">
                        <div
                          className="h-full rounded-full bg-[var(--accent-ember)] transition-[width] duration-1000 ease-linear"
                          style={{ width: hasDuration ? `${pct}%` : '100%' }}
                        />
                      </div>
                      <div className="flex items-center justify-between font-mono text-[11px] tabular-nums text-[var(--ink-muted)]">
                        <span>
                          {elapsed != null ? formatDuration(elapsed) : 'Live'}
                        </span>
                        <span>
                          {hasDuration ? formatDuration(friend.duration_ms) : '--:--'}
                        </span>
                      </div>
                    </div>
                  </div>
                </motion.article>
              );
            })}
          </div>
        )}
      </section>

      {/* Recently Played — Borderless Hairline List */}
      {recentFriends.length > 0 && (
        <section className="space-y-2.5 pt-1">
          <div className="flex items-baseline gap-2">
            <h3 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
              Recently played
            </h3>
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {recentFriends.length}
            </span>
          </div>

          <div className="divide-y divide-[var(--border-subtle)] border-t border-[var(--border-subtle)]">
            {recentFriends.map((friend, idx) => (
              <StaggerItem key={friend.user_uri} index={idx}>
                <div className="group flex items-center gap-3.5 py-2.5 transition-colors duration-150 hover:bg-[var(--bg-hover)] sm:px-2 sm:-mx-2 sm:rounded-[8px]">
                  <CoverArt
                    src={friend.cover_url}
                    alt={friend.track_name}
                    size="sm"
                    hoverZoom
                  />

                  <div className="min-w-0 flex-1">
                    <a
                      href={friend.spotify_url}
                      target="_blank"
                      rel="noreferrer"
                      className="block truncate text-[13px] font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-ember)]"
                    >
                      {friend.track_name}
                    </a>
                    <div className="truncate text-xs text-[var(--ink-secondary)]">
                      {friend.all_artists}
                    </div>
                  </div>

                  {/* Listener */}
                  <button
                    type="button"
                    onClick={() => onFilterByUser?.(friend.user_uri)}
                    className="tactile-press hidden shrink-0 cursor-pointer items-center gap-2 text-left text-xs text-[var(--ink-secondary)] transition-colors hover:text-[var(--ink-primary)] sm:inline-flex"
                    title={`Filter analytics by ${friend.user_name}`}
                  >
                    <UserAvatar
                      name={friend.user_name}
                      imageUrl={resolveAvatar(friend)}
                      size="xs"
                      isSelf={friend.is_self}
                    />
                    <span className="max-w-[120px] truncate">
                      {friend.user_name}
                      {friend.is_self ? ' · You' : ''}
                    </span>
                  </button>

                  {/* Context */}
                  {(friend.context_name || friend.context_uri) && (
                    <div className="hidden shrink-0 md:block">
                      {renderContextButton(friend, true)}
                    </div>
                  )}

                  {/* Relative time + hover link */}
                  <div className="flex shrink-0 items-center gap-2">
                    <span className="font-mono text-[11px] tabular-nums text-[var(--ink-muted)]">
                      {formatRelativeTime(friend.timestamp_ms)}
                    </span>
                    <a
                      href={friend.spotify_url}
                      target="_blank"
                      rel="noreferrer"
                      title="Open in Spotify"
                      aria-label={`Open ${friend.track_name} in Spotify`}
                      className="inline-flex h-6 w-6 items-center justify-center rounded-[6px] text-[var(--ink-muted)] opacity-0 transition-all duration-150 hover:bg-[var(--bg-active)] hover:text-[var(--ink-primary)] group-hover:opacity-100 focus-visible:opacity-100"
                    >
                      <ExternalLink className="h-3.5 w-3.5" />
                    </a>
                  </div>
                </div>
              </StaggerItem>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
