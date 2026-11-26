import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { motion } from 'motion/react';
import {
  Download,
  ExternalLink,
  Loader2,
  Lock,
  Repeat,
  Search,
  SkipForward,
  X,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { SPRING_SNAPPY, StaggerItem } from '@/components/ui/motion';
import { CoverArt } from '@/components/CoverArt';
import { EmptyState } from '@/components/EmptyState';
import { InfoTooltip } from '@/components/InfoTooltip';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import {
  cn,
  downloadTextFile,
  escapeCsvCell,
  formatContextLabel,
  formatDuration,
  formatRelativeTime,
  spotifyUriToUrl,
} from '@/lib/utils';
import {
  buildUsersParam,
  type AvatarMap,
  type ListenRecord,
  type PaginatedHistoryResponse,
} from '@/types/api';

export interface HistoryTabProps {
  selectedUsers: string[];
  days: number | null;
  refreshTick: number;
  avatarMap: AvatarMap;
  onSelectPlaylist?: (contextUri: string) => void;
  onFilterByUser?: (userUri: string) => void;
}

type PlaybackFilter = 'all' | 'completed' | 'skipped' | 'repeats';

const PAGE_SIZE = 50;

const FILTER_OPTIONS: { value: PlaybackFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'completed', label: 'Completed' },
  { value: 'skipped', label: 'Skipped' },
  { value: 'repeats', label: 'Repeats' },
];

const HISTORY_GRID_COLS =
  'grid grid-cols-[minmax(0,1fr)_auto_28px] sm:grid-cols-[minmax(0,1fr)_136px_44px_76px_28px] md:grid-cols-[minmax(0,1fr)_136px_190px_44px_76px_28px] lg:grid-cols-[minmax(0,1fr)_150px_220px_48px_56px_76px_28px] items-center gap-3';

interface DayGroup {
  key: string;
  label: string;
  items: ListenRecord[];
}

function formatDayGroup(ms: number): { key: string; label: string } {
  const date = new Date(ms);
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const diffDays = Math.round((startOfToday - startOfDate) / 86_400_000);

  const key = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(
    date.getDate(),
  ).padStart(2, '0')}`;

  if (diffDays === 0) return { key, label: 'Today' };
  if (diffDays === 1) return { key, label: 'Yesterday' };

  const sameYear = date.getFullYear() === now.getFullYear();
  const label = date.toLocaleDateString(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    ...(sameYear ? {} : { year: 'numeric' }),
  });
  return { key, label };
}

export function HistoryTab({
  selectedUsers,
  days,
  refreshTick,
  avatarMap,
  onSelectPlaylist,
  onFilterByUser,
}: HistoryTabProps) {
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [playbackFilter, setPlaybackFilter] = useState<PlaybackFilter>('all');
  const [items, setItems] = useState<ListenRecord[]>([]);
  const [total, setTotal] = useState<number>(0);
  const [offset, setOffset] = useState<number>(0);
  const [hasMore, setHasMore] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(true);
  const [loadingMore, setLoadingMore] = useState<boolean>(false);

  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const requestIdRef = useRef<number>(0);

  const usersParam = useMemo(() => buildUsersParam(selectedUsers), [selectedUsers]);

  useEffect(() => {
    const id = window.setTimeout(() => setDebouncedQuery(query.trim()), 200);
    return () => window.clearTimeout(id);
  }, [query]);

  const fetchPage = useCallback(
    async (nextOffset: number, replace: boolean) => {
      const reqId = ++requestIdRef.current;
      if (replace) {
        setLoading(true);
      } else {
        setLoadingMore(true);
      }

      const params = new URLSearchParams();
      params.set('limit', String(PAGE_SIZE));
      params.set('offset', String(nextOffset));
      if (usersParam) params.set('users', usersParam);
      if (days != null) params.set('days', String(days));
      if (debouncedQuery) params.set('q', debouncedQuery);

      try {
        const res = await fetch(`/api/history?${params.toString()}`);
        const data: PaginatedHistoryResponse | ListenRecord[] = await res.json();
        if (reqId !== requestIdRef.current) return;

        if (Array.isArray(data)) {
          setItems((prev) => {
            if (replace) return data;
            const seen = new Set(prev.map((r) => r.id));
            return [...prev, ...data.filter((r) => !seen.has(r.id))];
          });
          setTotal((prev) => (replace ? data.length : prev + data.length));
          setOffset(nextOffset);
          setHasMore(false);
        } else {
          const pageItems = Array.isArray(data.items) ? data.items : [];
          setItems((prev) => {
            if (replace) return pageItems;
            const seen = new Set(prev.map((r) => r.id));
            return [...prev, ...pageItems.filter((r) => !seen.has(r.id))];
          });
          setTotal(typeof data.total === 'number' ? data.total : pageItems.length);
          setOffset(typeof data.offset === 'number' ? data.offset : nextOffset);
          setHasMore(Boolean(data.has_more));
        }
      } catch {
        if (reqId !== requestIdRef.current) return;
        if (replace) {
          setItems([]);
          setTotal(0);
          setHasMore(false);
        }
      } finally {
        if (reqId === requestIdRef.current) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    [usersParam, days, debouncedQuery],
  );

  // Reset and load first page whenever filters/search/refresh change
  useEffect(() => {
    setItems([]);
    setOffset(0);
    setHasMore(false);
    void fetchPage(0, true);
  }, [fetchPage, refreshTick]);

  const handleLoadMore = useCallback(() => {
    if (loading || loadingMore || !hasMore) return;
    void fetchPage(offset + PAGE_SIZE, false);
  }, [fetchPage, hasMore, loading, loadingMore, offset]);

  // Infinite scroll via IntersectionObserver
  useEffect(() => {
    const node = sentinelRef.current;
    if (!node || !hasMore) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) {
          handleLoadMore();
        }
      },
      { rootMargin: '240px' },
    );

    observer.observe(node);
    return () => observer.disconnect();
  }, [hasMore, handleLoadMore]);

  const filteredItems = useMemo(() => {
    switch (playbackFilter) {
      case 'completed':
        return items.filter((item) => !item.was_skipped);
      case 'skipped':
        return items.filter((item) => Boolean(item.was_skipped));
      case 'repeats':
        return items.filter((item) => Boolean(item.inferred_repeat));
      default:
        return items;
    }
  }, [items, playbackFilter]);

  const dayGroups = useMemo<DayGroup[]>(() => {
    const map = new Map<string, DayGroup>();
    for (const item of filteredItems) {
      const { key, label } = formatDayGroup(item.played_at_ms);
      const existing = map.get(key);
      if (existing) {
        existing.items.push(item);
      } else {
        map.set(key, { key, label, items: [item] });
      }
    }
    return Array.from(map.values());
  }, [filteredItems]);

  const handleExportCsv = useCallback(() => {
    if (filteredItems.length === 0) return;
    const headers = [
      'Played At',
      'Track',
      'Artists',
      'Album',
      'Listener',
      'Context',
      'Duration',
      'Skipped',
      'Repeat',
      'Spotify URL',
    ];
    const rows = filteredItems.map((item) => {
      const artists = item.all_artists || item.artist_name;
      const context =
        item.context_name ||
        (item.context_type ? formatContextLabel(item.context_type) : '');
      const url =
        (item.spotify_url && item.spotify_url.startsWith('https://')
          ? item.spotify_url
          : spotifyUriToUrl(item.track_uri)) ?? '';
      return [
        item.played_at_local,
        item.track_name,
        artists,
        item.album_name ?? '',
        item.user_name,
        context,
        formatDuration(item.duration_ms),
        item.was_skipped ? 'Yes' : 'No',
        item.inferred_repeat ? 'Yes' : 'No',
        url,
      ]
        .map((cell) => escapeCsvCell(String(cell)))
        .join(',');
    });
    const csv = [headers.join(','), ...rows].join('\n');
    const stamp = new Date().toISOString().slice(0, 10);
    downloadTextFile(`bside-history-${stamp}.csv`, csv, 'text/csv;charset=utf-8');
  }, [filteredItems]);

  const hasActiveLocalFilter = playbackFilter !== 'all';

  const countLabel = loading
    ? 'Loading...'
    : hasActiveLocalFilter
      ? `${filteredItems.length.toLocaleString()} of ${items.length.toLocaleString()} loaded`
      : total > 0
        ? `${total.toLocaleString()} ${total === 1 ? 'listen' : 'listens'}`
        : `${items.length.toLocaleString()} ${items.length === 1 ? 'listen' : 'listens'}`;

  return (
    <div className="w-full bg-[#09090b] px-6 py-6 lg:px-8">
      {/* Single Clean Top Row: Heading + Total Count + Export CSV */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-baseline gap-3">
          <h1 className="font-display text-2xl font-bold tracking-[-0.03em] text-white sm:text-[26px]">
            History
          </h1>
          <span className="text-xs text-[var(--ink-muted)] tabular-nums">
            {countLabel}
          </span>
        </div>

        <Button
          variant="secondary"
          size="sm"
          onClick={handleExportCsv}
          disabled={loading || filteredItems.length === 0}
        >
          <Download className="h-3.5 w-3.5" />
          <span>Export CSV</span>
        </Button>
      </div>

      {/* Inline Search + Sliding Segmented Filter */}
      <div className="mt-4 flex flex-col gap-3 border-b border-white/[0.07] pb-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-1 flex-wrap items-center gap-2.5">
          {/* Search Input */}
          <div className="relative w-full sm:w-80">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search tracks, artists, albums..."
              className="pl-8 pr-7"
            />
            {query && (
              <button
                type="button"
                onClick={() => setQuery('')}
                aria-label="Clear search"
                className="tactile-press absolute top-1/2 right-2 -translate-y-1/2 cursor-pointer text-[var(--ink-muted)] transition-colors hover:text-white"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>

          {/* Segmented Playback Filter */}
          <div
            role="radiogroup"
            aria-label="Filter listens"
            className="inline-flex items-center rounded-[var(--radius-control)] border border-white/[0.08] bg-white/[0.03] p-0.5"
          >
            {FILTER_OPTIONS.map((opt) => {
              const active = playbackFilter === opt.value;
              return (
                <button
                  key={opt.value}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  onClick={() => setPlaybackFilter(opt.value)}
                  className={cn(
                    'tactile-press relative cursor-pointer rounded-[6px] px-2.5 py-1 text-xs font-medium transition-colors',
                    active
                      ? 'text-white'
                      : 'text-[var(--ink-muted)] hover:text-[var(--ink-secondary)]',
                  )}
                >
                  {active && (
                    <motion.span
                      layoutId="history-filter-pill"
                      transition={SPRING_SNAPPY}
                      className="absolute inset-0 rounded-[6px] bg-white/[0.12] ring-1 ring-inset ring-white/[0.08]"
                    />
                  )}
                  <span className="relative z-10">{opt.label}</span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* Main Content */}
      {loading && items.length === 0 ? (
        <div className="flex items-center justify-center gap-2 py-20 text-xs text-[var(--ink-muted)]">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span>Loading listening history...</span>
        </div>
      ) : !loading && filteredItems.length === 0 ? (
        <div className="py-10">
          <EmptyState
            title={
              debouncedQuery || hasActiveLocalFilter
                ? 'No matching listens'
                : 'No history recorded yet'
            }
            description={
              debouncedQuery
                ? `No listens matched "${debouncedQuery}" in the current view.`
                : hasActiveLocalFilter
                  ? `No ${playbackFilter} listens found in the currently loaded history.`
                  : 'Run bside watch to begin logging Spotify Friend Activity.'
            }
            command={debouncedQuery || hasActiveLocalFilter ? undefined : 'bside watch'}
          />
        </div>
      ) : (
        <div className="mt-4 space-y-6">
          {/* Razor-Sharp Column Header Bar */}
          <div
            className={cn(
              HISTORY_GRID_COLS,
              'border-b border-white/[0.06] pb-2 text-[11.5px] font-medium text-[var(--ink-muted)]',
            )}
          >
            <div>Track</div>
            <div className="hidden sm:block">Listener</div>
            <div className="hidden md:block">Context</div>
            <div className="hidden text-right sm:block">Status</div>
            <div className="hidden text-right lg:block">Duration</div>
            <div className="text-right">When</div>
            <div aria-hidden="true" />
          </div>

          {dayGroups.map((group) => (
            <section key={group.key} className="space-y-1">
              {/* Calm Day Group Header */}
              <div className="flex items-center gap-3 pb-1.5">
                <h2 className="text-[13px] font-semibold tracking-[-0.01em] text-white">
                  {group.label}
                </h2>
                <div className="h-px flex-1 bg-white/[0.07]" />
                <span className="text-[11.5px] text-[var(--ink-muted)] tabular-nums">
                  {group.items.length} {group.items.length === 1 ? 'listen' : 'listens'}
                </span>
              </div>

              {/* Borderless Aligned Grid Rows directly on #09090b */}
              <div className="divide-y divide-white/[0.055]">
                {group.items.map((item, idx) => {
                  const artists = item.all_artists || item.artist_name;
                  const trackUrl =
                    (item.spotify_url && item.spotify_url.startsWith('https://')
                      ? item.spotify_url
                      : spotifyUriToUrl(item.track_uri)) ?? null;
                  const contextLabel =
                    item.context_name ||
                    (item.context_type ? formatContextLabel(item.context_type) : null);
                  const userAvatarUrl =
                    item.user_image_url ??
                    lookupAvatar(avatarMap, item.user_uri, item.user_name);
                  const hasClickableContext = Boolean(item.context_uri && onSelectPlaylist);

                  return (
                    <StaggerItem key={item.id} index={idx}>
                      <div
                        className={cn(
                          HISTORY_GRID_COLS,
                          'group -mx-2.5 rounded-[8px] px-2.5 py-2.5 transition-colors duration-150 hover:bg-white/[0.055]',
                        )}
                      >
                        {/* 1. Cover + Track / Artist */}
                        <div className="flex min-w-0 items-center gap-3">
                          <CoverArt
                            src={item.cover_url}
                            alt={item.track_name}
                            size="sm"
                            hoverZoom
                          />
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-1.5">
                              {trackUrl ? (
                                <a
                                  href={trackUrl}
                                  target="_blank"
                                  rel="noreferrer"
                                  className="truncate text-[13.5px] font-medium text-white transition-colors hover:underline"
                                >
                                  {item.track_name}
                                </a>
                              ) : (
                                <span className="truncate text-[13.5px] font-medium text-white">
                                  {item.track_name}
                                </span>
                              )}
                              {item.explicit && (
                                <InfoTooltip content="Explicit content" side="top">
                                  <span className="shrink-0 cursor-default rounded bg-white/[0.08] px-1 py-px text-[9.5px] font-medium leading-tight text-[var(--ink-secondary)]">
                                    E
                                  </span>
                                </InfoTooltip>
                              )}
                            </div>
                            <div className="truncate text-xs text-[var(--ink-secondary)]">
                              {artists}
                              {item.album_name && (
                                <span className="hidden text-[var(--ink-muted)] xl:inline">
                                  {' '}
                                  &middot; {item.album_name}
                                </span>
                              )}
                            </div>
                          </div>
                        </div>

                        {/* 2. Listener */}
                        <div className="hidden min-w-0 sm:flex sm:items-center">
                          <button
                            type="button"
                            disabled={!onFilterByUser}
                            onClick={() => onFilterByUser?.(item.user_uri)}
                            className="tactile-press inline-flex max-w-full items-center gap-1.5 text-xs text-[var(--ink-secondary)] transition-colors enabled:cursor-pointer enabled:hover:text-white"
                            title={`Filter by ${item.user_name}`}
                          >
                            <UserAvatar
                              name={item.user_name}
                              imageUrl={userAvatarUrl}
                              size="xs"
                            />
                            <span className="truncate">{item.user_name}</span>
                          </button>
                        </div>

                        {/* 3. Context */}
                        <div className="hidden min-w-0 md:flex md:items-center">
                          {contextLabel ? (
                            hasClickableContext && item.context_uri ? (
                              <button
                                type="button"
                                onClick={() => onSelectPlaylist?.(item.context_uri!)}
                                className="tactile-press -ml-1.5 inline-flex max-w-full cursor-pointer items-center gap-1.5 rounded-[6px] px-1.5 py-0.5 text-left text-xs text-[var(--ink-muted)] transition-colors hover:bg-white/[0.07] hover:text-white"
                                title={`Inspect ${contextLabel}`}
                              >
                                <PlaylistCover
                                  size="xs"
                                  imageUrl={item.context_image_url}
                                  name={contextLabel}
                                  contextType={item.context_type}
                                  isPrivate={item.context_is_private}
                                />
                                {item.context_is_private && (
                                  <Lock className="h-3 w-3 shrink-0 text-[var(--warning)]/80" />
                                )}
                                <span className="truncate">{contextLabel}</span>
                              </button>
                            ) : (
                              <div className="inline-flex max-w-full items-center gap-1.5 text-xs text-[var(--ink-muted)]">
                                <PlaylistCover
                                  size="xs"
                                  imageUrl={item.context_image_url}
                                  name={contextLabel}
                                  contextType={item.context_type}
                                  isPrivate={item.context_is_private}
                                />
                                {item.context_is_private && (
                                  <Lock className="h-3 w-3 shrink-0 text-[var(--warning)]/80" />
                                )}
                                <span className="truncate">{contextLabel}</span>
                              </div>
                            )
                          ) : (
                            <span className="text-xs text-[var(--ink-muted)]/50">&mdash;</span>
                          )}
                        </div>

                        {/* 4. Status (Skip / Repeat) */}
                        <div className="hidden items-center justify-end gap-1.5 sm:flex">
                          {item.was_skipped && (
                            <InfoTooltip
                              content="Switched to another track before finishing"
                              side="top"
                            >
                              <span className="inline-flex cursor-default text-[var(--ink-muted)]">
                                <SkipForward className="h-3.5 w-3.5" />
                              </span>
                            </InfoTooltip>
                          )}
                          {item.inferred_repeat && (
                            <InfoTooltip
                              content="Repeated or inferred loop listen"
                              side="top"
                            >
                              <span className="inline-flex cursor-default text-[var(--ink-muted)]">
                                <Repeat className="h-3.5 w-3.5" />
                              </span>
                            </InfoTooltip>
                          )}
                        </div>

                        {/* 5. Duration */}
                        <div className="hidden text-right font-mono text-[11.5px] tabular-nums text-[var(--ink-muted)] lg:block">
                          {formatDuration(item.duration_ms)}
                        </div>

                        {/* 6. When */}
                        <div className="text-right font-mono text-[11.5px] tabular-nums text-[var(--ink-muted)]">
                          <InfoTooltip content={item.played_at_local} side="top">
                            <span className="cursor-default">
                              {formatRelativeTime(item.played_at_ms)}
                            </span>
                          </InfoTooltip>
                        </div>

                        {/* 7. Link */}
                        <div className="flex justify-end">
                          {trackUrl && (
                            <a
                              href={trackUrl}
                              target="_blank"
                              rel="noreferrer"
                              title="Open in Spotify"
                              aria-label={`Open ${item.track_name} in Spotify`}
                              className="inline-flex h-7 w-7 items-center justify-center rounded-[6px] text-[var(--ink-muted)] opacity-0 transition-all duration-150 hover:bg-white/[0.1] hover:text-white group-hover:opacity-100 focus-visible:opacity-100"
                            >
                              <ExternalLink className="h-3.5 w-3.5" />
                            </a>
                          )}
                        </div>
                      </div>
                    </StaggerItem>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}

      {/* Clean Pagination & Infinite Scroll Footer */}
      {filteredItems.length > 0 && (
        <div
          ref={sentinelRef}
          className="mt-6 flex flex-wrap items-center justify-between gap-3 border-t border-white/[0.07] pt-4"
        >
          <span className="text-xs text-[var(--ink-muted)] tabular-nums">
            {total > 0
              ? `Showing ${items.length.toLocaleString()} of ${total.toLocaleString()} listens`
              : `Showing ${items.length.toLocaleString()} listens`}
            {hasActiveLocalFilter ? ` · ${filteredItems.length} matching filter` : ''}
          </span>

          <div className="flex items-center gap-2">
            {loadingMore && (
              <span className="inline-flex items-center gap-1.5 text-xs text-[var(--ink-secondary)]">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                <span>Loading more...</span>
              </span>
            )}
            {hasMore && !loadingMore && (
              <Button variant="secondary" size="sm" onClick={handleLoadMore}>
                Load more
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
