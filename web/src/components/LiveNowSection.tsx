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
import { useCoverAccent } from '@/lib/extractColor';
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

function computeProgress(friend: LiveFriendItem, now: number) {
  const hasDuration = friend.duration_ms != null && friend.duration_ms > 0;
  const elapsed = hasDuration
    ? Math.min(Math.max(0, now - friend.timestamp_ms), friend.duration_ms!)
    : null;
  const pct =
    elapsed != null && friend.duration_ms
      ? Math.min(100, (elapsed / friend.duration_ms) * 100)
      : 0;
  return { hasDuration, elapsed, pct };
}

interface CompactLiveTrackProps {
  friend: LiveFriendItem;
  avatarUrl: string | null;
  now: number;
  index: number;
  onFilterByUser?: (userUri: string) => void;
}

function CompactLiveTrack({
  friend,
  avatarUrl,
  now,
  index,
  onFilterByUser,
}: CompactLiveTrackProps) {
  const palette = useCoverAccent(friend.cover_url, friend.track_uri || friend.track_name);
  const { hasDuration, pct } = computeProgress(friend, now);

  return (
    <StaggerItem index={index}>
      <div className="group relative flex items-center gap-3.5 overflow-hidden rounded-[10px] bg-white/[0.025] p-2.5 ring-1 ring-inset ring-white/[0.06] transition-colors duration-150 hover:bg-white/[0.055] hover:ring-white/[0.11]">
        {/* Crisp 56x56 album art as the clear visual anchor */}
        <CoverArt
          src={friend.cover_url}
          alt={friend.track_name}
          size="md"
          className="h-14 w-14 shrink-0 rounded-[7px] shadow-[0_6px_18px_rgba(0,0,0,0.45)]"
          hoverZoom
        />

        <div className="flex min-w-0 flex-1 flex-col justify-between self-stretch py-0.5">
          <div>
            <div className="flex items-center justify-between gap-2">
              <a
                href={friend.spotify_url}
                target="_blank"
                rel="noreferrer"
                className="block min-w-0 truncate text-[13.5px] font-semibold leading-tight text-white transition-colors hover:underline"
              >
                {friend.track_name}
              </a>
              <PixelEq active color={palette.accent} className="h-2.5 shrink-0" />
            </div>

            <div className="mt-0.5 truncate text-[12px] leading-snug text-[var(--ink-secondary)]">
              {friend.all_artists}
            </div>
          </div>

          <div className="mt-1.5 flex items-center justify-between gap-2">
            <button
              type="button"
              onClick={() => onFilterByUser?.(friend.user_uri)}
              className="tactile-press flex min-w-0 cursor-pointer items-center gap-1.5 text-left"
              title={`Filter analytics by ${friend.user_name}`}
            >
              <UserAvatar
                name={friend.user_name}
                imageUrl={avatarUrl}
                size="xs"
                isLive={false}
                className="h-4 w-4 opacity-90"
              />
              <span className="truncate text-[11px] font-medium text-[var(--ink-muted)] transition-colors hover:text-white">
                {friend.user_name}
                {friend.is_self ? ' · You' : ''}
              </span>
            </button>
          </div>
        </div>

        {/* Flush bottom hairline progress bar tinted with the track's cover accent */}
        {hasDuration && (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 h-[2px] bg-white/[0.06]">
            <div
              className="h-full transition-[width] duration-1000 ease-linear"
              style={{
                width: `${pct}%`,
                backgroundColor: palette.accent,
                boxShadow: `0 0 10px ${palette.soft}`,
              }}
            />
          </div>
        )}
      </div>
    </StaggerItem>
  );
}

interface LiveTrackRowProps {
  friend: LiveFriendItem;
  avatarUrl: string | null;
  now: number;
  index: number;
  onSpotlight: (userUri: string) => void;
  onFilterByUser?: (userUri: string) => void;
  onSelectPlaylist?: (contextUri: string) => void;
}

function LiveTrackRow({
  friend,
  avatarUrl,
  now,
  index,
  onSpotlight,
  onFilterByUser,
  onSelectPlaylist,
}: LiveTrackRowProps) {
  const rowPalette = useCoverAccent(friend.cover_url, friend.track_uri || friend.track_name);
  const { hasDuration, elapsed, pct } = computeProgress(friend, now);
  const contextLabel = formatContextTag(friend);
  const canOpenPlaylist = Boolean(friend.context_uri && onSelectPlaylist);

  return (
    <StaggerItem index={index}>
      <div
        role="button"
        tabIndex={0}
        onClick={() => onSpotlight(friend.user_uri)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            onSpotlight(friend.user_uri);
          }
        }}
        title={`Spotlight ${friend.user_name} · ${friend.track_name}`}
        className="tactile-press group -mx-2.5 flex cursor-pointer items-center gap-3.5 rounded-[8px] px-2.5 py-2.5 transition-colors duration-150 hover:bg-white/[0.055]"
      >
        <CoverArt
          src={friend.cover_url}
          alt={friend.track_name}
          size="sm"
          hoverZoom
        />

        {/* Track + Artist */}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="truncate text-[13.5px] font-medium text-white">
              {friend.track_name}
            </span>
            {friend.explicit && (
              <InfoTooltip content="Explicit content" side="top">
                <span className="shrink-0 cursor-default rounded bg-white/[0.08] px-1 py-px text-[9.5px] font-medium leading-tight text-[var(--ink-secondary)]">
                  E
                </span>
              </InfoTooltip>
            )}
          </div>
          <div className="truncate text-xs text-[var(--ink-secondary)]">
            {friend.all_artists}
            {friend.album_name && (
              <span className="hidden text-[var(--ink-muted)] xl:inline">
                {' '}
                &middot; {friend.album_name}
              </span>
            )}
          </div>
        </div>

        {/* Listener with dynamic live ring */}
        <div className="hidden w-36 shrink-0 sm:flex sm:items-center">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onFilterByUser?.(friend.user_uri);
            }}
            className="tactile-press inline-flex max-w-full cursor-pointer items-center gap-2 text-left text-xs text-[var(--ink-secondary)] transition-colors hover:text-white"
            title={`Filter analytics by ${friend.user_name}`}
          >
            <UserAvatar
              name={friend.user_name}
              imageUrl={avatarUrl}
              size="xs"
              isLive
              isSelf={friend.is_self}
              ringColor={rowPalette.accent}
            />
            <span className="truncate">
              {friend.user_name}
              {friend.is_self ? ' · You' : ''}
            </span>
          </button>
        </div>

        {/* Context */}
        <div className="hidden w-44 shrink-0 lg:block">
          {friend.context_name || friend.context_uri ? (
            canOpenPlaylist && friend.context_uri ? (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  onSelectPlaylist?.(friend.context_uri!);
                }}
                className="tactile-press -ml-1.5 inline-flex max-w-full cursor-pointer items-center gap-1.5 rounded-[6px] px-1.5 py-0.5 text-xs text-[var(--ink-muted)] transition-colors hover:bg-white/[0.07] hover:text-white"
                title={`Open ${contextLabel}`}
              >
                <PlaylistCover
                  name={contextLabel}
                  imageUrl={friend.context_image_url}
                  contextType={friend.context_type}
                  isPrivate={friend.context_is_private}
                  size="xs"
                />
                {friend.context_is_private && (
                  <Lock className="h-3 w-3 shrink-0 text-[var(--warning)]/85" />
                )}
                <span className="truncate">{contextLabel}</span>
              </button>
            ) : (
              <span className="inline-flex max-w-full items-center gap-1.5 text-xs text-[var(--ink-muted)]">
                <PlaylistCover
                  name={contextLabel}
                  imageUrl={friend.context_image_url}
                  contextType={friend.context_type}
                  isPrivate={friend.context_is_private}
                  size="xs"
                />
                {friend.context_is_private && (
                  <Lock className="h-3 w-3 shrink-0 text-[var(--warning)]/85" />
                )}
                <span className="truncate">{contextLabel}</span>
              </span>
            )
          ) : (
            <span className="text-xs text-[var(--ink-muted)]/50">&mdash;</span>
          )}
        </div>

        {/* Inline Progress Bar in row's dynamic cover accent */}
        <div className="hidden w-32 shrink-0 flex-col gap-1 md:flex">
          <div className="h-[2.5px] w-full overflow-hidden rounded-full bg-white/[0.08]">
            <div
              className="h-full rounded-full transition-[width] duration-1000 ease-linear"
              style={{
                width: hasDuration ? `${pct}%` : '100%',
                backgroundColor: rowPalette.accent,
                boxShadow: `0 0 12px ${rowPalette.soft}`,
              }}
            />
          </div>
          <div className="flex items-center justify-between font-mono text-[10.5px] tabular-nums text-[var(--ink-muted)]">
            <span>{elapsed != null ? formatDuration(elapsed) : 'Live'}</span>
            <span>{hasDuration ? formatDuration(friend.duration_ms) : '--:--'}</span>
          </div>
        </div>

        {/* Dancing EQ + Hover Spotify Link */}
        <div className="flex shrink-0 items-center gap-2.5">
          <PixelEq active color={rowPalette.accent} />
          <a
            href={friend.spotify_url}
            target="_blank"
            rel="noreferrer"
            onClick={(e) => e.stopPropagation()}
            title="Open in Spotify"
            aria-label={`Open ${friend.track_name} in Spotify`}
            className="inline-flex h-7 w-7 items-center justify-center rounded-[6px] text-[var(--ink-muted)] opacity-0 transition-all duration-150 hover:bg-white/[0.1] hover:text-white group-hover:opacity-100 focus-visible:opacity-100"
          >
            <ExternalLink className="h-3.5 w-3.5" />
          </a>
        </div>
      </div>
    </StaggerItem>
  );
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
  const [spotlightUserUri, setSpotlightUserUri] = useState<string | null>(null);

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

  const spotlightFriend =
    (spotlightUserUri
      ? spinningFriends.find((f) => f.user_uri === spotlightUserUri)
      : null) ??
    spinningFriends[0] ??
    null;

  const otherSpinningFriends = spotlightFriend
    ? spinningFriends.filter((f) => f.user_uri !== spotlightFriend.user_uri)
    : [];

  const spotlightPalette = useCoverAccent(
    spotlightFriend?.cover_url,
    spotlightFriend?.track_uri || spotlightFriend?.track_name,
  );

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
      'inline-flex items-center gap-1.5 rounded-[6px] px-1.5 py-0.5 text-[12px] font-medium transition-colors',
      compact ? 'max-w-[175px]' : 'max-w-[280px]',
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
              'tactile-press -ml-1.5 cursor-pointer hover:bg-white/[0.08] hover:text-white',
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

  // COMPACT MODE (embedded full-width band inside Overview)
  if (mode === 'compact') {
    return (
      <section
        className={cn(
          'w-full px-6 lg:px-8',
          collapsed ? 'pb-4' : 'pb-6',
        )}
      >
        {/* Compact Header */}
        <div className="flex flex-wrap items-center justify-between gap-2 pb-2.5">
          <div className="flex min-w-0 flex-wrap items-center gap-2 text-xs">
            <PixelEq
              active={spinningFriends.length > 0}
              color={spinningFriends.length > 0 ? spotlightPalette.accent : undefined}
            />
            <span className="font-semibold text-white">
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
                className="h-7 px-2 text-[11.5px] text-white"
              >
                <span>View all live</span>
                <ArrowRight className="h-3 w-3 text-[var(--ink-muted)]" />
              </Button>
            )}
            <button
              type="button"
              onClick={() => setCollapsed((c) => !c)}
              className="tactile-press inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-[var(--radius-control)] text-[var(--ink-muted)] transition-colors hover:bg-white/[0.06] hover:text-white"
              title={collapsed ? 'Expand Listening now' : 'Collapse Listening now'}
              aria-label={collapsed ? 'Expand Listening now' : 'Collapse Listening now'}
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
              className="overflow-hidden"
            >
              <div className="pt-2">
                {banner && (
                  <div className="flex flex-wrap items-center justify-between gap-3 rounded-[8px] border border-white/[0.07] bg-white/[0.025] px-3.5 py-2.5 text-xs">
                    <div className="flex min-w-0 items-center gap-2.5">
                      {renderBannerIcon(banner.kind)}
                      <div className="min-w-0">
                        <span className="font-medium text-white">{banner.label}</span>
                        <span className="mx-1.5 text-[var(--ink-muted)]">&middot;</span>
                        <span className="text-[var(--ink-secondary)]">{banner.text}</span>
                      </div>
                    </div>
                    {showConnectCta && renderConnectCta('sm')}
                  </div>
                )}

                {!banner && spinningFriends.length === 0 && (
                  <div className="flex flex-wrap items-center justify-between gap-2 py-1 text-xs text-[var(--ink-muted)]">
                    <span>
                      {hasFilter && totalSpinningCount > 0
                        ? `None of the selected listeners are playing right now (${totalSpinningCount} other ${totalSpinningCount === 1 ? 'listener is' : 'listeners are'} active).`
                        : 'No listeners are actively playing right now.'}
                    </span>
                    {recentFriends.length > 0 && onOpenFull && (
                      <button
                        type="button"
                        onClick={onOpenFull}
                        className="tactile-press inline-flex cursor-pointer items-center gap-1 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:text-white"
                      >
                        <span>View {recentFriends.length} recently played</span>
                        <ArrowRight className="h-3 w-3" />
                      </button>
                    )}
                  </div>
                )}

                {spinningFriends.length > 0 && (
                  <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                    {spinningFriends.map((friend, idx) => (
                      <CompactLiveTrack
                        key={friend.user_uri}
                        friend={friend}
                        avatarUrl={resolveAvatar(friend)}
                        now={now}
                        index={idx}
                        onFilterByUser={onFilterByUser}
                      />
                    ))}
                  </div>
                )}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </section>
    );
  }

  // FULL-BLEED LIVE NOW VIEW
  const spotlightProgress = spotlightFriend ? computeProgress(spotlightFriend, now) : null;
  const spotlightReleaseYear = spotlightFriend?.release_date?.slice(0, 4);

  return (
    <div className="w-full bg-[#09090b]">
      {/* Full-Bleed Top Hero or Calm Header */}
      {spotlightFriend ? (
        <section className="relative w-full overflow-hidden px-6 pt-6 pb-8 lg:px-8">
          <AmbientArtGlow palette={spotlightPalette} intensity="vivid" />

          <div className="relative z-10 space-y-6">
            {/* Top Utility Strip inside Hero */}
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                <h1 className="font-display text-2xl font-bold tracking-[-0.03em] text-white sm:text-[26px]">
                  Listening now
                </h1>
                <div className="flex flex-wrap items-center gap-2 text-xs text-white/75">
                  <span className="inline-flex items-center gap-1.5 font-medium text-white tabular-nums">
                    <PixelEq active color={spotlightPalette.accent} />
                    <span>
                      {spinningFriends.length}{' '}
                      {spinningFriends.length === 1 ? 'active' : 'active'}
                    </span>
                  </span>
                  {hasFilter && hiddenCount > 0 && (
                    <span className="text-white/60 tabular-nums">
                      &middot; Filtered ({visibleFriends.length} of {allFriends.length})
                    </span>
                  )}
                  <InfoTooltip content={transport.tooltip} side="bottom">
                    <span className="cursor-default text-white/60">
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
                          &middot;{' '}
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
                size="sm"
                onClick={pollNow}
                disabled={polling}
                className="border-white/15 bg-black/30 text-white backdrop-blur-md hover:bg-black/45"
              >
                <RefreshCw className={cn('h-3.5 w-3.5', polling && 'animate-spin')} />
                <span>{isLive && transport.isPush ? 'Sync' : 'Refresh'}</span>
              </Button>
            </div>

            {/* Spotlight Content */}
            <AnimatePresence mode="wait">
              <motion.div
                key={`${spotlightFriend.user_uri}:${spotlightFriend.track_uri}`}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={{ duration: 0.22, ease: EASE_OUT_EXPO }}
                className="group flex flex-col gap-6 sm:flex-row sm:items-end"
              >
                <CoverArt
                  src={spotlightFriend.cover_url}
                  alt={spotlightFriend.track_name}
                  size="xl"
                  className="h-36 w-36 shrink-0 shadow-[var(--shadow-art)]"
                  hoverZoom
                />

                <div className="min-w-0 flex-1 space-y-3">
                  {/* Listener row */}
                  <div className="flex flex-wrap items-center gap-2.5">
                    <button
                      type="button"
                      onClick={() => onFilterByUser?.(spotlightFriend.user_uri)}
                      className="tactile-press inline-flex cursor-pointer items-center gap-2 text-left"
                      title={`Filter analytics by ${spotlightFriend.user_name}`}
                    >
                      <UserAvatar
                        name={spotlightFriend.user_name}
                        imageUrl={resolveAvatar(spotlightFriend)}
                        size="sm"
                        isLive
                        isSelf={spotlightFriend.is_self}
                        ringColor={spotlightPalette.accent}
                      />
                      <span className="text-[13.5px] font-semibold text-white transition-opacity hover:opacity-85">
                        {spotlightFriend.user_name}
                      </span>
                      {spotlightFriend.is_self && (
                        <span className="text-xs text-white/60">&middot; You</span>
                      )}
                    </button>
                    <PixelEq active color={spotlightPalette.accent} />
                  </div>

                  {/* Huge Crisp Track Title + Artist & Album */}
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <a
                        href={spotlightFriend.spotify_url}
                        target="_blank"
                        rel="noreferrer"
                        className="group/title inline-flex max-w-full items-center gap-2 text-2xl font-bold leading-tight tracking-[-0.03em] text-white transition-opacity hover:opacity-90 sm:text-3xl"
                      >
                        <span className="truncate">{spotlightFriend.track_name}</span>
                        <ExternalLink className="h-4 w-4 shrink-0 opacity-0 transition-opacity duration-150 group-hover/title:opacity-85" />
                      </a>
                      {spotlightFriend.explicit && (
                        <InfoTooltip content="Explicit content" side="top">
                          <span className="shrink-0 cursor-default rounded bg-white/15 px-1.5 py-0.5 text-[10px] font-semibold leading-tight text-white/85">
                            E
                          </span>
                        </InfoTooltip>
                      )}
                    </div>

                    <div className="truncate text-sm text-[var(--ink-secondary)]">
                      <span className="font-medium text-white/90">
                        {spotlightFriend.all_artists}
                      </span>
                      {spotlightFriend.album_name && (
                        <span className="text-white/60">
                          {' '}
                          &middot; {spotlightFriend.album_name}
                          {spotlightReleaseYear ? ` (${spotlightReleaseYear})` : ''}
                        </span>
                      )}
                    </div>
                  </div>

                  {/* Playlist Context + Spotify Link */}
                  <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5">
                    {renderContextButton(spotlightFriend)}
                    <a
                      href={spotlightFriend.spotify_url}
                      target="_blank"
                      rel="noreferrer"
                      className="tactile-press inline-flex items-center gap-1.5 text-xs font-medium text-white/70 transition-colors hover:text-white"
                    >
                      <span>Open in Spotify</span>
                      <ExternalLink className="h-3 w-3" />
                    </a>
                  </div>

                  {/* Crisp 3px Progress Bar in Dynamic Cover Color */}
                  {spotlightProgress && (
                    <div className="max-w-2xl space-y-1.5 pt-1">
                      <div className="h-[3px] w-full overflow-hidden rounded-full bg-white/[0.12]">
                        <div
                          className="h-full rounded-full transition-[width] duration-1000 ease-linear"
                          style={{
                            width: spotlightProgress.hasDuration
                              ? `${spotlightProgress.pct}%`
                              : '100%',
                            backgroundColor: spotlightPalette.accent,
                            boxShadow: `0 0 12px ${spotlightPalette.soft}`,
                          }}
                        />
                      </div>
                      <div className="flex items-center justify-between font-mono text-[11px] tabular-nums text-white/65">
                        <span>
                          {spotlightProgress.elapsed != null
                            ? formatDuration(spotlightProgress.elapsed)
                            : 'Live'}
                        </span>
                        <span>
                          {spotlightProgress.hasDuration
                            ? formatDuration(spotlightFriend.duration_ms)
                            : '--:--'}
                        </span>
                      </div>
                    </div>
                  )}
                </div>
              </motion.div>
            </AnimatePresence>
          </div>
        </section>
      ) : (
        /* 0 Friends Playing — Clean Full-Bleed Top Header */
        <section className="w-full border-b border-white/[0.07] px-6 pt-6 pb-6 lg:px-8">
          <div className="flex flex-wrap items-end justify-between gap-4">
            <div className="space-y-1">
              <h1 className="font-display text-2xl font-bold tracking-[-0.03em] text-white sm:text-[26px]">
                Listening now
              </h1>
              <div className="flex flex-wrap items-center gap-2 text-xs text-[var(--ink-secondary)]">
                <span className="inline-flex items-center gap-2">
                  <PixelEq active={false} />
                  <span className="tabular-nums">0 listeners active</span>
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
              size="sm"
              onClick={pollNow}
              disabled={polling}
            >
              <RefreshCw className={cn('h-3.5 w-3.5', polling && 'animate-spin')} />
              <span>{isLive && transport.isPush ? 'Sync' : 'Refresh'}</span>
            </Button>
          </div>
        </section>
      )}

      {/* Unauthenticated / Error Banner (Full-Bleed Hairline Strip) */}
      {banner && (
        <motion.div
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2, ease: EASE_OUT_EXPO }}
          className="flex flex-wrap items-center justify-between gap-4 border-b border-white/[0.07] bg-white/[0.02] px-6 py-4 lg:px-8"
        >
          <div className="flex min-w-0 items-start gap-3">
            <div className="mt-0.5">{renderBannerIcon(banner.kind)}</div>
            <div className="min-w-0 space-y-0.5">
              <div className="text-[13px] font-medium text-white">{banner.label}</div>
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

      {/* Quiet Calm Empty State when 0 friends are playing */}
      {!spotlightFriend && (
        <section className="px-6 py-14 text-center lg:px-8">
          <div className="mx-auto mb-3 flex justify-center">
            <PixelEq active={false} />
          </div>
          <p className="text-[15px] font-semibold tracking-[-0.015em] text-white">
            {hasFilter && totalSpinningCount > 0
              ? 'No selected listeners active'
              : 'No active playback right now'}
          </p>
          <p className="mx-auto mt-1 max-w-sm text-xs leading-relaxed text-[var(--ink-muted)]">
            {hasFilter && totalSpinningCount > 0
              ? `${totalSpinningCount} other ${totalSpinningCount === 1 ? 'listener is' : 'listeners are'} playing outside the active filter.`
              : 'Active listening sessions will appear here automatically as friends start playing.'}
          </p>
        </section>
      )}

      {/* Other Active Listeners (if > 1 spinning) — Cardless Borderless Rows */}
      {otherSpinningFriends.length > 0 && (
        <section className="w-full px-6 pt-2 pb-6 lg:px-8">
          <div className="mb-2 flex items-baseline justify-between gap-2 border-b border-white/[0.07] pb-2.5">
            <div className="flex items-baseline gap-2">
              <h2 className="text-[14px] font-semibold tracking-[-0.015em] text-white">
                Also listening
              </h2>
              <span className="text-xs text-[var(--ink-muted)] tabular-nums">
                {otherSpinningFriends.length}
              </span>
            </div>
            <span className="text-[11.5px] text-[var(--ink-muted)]">
              Click a listener to spotlight
            </span>
          </div>

          <div className="divide-y divide-white/[0.06]">
            {otherSpinningFriends.map((friend, idx) => (
              <LiveTrackRow
                key={friend.user_uri}
                friend={friend}
                avatarUrl={resolveAvatar(friend)}
                now={now}
                index={idx}
                onSpotlight={setSpotlightUserUri}
                onFilterByUser={onFilterByUser}
                onSelectPlaylist={onSelectPlaylist}
              />
            ))}
          </div>
        </section>
      )}

      {/* Recently Played — Full-Width Cardless Hairline Rows */}
      {recentFriends.length > 0 && (
        <section className="w-full px-6 pt-2 pb-10 lg:px-8">
          <div className="mb-2 flex items-baseline gap-2 border-b border-white/[0.07] pb-2.5">
            <h2 className="text-[14px] font-semibold tracking-[-0.015em] text-white">
              Recently played
            </h2>
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {recentFriends.length}
            </span>
          </div>

          <div className="divide-y divide-white/[0.06]">
            {recentFriends.map((friend, idx) => (
              <StaggerItem key={friend.user_uri} index={idx}>
                <div className="group -mx-2.5 flex items-center gap-3.5 rounded-[8px] px-2.5 py-2.5 transition-colors duration-150 hover:bg-white/[0.055]">
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
                      className="block truncate text-[13.5px] font-medium text-white transition-colors hover:underline"
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
                    className="tactile-press hidden w-36 shrink-0 cursor-pointer items-center gap-2 text-left text-xs text-[var(--ink-secondary)] transition-colors hover:text-white sm:inline-flex"
                    title={`Filter analytics by ${friend.user_name}`}
                  >
                    <UserAvatar
                      name={friend.user_name}
                      imageUrl={resolveAvatar(friend)}
                      size="xs"
                      isSelf={friend.is_self}
                    />
                    <span className="truncate">
                      {friend.user_name}
                      {friend.is_self ? ' · You' : ''}
                    </span>
                  </button>

                  {/* Context */}
                  <div className="hidden w-44 shrink-0 md:block">
                    {friend.context_name || friend.context_uri ? (
                      renderContextButton(friend, true)
                    ) : (
                      <span className="text-xs text-[var(--ink-muted)]/50">&mdash;</span>
                    )}
                  </div>

                  {/* Relative time + hover Spotify link */}
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
                      className="inline-flex h-7 w-7 items-center justify-center rounded-[6px] text-[var(--ink-muted)] opacity-0 transition-all duration-150 hover:bg-white/[0.1] hover:text-white group-hover:opacity-100 focus-visible:opacity-100"
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
