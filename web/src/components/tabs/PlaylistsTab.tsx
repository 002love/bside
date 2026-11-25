import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Check,
  Copy,
  Disc3,
  Download,
  ExternalLink,
  FileSpreadsheet,
  Link2,
  ListMusic,
  Lock,
  Radio,
  Search,
  X,
} from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { EmptyState } from '@/components/EmptyState';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { PixelDot, PixelEq } from '@/components/PixelEq';
import {
  cn,
  downloadTextFile,
  escapeCsvCell,
  formatContextLabel,
  formatDuration,
  spotifyUriToUrl,
} from '@/lib/utils';
import {
  buildUsersParam,
  type AvatarMap,
  type ContextListener,
  type ContextSummary,
  type LiveFriendItem,
  type LiveSnapshot,
  type RebuiltTrack,
} from '@/types/api';

export interface PlaylistsTabProps {
  selectedUsers: string[];
  days: number | null;
  refreshTick: number;
  liveSnapshot: LiveSnapshot | null;
  avatarMap: AvatarMap;
  focusedContextUri?: string | null;
  onClearFocusedContext?: () => void;
  onFilterByUser?: (userUri: string) => void;
}

function isLikedSongsContext(
  contextType?: string | null,
  contextUri?: string | null,
): boolean {
  const t = (contextType ?? '').toLowerCase();
  if (t === 'liked_songs' || t === 'collection') return true;
  const u = (contextUri ?? '').toLowerCase();
  return u === 'spotify:collection' || u.endsWith(':collection') || u.includes(':collection:');
}

function canonicalLiveContextUri(f: LiveFriendItem): string | null {
  if (!f.context_uri) return null;
  if (isLikedSongsContext(f.context_type, f.context_uri)) {
    const uid = f.user_uri.startsWith('spotify:user:')
      ? f.user_uri.slice('spotify:user:'.length)
      : f.user_uri;
    return `spotify:user:${uid}:collection`;
  }
  return f.context_uri;
}

function contextTypeLabel(type?: string | null, uri?: string | null): string {
  if (isLikedSongsContext(type, uri)) return 'Liked Songs';
  return formatContextLabel(type);
}

function contextKey(c: ContextSummary): string {
  return `${c.user_uri}::${c.context_uri || c.context_name || c.context_type}`;
}

function contextDisplayName(c: ContextSummary): string {
  if (isLikedSongsContext(c.context_type, c.context_uri)) {
    return 'Liked Songs';
  }
  return (
    c.context_name ||
    (c.is_private
      ? `Private ${formatContextLabel(c.context_type)}`
      : c.context_uri || formatContextLabel(c.context_type))
  );
}

function resolveContextListeners(ctx: ContextSummary): ContextListener[] {
  if (ctx.listeners && ctx.listeners.length > 0) {
    return ctx.listeners;
  }
  return [
    {
      user_uri: ctx.user_uri,
      user_name: ctx.user_name,
      image_url: null,
    },
  ];
}

export function PlaylistsTab({
  selectedUsers,
  days,
  refreshTick,
  liveSnapshot,
  avatarMap,
  focusedContextUri,
  onClearFocusedContext,
  onFilterByUser,
}: PlaylistsTabProps) {
  const [privateOnly, setPrivateOnly] = useState(false);
  const [playlistQuery, setPlaylistQuery] = useState('');
  const [trackQuery, setTrackQuery] = useState('');
  const [contexts, setContexts] = useState<ContextSummary[]>([]);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [externalContextUri, setExternalContextUri] = useState<string | null>(null);
  const [tracks, setTracks] = useState<RebuiltTrack[]>([]);
  const [loadingContexts, setLoadingContexts] = useState(true);
  const [loadingTracks, setLoadingTracks] = useState(false);
  const [copiedUrls, setCopiedUrls] = useState(false);
  const [copiedUri, setCopiedUri] = useState(false);

  const itemRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  const usersParam = useMemo(() => buildUsersParam(selectedUsers), [selectedUsers]);

  // Map canonical context_uri -> active live listeners
  const liveByContext = useMemo(() => {
    const map = new Map<string, LiveFriendItem[]>();
    if (!liveSnapshot?.friends) return map;
    for (const f of liveSnapshot.friends) {
      if (!f.playing) continue;
      const uri = canonicalLiveContextUri(f);
      if (!uri) continue;
      const list = map.get(uri);
      if (list) {
        list.push(f);
      } else {
        map.set(uri, [f]);
      }
    }
    return map;
  }, [liveSnapshot]);

  // Fetch discovered contexts
  useEffect(() => {
    let active = true;
    setLoadingContexts(true);
    const params = new URLSearchParams();
    if (usersParam) params.set('users', usersParam);
    if (days != null) params.set('days', String(days));
    if (privateOnly) params.set('private_only', 'true');

    fetch(`/api/contexts?${params.toString()}`)
      .then((res) => res.json())
      .then((data: ContextSummary[]) => {
        if (!active) return;
        const list = Array.isArray(data) ? data : [];
        setContexts(list);
        if (list.length === 0) {
          setSelectedKey(null);
        } else {
          setSelectedKey((prev) => {
            if (prev && list.some((c) => contextKey(c) === prev)) {
              return prev;
            }
            return contextKey(list[0]);
          });
        }
      })
      .catch(() => {
        if (active) setContexts([]);
      })
      .finally(() => {
        if (active) setLoadingContexts(false);
      });

    return () => {
      active = false;
    };
  }, [usersParam, days, privateOnly, refreshTick]);

  // Sort contexts with active live listeners first, then apply sidebar search filter
  const sortedAndFilteredContexts = useMemo(() => {
    const q = playlistQuery.trim().toLowerCase();
    const filtered = q
      ? contexts.filter((c) => {
          const name = contextDisplayName(c).toLowerCase();
          const user = (c.user_name || '').toLowerCase();
          const owner = (c.context_owner || '').toLowerCase();
          const type = contextTypeLabel(c.context_type, c.context_uri).toLowerCase();
          return name.includes(q) || user.includes(q) || owner.includes(q) || type.includes(q);
        })
      : contexts;

    return [...filtered].sort((a, b) => {
      const aLive = a.context_uri ? (liveByContext.get(a.context_uri)?.length ?? 0) : 0;
      const bLive = b.context_uri ? (liveByContext.get(b.context_uri)?.length ?? 0) : 0;
      if (aLive !== bLive) return bLive - aLive;
      return 0;
    });
  }, [contexts, playlistQuery, liveByContext]);

  // Handle external deep-linking via focusedContextUri
  useEffect(() => {
    if (!focusedContextUri) return;
    const match = contexts.find((c) => c.context_uri === focusedContextUri);
    if (match) {
      const key = contextKey(match);
      setExternalContextUri(null);
      setSelectedKey(key);
      setPlaylistQuery('');
      setTrackQuery('');
      window.requestAnimationFrame(() => {
        itemRefs.current[key]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      });
      onClearFocusedContext?.();
    } else if (!loadingContexts) {
      setExternalContextUri(focusedContextUri);
      setSelectedKey(null);
      setTrackQuery('');
      onClearFocusedContext?.();
    }
  }, [focusedContextUri, contexts, loadingContexts, onClearFocusedContext]);

  const selectedContext = useMemo(
    () => (externalContextUri ? null : contexts.find((c) => contextKey(c) === selectedKey) ?? null),
    [contexts, selectedKey, externalContextUri],
  );

  const activeContextUri = selectedContext?.context_uri || externalContextUri || null;
  const activeLiveListeners = useMemo(
    () => (activeContextUri ? liveByContext.get(activeContextUri) ?? [] : []),
    [activeContextUri, liveByContext],
  );

  // Map track_uri -> live listeners spinning that track in this context
  const liveByTrackUri = useMemo(() => {
    const map = new Map<string, LiveFriendItem[]>();
    for (const f of activeLiveListeners) {
      if (!f.track_uri) continue;
      const list = map.get(f.track_uri);
      if (list) {
        list.push(f);
      } else {
        map.set(f.track_uri, [f]);
      }
    }
    return map;
  }, [activeLiveListeners]);

  // Fetch rebuilt tracklist
  useEffect(() => {
    const targetContext =
      externalContextUri ||
      (selectedContext
        ? selectedContext.context_uri || selectedContext.context_name || selectedContext.context_type
        : null);

    if (!targetContext) {
      setTracks([]);
      return;
    }

    let active = true;
    setLoadingTracks(true);
    const params = new URLSearchParams();
    if (
      selectedContext &&
      isLikedSongsContext(selectedContext.context_type, selectedContext.context_uri)
    ) {
      if (selectedContext.user_uri) {
        params.set('friend', selectedContext.user_uri);
      }
    } else if (usersParam) {
      params.set('users', usersParam);
    } else if (selectedContext?.user_uri && (!selectedContext.listeners || selectedContext.listeners.length <= 1)) {
      params.set('friend', selectedContext.user_uri);
    }
    if (days != null) params.set('days', String(days));
    params.set('context', targetContext);

    fetch(`/api/rebuild?${params.toString()}`)
      .then((res) => res.json())
      .then((data: RebuiltTrack[]) => {
        if (active) setTracks(Array.isArray(data) ? data : []);
      })
      .catch(() => {
        if (active) setTracks([]);
      })
      .finally(() => {
        if (active) setLoadingTracks(false);
      });

    return () => {
      active = false;
    };
  }, [selectedContext, externalContextUri, usersParam, days, refreshTick]);

  // Filter tracks by inline search
  const filteredTracks = useMemo(() => {
    const q = trackQuery.trim().toLowerCase();
    if (!q) return tracks;
    return tracks.filter(
      (t) =>
        t.track_name.toLowerCase().includes(q) ||
        t.all_artists.toLowerCase().includes(q) ||
        (t.album_name ?? '').toLowerCase().includes(q),
    );
  }, [tracks, trackQuery]);

  // Resolve header details (works for both selectedContext and externalContextUri)
  const headerTitle = useMemo(() => {
    if (selectedContext) return contextDisplayName(selectedContext);
    if (activeLiveListeners[0]?.context_name) return activeLiveListeners[0].context_name;
    if (externalContextUri) return externalContextUri;
    return 'Select a playlist';
  }, [selectedContext, activeLiveListeners, externalContextUri]);

  const headerImageUrl =
    selectedContext?.image_url ?? activeLiveListeners[0]?.context_image_url ?? null;
  const headerContextType =
    selectedContext?.context_type ?? activeLiveListeners[0]?.context_type ?? 'playlist';
  const headerIsLikedSongs = isLikedSongsContext(headerContextType, activeContextUri);
  const headerIsPrivate =
    headerIsLikedSongs ||
    (selectedContext?.is_private ?? activeLiveListeners[0]?.context_is_private ?? false);
  const headerSpotifyUrl = headerIsLikedSongs ? null : spotifyUriToUrl(activeContextUri);
  const headerListeners = selectedContext ? resolveContextListeners(selectedContext) : [];
  const headerOwnerName =
    selectedContext?.context_owner ||
    selectedContext?.user_name ||
    activeLiveListeners[0]?.user_name ||
    null;
  const headerOwnerUri =
    selectedContext?.user_uri || activeLiveListeners[0]?.user_uri || '';
  const headerOwnerImageUrl =
    headerListeners[0]?.image_url ??
    activeLiveListeners[0]?.user_image_url ??
    (headerOwnerUri ? lookupAvatar(avatarMap, headerOwnerUri, headerOwnerName ?? '') : null);
  const exportBaseName =
    headerIsLikedSongs && headerOwnerName ? `${headerTitle}-${headerOwnerName}` : headerTitle;

  const handleCopyUrls = async () => {
    const urls = filteredTracks.map((t) => t.spotify_url).filter(Boolean).join('\n');
    if (!urls) return;
    try {
      await navigator.clipboard.writeText(urls);
      setCopiedUrls(true);
      window.setTimeout(() => setCopiedUrls(false), 1800);
    } catch {
      // Clipboard blocked
    }
  };

  const handleCopyUri = async () => {
    if (!activeContextUri) return;
    try {
      await navigator.clipboard.writeText(activeContextUri);
      setCopiedUri(true);
      window.setTimeout(() => setCopiedUri(false), 1800);
    } catch {
      // Clipboard blocked
    }
  };

  const safeSlug = (name: string) =>
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'playlist';

  const handleExportM3u8 = () => {
    if (tracks.length === 0) return;
    const lines = ['#EXTM3U', `#PLAYLIST:${headerTitle}`];
    for (const t of tracks) {
      const secs = t.duration_ms ? Math.round(t.duration_ms / 1000) : -1;
      lines.push(`#EXTINF:${secs},${t.all_artists} - ${t.track_name}`);
      lines.push(t.spotify_url);
    }
    downloadTextFile(`${safeSlug(exportBaseName)}.m3u8`, `${lines.join('\n')}\n`, 'audio/x-mpegurl');
  };

  const handleExportCsv = () => {
    if (tracks.length === 0) return;
    const header = [
      'track_name',
      'artists',
      'album',
      'play_count',
      'duration_ms',
      'release_date',
      'popularity',
      'spotify_url',
    ].join(',');
    const rows = tracks.map((t) =>
      [
        escapeCsvCell(t.track_name),
        escapeCsvCell(t.all_artists),
        escapeCsvCell(t.album_name ?? ''),
        String(t.play_count),
        t.duration_ms != null ? String(t.duration_ms) : '',
        escapeCsvCell(t.release_date ?? ''),
        t.popularity != null ? String(t.popularity) : '',
        escapeCsvCell(t.spotify_url),
      ].join(','),
    );
    downloadTextFile(`${safeSlug(exportBaseName)}.csv`, `${[header, ...rows].join('\n')}\n`, 'text/csv');
  };

  if (!loadingContexts && contexts.length === 0 && !externalContextUri) {
    return (
      <div className="space-y-4 animate-fade-up">
        <div className="flex items-center justify-end">
          <div className="flex items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-3 py-1.5">
            <label
              htmlFor="private-only-empty"
              className="font-mono text-[10px] font-semibold uppercase tracking-wider text-[var(--ink-muted)] cursor-pointer"
            >
              Private only
            </label>
            <Switch
              id="private-only-empty"
              checked={privateOnly}
              onCheckedChange={setPrivateOnly}
            />
          </div>
        </div>
        <EmptyState
          title={privateOnly ? 'No private playlists discovered yet' : 'No playlists captured yet'}
          description={
            privateOnly
              ? 'Turn off "Private only" to view all playlists and albums, or keep watching to capture Liked Songs and private collections.'
              : 'Once bside watch logs listens with playlist or album contexts, they will appear here.'
          }
          command="bside watch"
        />
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[340px_minmax(0,1fr)] gap-4 lg:h-[calc(100vh-230px)] lg:min-h-[540px] lg:max-h-[820px] animate-fade-up">
      {/* Left sidebar pane: Discovered Playlists & Collections */}
      <Card className="flex flex-col min-h-0 max-h-[480px] lg:max-h-none overflow-hidden">
        <div className="shrink-0 space-y-2.5 border-b border-[var(--border-subtle)] p-4 pb-3">
          <div className="flex items-center justify-between gap-2">
            <div className="min-w-0">
              <h3 className="font-display text-base font-bold tracking-tight text-[var(--ink-primary)]">
                Playlists
              </h3>
              <p className="font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
                {sortedAndFilteredContexts.length}
                {playlistQuery.trim() ? ` of ${contexts.length}` : ''} collections
              </p>
            </div>
            <div className="flex items-center gap-2">
              <label
                htmlFor="private-only-toggle"
                className="font-mono text-[10px] font-semibold uppercase tracking-wider text-[var(--ink-muted)] cursor-pointer"
              >
                Private
              </label>
              <Switch
                id="private-only-toggle"
                checked={privateOnly}
                onCheckedChange={setPrivateOnly}
              />
            </div>
          </div>

          <div className="relative">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
            <Input
              value={playlistQuery}
              onChange={(e) => setPlaylistQuery(e.target.value)}
              placeholder="Filter playlists..."
              className="h-8 pl-8 pr-7 text-xs"
            />
            {playlistQuery && (
              <button
                type="button"
                onClick={() => setPlaylistQuery('')}
                aria-label="Clear playlist filter"
                className="absolute top-1/2 right-2 -translate-y-1/2 text-[var(--ink-muted)] hover:text-[var(--ink-primary)] cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto space-y-2 p-2.5 pr-1.5">
          {loadingContexts && contexts.length === 0 ? (
            <div className="space-y-2 p-1">
              {Array.from({ length: 6 }).map((_, idx) => (
                <div
                  key={idx}
                  className="h-[68px] animate-pulse rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)]"
                />
              ))}
            </div>
          ) : sortedAndFilteredContexts.length === 0 ? (
            <div className="py-10 text-center font-mono text-xs text-[var(--ink-muted)]">
              No matching playlists
            </div>
          ) : (
            sortedAndFilteredContexts.map((ctx) => {
              const key = contextKey(ctx);
              const isSelected = !externalContextUri && key === selectedKey;
              const displayName = contextDisplayName(ctx);
              const isLikedSongs = isLikedSongsContext(ctx.context_type, ctx.context_uri);
              const ownerName = ctx.context_owner || ctx.user_name;
              const liveHere = ctx.context_uri ? liveByContext.get(ctx.context_uri) ?? [] : [];
              const isLive = liveHere.length > 0;
              const listeners = resolveContextListeners(ctx);

              return (
                <button
                  key={key}
                  ref={(el) => {
                    itemRefs.current[key] = el;
                  }}
                  type="button"
                  onClick={() => {
                    setExternalContextUri(null);
                    setSelectedKey(key);
                    setTrackQuery('');
                  }}
                  className={cn(
                    'group w-full rounded-[var(--radius-control)] border p-2.5 text-left transition-all cursor-pointer',
                    isSelected
                      ? 'border-[var(--border-accent)] bg-[var(--accent-soft)] shadow-[var(--layer-shadow-btn-secondary)]'
                      : isLive
                        ? 'border-[var(--accent-primary)]/40 bg-[var(--bg-inset)] hover:border-[var(--accent-primary)]/70 hover:bg-[var(--bg-hover)]'
                        : 'border-transparent bg-[var(--bg-inset)]/50 hover:border-[var(--border-subtle)] hover:bg-[var(--bg-hover)]',
                  )}
                >
                  <div className="flex items-start gap-2.5">
                    <PlaylistCover
                      imageUrl={ctx.image_url}
                      name={displayName}
                      contextType={ctx.context_type}
                      isPrivate={ctx.is_private || isLikedSongs}
                      size="sm"
                      className="mt-0.5"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center justify-between gap-1.5">
                        <div className="flex min-w-0 items-center gap-1.5">
                          <span className="truncate text-xs font-semibold text-[var(--ink-primary)]">
                            {displayName}
                          </span>
                          {isLikedSongs && ownerName && (
                            <span className="inline-flex shrink-0 items-center gap-1 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--ink-secondary)]">
                              <UserAvatar
                                name={ownerName}
                                imageUrl={
                                  listeners[0]?.image_url ??
                                  lookupAvatar(avatarMap, ctx.user_uri, ownerName)
                                }
                                size="xs"
                              />
                              <span className="max-w-[84px] truncate">{ownerName}</span>
                            </span>
                          )}
                        </div>
                        {isLive ? (
                          <span className="inline-flex shrink-0 items-center gap-1 rounded-[var(--radius-pill)] border border-[var(--accent-primary)]/50 bg-[var(--accent-soft)] px-1.5 py-0.5 font-mono text-[9px] font-bold uppercase tracking-wider text-[var(--accent-primary)]">
                            <PixelDot active tone="sage" className="h-1.5 w-1.5" />
                            LIVE
                          </span>
                        ) : ctx.is_private || isLikedSongs ? (
                          <Badge
                            variant="warning"
                            className="shrink-0 gap-0.5 px-1.5 py-0 text-[9px]"
                          >
                            <Lock className="h-2.5 w-2.5" />
                            Private
                          </Badge>
                        ) : null}
                      </div>

                      <div className="mt-1 flex items-center justify-between gap-2">
                        <div className="flex min-w-0 items-center gap-1.5">
                          {isLive ? (
                            <div className="flex items-center -space-x-1.5">
                              {liveHere.slice(0, 3).map((lf) => (
                                <span
                                  key={lf.user_uri}
                                  role="button"
                                  tabIndex={-1}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    onFilterByUser?.(lf.user_uri);
                                  }}
                                  className="inline-flex cursor-pointer"
                                  title={`Listening now: ${lf.user_name} — ${lf.track_name}`}
                                >
                                  <UserAvatar
                                    name={lf.user_name}
                                    imageUrl={
                                      lf.user_image_url ??
                                      lookupAvatar(avatarMap, lf.user_uri, lf.user_name)
                                    }
                                    size="xs"
                                    isLive
                                  />
                                </span>
                              ))}
                            </div>
                          ) : (
                            <div className="flex items-center -space-x-1">
                              {listeners.slice(0, 3).map((l) => (
                                <span
                                  key={l.user_uri}
                                  role="button"
                                  tabIndex={-1}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    onFilterByUser?.(l.user_uri);
                                  }}
                                  className="inline-flex cursor-pointer"
                                  title={`Filter by ${l.user_name}`}
                                >
                                  <UserAvatar
                                    name={l.user_name}
                                    imageUrl={
                                      l.image_url ??
                                      lookupAvatar(avatarMap, l.user_uri, l.user_name)
                                    }
                                    size="xs"
                                  />
                                </span>
                              ))}
                            </div>
                          )}
                          <span className="truncate font-mono text-[10px] text-[var(--ink-muted)]">
                            {listeners.length > 1
                              ? `${listeners.length} listeners`
                              : ctx.user_name}{' '}
                            &middot; {contextTypeLabel(ctx.context_type, ctx.context_uri)}
                          </span>
                        </div>
                      </div>

                      <div className="mt-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
                        <span>{ctx.unique_tracks} tracks</span>
                        <span>&middot;</span>
                        <span className={isSelected ? 'font-semibold text-[var(--accent-primary)]' : ''}>
                          {ctx.total_plays} plays
                        </span>
                      </div>
                    </div>
                  </div>
                </button>
              );
            })
          )}
        </div>
      </Card>

      {/* Right detail pane: Rebuilt Tracklist */}
      <Card className="flex flex-col min-h-0 max-h-[680px] lg:max-h-none overflow-hidden">
        {/* Fixed playlist header */}
        <div className="shrink-0 space-y-3 border-b border-[var(--border-subtle)] p-4 pb-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="flex min-w-0 flex-1 items-start gap-3.5">
              <PlaylistCover
                imageUrl={headerImageUrl}
                name={headerTitle}
                contextType={headerContextType}
                isPrivate={headerIsPrivate}
                size="lg"
              />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  {headerSpotifyUrl ? (
                    <a
                      href={headerSpotifyUrl}
                      target="_blank"
                      rel="noreferrer"
                      className="group inline-flex min-w-0 items-center gap-1.5 font-display text-xl font-bold tracking-tight text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)]"
                    >
                      <span className="truncate">{headerTitle}</span>
                      <ExternalLink className="h-3.5 w-3.5 shrink-0 opacity-60 transition-opacity group-hover:opacity-100" />
                    </a>
                  ) : (
                    <h2 className="truncate font-display text-xl font-bold tracking-tight text-[var(--ink-primary)]">
                      {headerTitle}
                    </h2>
                  )}
                  {headerIsLikedSongs && headerOwnerName && (
                    <span className="inline-flex items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2 py-0.5 text-xs font-medium text-[var(--ink-secondary)]">
                      <UserAvatar
                        name={headerOwnerName}
                        imageUrl={headerOwnerImageUrl}
                        size="xs"
                      />
                      <span>{headerOwnerName}</span>
                    </span>
                  )}
                  {headerIsPrivate && (
                    <Badge variant="warning" className="gap-1">
                      <Lock className="h-2.5 w-2.5" />
                      Private
                    </Badge>
                  )}
                </div>

                <div className="mt-1 flex flex-wrap items-center gap-x-2.5 gap-y-1 text-xs text-[var(--ink-muted)]">
                  <span className="font-mono text-[11px] uppercase tracking-wider">
                    {contextTypeLabel(headerContextType, activeContextUri)}
                  </span>
                  <span>&middot;</span>
                  <span className="font-mono text-[11px] tabular-nums">
                    {tracks.length} {tracks.length === 1 ? 'track' : 'tracks'}
                  </span>
                  {selectedContext && (
                    <>
                      <span>&middot;</span>
                      <span className="font-mono text-[11px] tabular-nums text-[var(--accent-primary)]">
                        {selectedContext.total_plays} plays
                      </span>
                    </>
                  )}
                </div>

                {/* Listener avatars */}
                {headerListeners.length > 0 && (
                  <div className="mt-2 flex flex-wrap items-center gap-1.5">
                    {headerListeners.map((listener) => (
                      <button
                        key={listener.user_uri}
                        type="button"
                        onClick={() => onFilterByUser?.(listener.user_uri)}
                        className="inline-flex items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2 py-0.5 text-[11px] text-[var(--ink-secondary)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--ink-primary)] cursor-pointer"
                        title={`Filter by ${listener.user_name}`}
                      >
                        <UserAvatar
                          name={listener.user_name}
                          imageUrl={
                            listener.image_url ??
                            lookupAvatar(avatarMap, listener.user_uri, listener.user_name)
                          }
                          size="xs"
                        />
                        <span>{listener.user_name}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>

            {/* Action buttons */}
            <div className="flex flex-wrap items-center gap-1.5">
              {headerIsLikedSongs ? (
                <span
                  title="Private Liked Songs cannot be opened directly on Spotify"
                  className="inline-flex"
                >
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled
                    className="cursor-not-allowed opacity-60"
                  >
                    <Lock className="h-3.5 w-3.5" />
                    Open in Spotify
                  </Button>
                </span>
              ) : headerSpotifyUrl ? (
                <a
                  href={headerSpotifyUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex h-7 items-center gap-1.5 rounded-[var(--radius-control)] border border-[var(--accent-primary)] bg-[var(--accent-primary)] px-2.5 font-mono text-[11px] font-semibold uppercase tracking-wider text-[var(--ink-on-accent)] bside-btn-layered hover:bg-[var(--accent-hover)]"
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                  Open in Spotify
                </a>
              ) : activeContextUri ? (
                <Button variant="secondary" size="sm" onClick={handleCopyUri}>
                  {copiedUri ? <Check className="h-3.5 w-3.5" /> : <Link2 className="h-3.5 w-3.5" />}
                  {copiedUri ? 'Copied URI' : 'Copy URI'}
                </Button>
              ) : null}

              <Button
                variant="secondary"
                size="sm"
                disabled={filteredTracks.length === 0}
                onClick={handleCopyUrls}
              >
                {copiedUrls ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
                {copiedUrls ? 'Copied' : 'URLs'}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                disabled={tracks.length === 0}
                onClick={handleExportM3u8}
              >
                <Download className="h-3.5 w-3.5" />
                .M3U8
              </Button>
              <Button
                variant="secondary"
                size="sm"
                disabled={tracks.length === 0}
                onClick={handleExportCsv}
              >
                <FileSpreadsheet className="h-3.5 w-3.5" />
                CSV
              </Button>
            </div>
          </div>

          {/* Live Listening Now strip */}
          {activeLiveListeners.length > 0 && (
            <div className="flex flex-wrap items-center gap-2 rounded-[var(--radius-control)] border border-[var(--accent-primary)]/40 bg-[var(--accent-soft)] px-3 py-2">
              <div className="flex items-center gap-1.5 font-mono text-[10px] font-bold uppercase tracking-wider text-[var(--accent-primary)]">
                <Radio className="h-3.5 w-3.5 animate-pulse" />
                Listening Now
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {activeLiveListeners.map((lf) => (
                  <button
                    key={lf.user_uri}
                    type="button"
                    onClick={() => onFilterByUser?.(lf.user_uri)}
                    className="inline-flex items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-2 py-0.5 text-xs transition-colors hover:border-[var(--accent-primary)] cursor-pointer"
                  >
                    <UserAvatar
                      name={lf.user_name}
                      imageUrl={
                        lf.user_image_url ?? lookupAvatar(avatarMap, lf.user_uri, lf.user_name)
                      }
                      size="xs"
                      isLive
                    />
                    <span className="font-semibold text-[var(--ink-primary)]">{lf.user_name}</span>
                    <span className="text-[var(--ink-muted)]">&middot;</span>
                    <span className="max-w-[180px] truncate text-[var(--accent-primary)]">
                      {lf.track_name}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Inline track search */}
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
              <Input
                value={trackQuery}
                onChange={(e) => setTrackQuery(e.target.value)}
                placeholder="Search tracks in this playlist..."
                className="h-8 pl-8 pr-7 text-xs"
              />
              {trackQuery && (
                <button
                  type="button"
                  onClick={() => setTrackQuery('')}
                  aria-label="Clear track search"
                  className="absolute top-1/2 right-2 -translate-y-1/2 text-[var(--ink-muted)] hover:text-[var(--ink-primary)] cursor-pointer"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
            {trackQuery.trim() && (
              <span className="shrink-0 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
                {filteredTracks.length} / {tracks.length}
              </span>
            )}
          </div>
        </div>

        {/* Scrollable tracklist body */}
        <div className="flex-1 min-h-0 overflow-y-auto">
          {loadingTracks ? (
            <div className="flex flex-col items-center justify-center gap-2 py-16 font-mono text-xs uppercase tracking-wider text-[var(--ink-muted)]">
              <ListMusic className="h-5 w-5 animate-pulse text-[var(--accent-primary)]" />
              Loading tracklist...
            </div>
          ) : filteredTracks.length === 0 ? (
            <div className="py-16 text-center font-mono text-xs text-[var(--ink-muted)]">
              {trackQuery.trim()
                ? `No tracks matched "${trackQuery.trim()}".`
                : 'No tracks captured for this context yet.'}
            </div>
          ) : (
            <Table>
              <TableHeader className="sticky top-0 z-10 bg-[var(--bg-inset)]">
                <TableRow className="hover:bg-transparent">
                  <TableHead className="w-11">#</TableHead>
                  <TableHead>Track</TableHead>
                  <TableHead className="hidden md:table-cell">Album</TableHead>
                  <TableHead className="text-right">Plays</TableHead>
                  <TableHead className="text-right">Duration</TableHead>
                  <TableHead className="w-10" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredTracks.map((track, idx) => {
                  const trackUrl =
                    (track.spotify_url && track.spotify_url.startsWith('https://')
                      ? track.spotify_url
                      : spotifyUriToUrl(track.track_uri)) ?? null;
                  const spinningNow = liveByTrackUri.get(track.track_uri) ?? [];
                  const isSpinning = spinningNow.length > 0;

                  return (
                    <TableRow
                      key={track.track_uri}
                      className={cn(
                        isSpinning &&
                          'border-l-2 border-l-[var(--accent-primary)] bg-[var(--accent-soft)]/70 hover:bg-[var(--accent-soft)]',
                      )}
                    >
                      <TableCell className="font-mono text-[10px] text-[var(--ink-muted)]">
                        {isSpinning ? (
                          <PixelEq active className="h-3.5" />
                        ) : (
                          String(idx + 1).padStart(2, '0')
                        )}
                      </TableCell>
                      <TableCell className="max-w-[280px]">
                        <div className="flex items-center gap-2.5">
                          {track.cover_url ? (
                            <img
                              src={track.cover_url}
                              alt={track.track_name}
                              loading="lazy"
                              className="h-9 w-9 shrink-0 rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] object-cover"
                            />
                          ) : (
                            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] text-[var(--ink-muted)]">
                              <Disc3 className="h-4 w-4" />
                            </div>
                          )}
                          <div className="min-w-0 flex-1">
                            {trackUrl ? (
                              <a
                                href={trackUrl}
                                target="_blank"
                                rel="noreferrer"
                                className={cn(
                                  'block truncate font-medium transition-colors hover:text-[var(--accent-primary)] hover:underline',
                                  isSpinning
                                    ? 'text-[var(--accent-primary)]'
                                    : 'text-[var(--ink-primary)]',
                                )}
                              >
                                {track.track_name}
                              </a>
                            ) : (
                              <div
                                className={cn(
                                  'truncate font-medium',
                                  isSpinning
                                    ? 'text-[var(--accent-primary)]'
                                    : 'text-[var(--ink-primary)]',
                                )}
                              >
                                {track.track_name}
                              </div>
                            )}
                            <div className="flex items-center gap-1.5">
                              <span className="truncate text-[11px] text-[var(--ink-muted)]">
                                {track.all_artists}
                              </span>
                              {isSpinning && (
                                <span className="inline-flex shrink-0 items-center -space-x-1">
                                  {spinningNow.map((lf) => (
                                    <UserAvatar
                                      key={lf.user_uri}
                                      name={lf.user_name}
                                      imageUrl={
                                        lf.user_image_url ??
                                        lookupAvatar(avatarMap, lf.user_uri, lf.user_name)
                                      }
                                      size="xs"
                                      isLive
                                    />
                                  ))}
                                </span>
                              )}
                            </div>
                          </div>
                        </div>
                      </TableCell>
                      <TableCell className="hidden max-w-[180px] truncate text-[var(--ink-muted)] md:table-cell">
                        {track.album_name || '—'}
                      </TableCell>
                      <TableCell className="text-right font-mono text-xs font-semibold text-[var(--accent-primary)] tabular-nums">
                        {track.play_count}
                      </TableCell>
                      <TableCell className="text-right font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                        {formatDuration(track.duration_ms)}
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
          )}
        </div>
      </Card>
    </div>
  );
}
