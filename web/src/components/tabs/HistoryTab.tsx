import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Disc3, ExternalLink, Loader2, Lock, Search, X } from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { EmptyState } from '@/components/EmptyState';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { PlaylistCover } from '@/components/PlaylistCover';
import { InfoTooltip } from '@/components/InfoTooltip';
import { formatContextLabel, formatDuration, spotifyUriToUrl } from '@/lib/utils';
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

const PAGE_SIZE = 50;

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

  return (
    <div className="space-y-3 animate-fade-up">
      <Card>
        <CardHeader className="flex-row flex-wrap items-center justify-between gap-3 border-b border-[var(--border-subtle)] pb-4">
          <div>
            <CardTitle>Listening History</CardTitle>
            <p className="mt-0.5 font-mono text-[11px] uppercase tracking-wider text-[var(--ink-muted)] tabular-nums">
              {loading
                ? 'Loading listens...'
                : total > 0
                  ? `Showing ${items.length} of ${total} ${total === 1 ? 'listen' : 'listens'}`
                  : `${items.length} ${items.length === 1 ? 'listen' : 'listens'}`}
            </p>
          </div>
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
                className="absolute top-1/2 right-2 -translate-y-1/2 text-[var(--ink-muted)] hover:text-[var(--ink-primary)] cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
        </CardHeader>

        <CardContent className="p-0">
          {!loading && items.length === 0 ? (
            <div className="p-5">
              <EmptyState
                title={debouncedQuery ? 'No matching listens' : 'No history recorded yet'}
                description={
                  debouncedQuery
                    ? `No listens matched "${debouncedQuery}" in the current filter.`
                    : 'Run bside watch to begin logging Spotify Friend Activity.'
                }
                command={debouncedQuery ? undefined : 'bside watch'}
              />
            </div>
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow className="hover:bg-transparent">
                    <TableHead>Track</TableHead>
                    <TableHead className="hidden lg:table-cell">Album</TableHead>
                    <TableHead>Listener</TableHead>
                    <TableHead>Context</TableHead>
                    <TableHead className="text-right">Duration</TableHead>
                    <TableHead>Flags</TableHead>
                    <TableHead className="text-right">Played At</TableHead>
                    <TableHead className="w-10" />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {items.map((item) => {
                    const artists = item.all_artists || item.artist_name;
                    const trackUrl =
                      (item.spotify_url && item.spotify_url.startsWith('https://')
                        ? item.spotify_url
                        : spotifyUriToUrl(item.track_uri)) ?? null;
                    const contextLabel =
                      item.context_name ||
                      (item.context_is_private
                        ? formatContextLabel(item.context_type)
                        : item.context_type
                          ? formatContextLabel(item.context_type)
                          : '—');
                    const userAvatarUrl =
                      item.user_image_url ?? lookupAvatar(avatarMap, item.user_uri, item.user_name);
                    const hasClickableContext = Boolean(item.context_uri && onSelectPlaylist);

                    return (
                      <TableRow key={item.id} className="animate-fade-up">
                        <TableCell className="max-w-[280px]">
                          <div className="flex items-center gap-3">
                            {item.cover_url ? (
                              <img
                                src={item.cover_url}
                                alt={item.track_name}
                                loading="lazy"
                                className="h-9 w-9 shrink-0 rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] object-cover"
                              />
                            ) : (
                              <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] text-[var(--ink-muted)]">
                                <Disc3 className="h-4 w-4" />
                              </div>
                            )}
                            <div className="min-w-0">
                              {trackUrl ? (
                                <a
                                  href={trackUrl}
                                  target="_blank"
                                  rel="noreferrer"
                                  className="block truncate font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)] hover:underline"
                                >
                                  {item.track_name}
                                </a>
                              ) : (
                                <div className="truncate font-medium text-[var(--ink-primary)]">
                                  {item.track_name}
                                </div>
                              )}
                              <div className="truncate text-[11px] text-[var(--ink-muted)]">
                                {artists}
                              </div>
                            </div>
                          </div>
                        </TableCell>

                        <TableCell className="hidden max-w-[180px] truncate text-[var(--ink-muted)] lg:table-cell">
                          {item.album_name || '—'}
                        </TableCell>

                        <TableCell className="whitespace-nowrap">
                          <button
                            type="button"
                            disabled={!onFilterByUser}
                            onClick={() => onFilterByUser?.(item.user_uri)}
                            className="inline-flex items-center gap-2 text-xs text-[var(--ink-primary)] transition-colors enabled:cursor-pointer enabled:hover:text-[var(--accent-primary)]"
                            title={`Filter by ${item.user_name}`}
                          >
                            <UserAvatar name={item.user_name} imageUrl={userAvatarUrl} size="xs" />
                            <span className="max-w-[130px] truncate">{item.user_name}</span>
                          </button>
                        </TableCell>

                        <TableCell className="max-w-[230px]">
                          {contextLabel === '—' ? (
                            <span className="text-[var(--ink-muted)]">—</span>
                          ) : hasClickableContext && item.context_uri ? (
                            <button
                              type="button"
                              onClick={() => onSelectPlaylist?.(item.context_uri!)}
                              className="group inline-flex max-w-full items-center gap-1.5 text-left text-xs text-[var(--ink-secondary)] transition-colors hover:text-[var(--accent-primary)] cursor-pointer"
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
                                <Lock className="h-3 w-3 shrink-0 text-[var(--warning)]" />
                              )}
                              <span className="truncate group-hover:underline">{contextLabel}</span>
                            </button>
                          ) : (
                            <div className="flex items-center gap-1.5 text-xs text-[var(--ink-muted)]">
                              <PlaylistCover
                                size="xs"
                                imageUrl={item.context_image_url}
                                name={contextLabel}
                                contextType={item.context_type}
                                isPrivate={item.context_is_private}
                              />
                              {item.context_is_private && (
                                <Lock className="h-3 w-3 shrink-0 text-[var(--warning)]" />
                              )}
                              <span className="truncate">{contextLabel}</span>
                            </div>
                          )}
                        </TableCell>

                        <TableCell className="text-right font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                          {formatDuration(item.duration_ms)}
                        </TableCell>

                        <TableCell>
                          <div className="flex items-center gap-1">
                            {item.was_skipped && (
                              <InfoTooltip content="Switched to another track before finishing">
                                <Badge variant="destructive">SKIP</Badge>
                              </InfoTooltip>
                            )}
                            {item.inferred_repeat && (
                              <InfoTooltip content="Repeated or inferred loop listen">
                                <Badge variant="info">REPEAT</Badge>
                              </InfoTooltip>
                            )}
                            {item.explicit && (
                              <InfoTooltip content="Explicit content">
                                <Badge variant="outline">E</Badge>
                              </InfoTooltip>
                            )}
                          </div>
                        </TableCell>

                        <TableCell className="whitespace-nowrap text-right font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                          {item.played_at_local}
                        </TableCell>

                        <TableCell className="text-right">
                          {trackUrl && (
                            <a
                              href={trackUrl}
                              target="_blank"
                              rel="noreferrer"
                              title="Open in Spotify"
                              className="inline-flex h-7 w-7 items-center justify-center rounded-[var(--radius-control)] border border-transparent text-[var(--ink-muted)] transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--bg-inset)] hover:text-[var(--accent-primary)]"
                            >
                              <ExternalLink className="h-3.5 w-3.5" />
                            </a>
                          )}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>

              {/* Infinite scroll sentinel & footer */}
              <div
                ref={sentinelRef}
                className="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--border-subtle)] bg-[var(--bg-inset)]/40 px-4 py-3"
              >
                <span className="font-mono text-[11px] uppercase tracking-wider text-[var(--ink-muted)] tabular-nums">
                  {loading
                    ? 'Loading listens...'
                    : total > 0
                      ? `Showing ${items.length} of ${total} listens`
                      : `Showing ${items.length} listens`}
                </span>

                <div className="flex items-center gap-2">
                  {loadingMore && (
                    <span className="inline-flex items-center gap-1.5 font-mono text-[11px] uppercase tracking-wider text-[var(--accent-primary)]">
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      Loading more...
                    </span>
                  )}
                  {hasMore && !loadingMore && (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={handleLoadMore}
                      className="bside-btn-layered"
                    >
                      Load more
                    </Button>
                  )}
                </div>
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
