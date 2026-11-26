import { useEffect, useMemo, useRef, useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import {
  ArrowUpDown,
  Check,
  Copy,
  Download,
  ExternalLink,
  FileSpreadsheet,
  LayoutGrid,
  Link2,
  ListMusic,
  Lock,
  PanelLeft,
  Search,
  X,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  AmbientArtGlow,
  EASE_OUT_EXPO,
  SPRING_SNAPPY,
  StaggerItem,
} from '@/components/ui/motion';
import { CoverArt } from '@/components/CoverArt';
import { EmptyState } from '@/components/EmptyState';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { PixelEq } from '@/components/PixelEq';
import { useCoverAccent } from '@/lib/extractColor';
import {
  cn,
  downloadTextFile,
  escapeCsvCell,
  formatContextLabel,
  formatDuration,
  formatRelativeTime,
  formatTotalTime,
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

type TypeFilter = 'all' | 'playlist' | 'album' | 'private';
type SortOption = 'plays' | 'recent' | 'tracks' | 'name';
type ViewMode = 'library' | 'gallery';

const TYPE_FILTERS: { id: TypeFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'playlist', label: 'Playlists' },
  { id: 'album', label: 'Albums' },
  { id: 'private', label: 'Private' },
];

const SORT_OPTIONS: { id: SortOption; label: string }[] = [
  { id: 'plays', label: 'Most played' },
  { id: 'recent', label: 'Recently played' },
  { id: 'tracks', label: 'Most tracks' },
  { id: 'name', label: 'Name A–Z' },
];

const SPOTIFY_DJ_PLAYLIST_ID = '37i9dqzf1eykqdzj48dyyq';

function isLikedSongsContext(
  contextType?: string | null,
  contextUri?: string | null,
): boolean {
  const t = (contextType ?? '').toLowerCase();
  if (t === 'liked_songs' || t === 'collection') return true;
  const u = (contextUri ?? '').toLowerCase();
  return u === 'spotify:collection' || u.endsWith(':collection') || u.includes(':collection:');
}

function isDjContext(
  contextType?: string | null,
  contextUri?: string | null,
): boolean {
  const t = (contextType ?? '').toLowerCase();
  if (t === 'dj') return true;
  const u = (contextUri ?? '').toLowerCase();
  return (
    u === 'spotify:dj' ||
    u.endsWith(':dj') ||
    u.includes(':dj:') ||
    u === `spotify:playlist:${SPOTIFY_DJ_PLAYLIST_ID}` ||
    u.endsWith(`:playlist:${SPOTIFY_DJ_PLAYLIST_ID}`)
  );
}

function isPersonalPrivateContext(
  contextType?: string | null,
  contextUri?: string | null,
): boolean {
  return isLikedSongsContext(contextType, contextUri) || isDjContext(contextType, contextUri);
}

function canonicalLiveContextUri(f: LiveFriendItem): string | null {
  if (!f.context_uri) return null;
  if (isLikedSongsContext(f.context_type, f.context_uri)) {
    const uid = f.user_uri.startsWith('spotify:user:')
      ? f.user_uri.slice('spotify:user:'.length)
      : f.user_uri;
    return `spotify:user:${uid}:collection`;
  }
  if (isDjContext(f.context_type, f.context_uri)) {
    const uid = f.user_uri.startsWith('spotify:user:')
      ? f.user_uri.slice('spotify:user:'.length)
      : f.user_uri;
    return `spotify:user:${uid}:dj`;
  }
  return f.context_uri;
}

function contextTypeLabel(type?: string | null, uri?: string | null): string {
  if (isLikedSongsContext(type, uri)) return 'Liked Songs';
  if (isDjContext(type, uri)) return 'DJ';
  return formatContextLabel(type);
}

function contextKey(c: ContextSummary): string {
  return `${c.user_uri}::${c.context_uri || c.context_name || c.context_type}`;
}

function contextDisplayName(c: ContextSummary): string {
  if (isLikedSongsContext(c.context_type, c.context_uri)) {
    return 'Liked Songs';
  }
  if (isDjContext(c.context_type, c.context_uri)) {
    return 'DJ';
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

function isAlbumContext(ctx: ContextSummary): boolean {
  return (ctx.context_type ?? '').toLowerCase() === 'album';
}

function isPlaylistLikeContext(ctx: ContextSummary): boolean {
  const t = (ctx.context_type ?? '').toLowerCase();
  return (
    t === 'playlist' ||
    isLikedSongsContext(ctx.context_type, ctx.context_uri) ||
    isDjContext(ctx.context_type, ctx.context_uri)
  );
}

interface PlaylistRailItemProps {
  ctx: ContextSummary;
  itemKey: string;
  isSelected: boolean;
  isLive: boolean;
  onSelect: () => void;
  buttonRef: (el: HTMLButtonElement | null) => void;
}

function PlaylistRailItem({
  ctx,
  itemKey,
  isSelected,
  isLive,
  onSelect,
  buttonRef,
}: PlaylistRailItemProps) {
  const displayName = contextDisplayName(ctx);
  const isPrivate = ctx.is_private || isPersonalPrivateContext(ctx.context_type, ctx.context_uri);
  const typeLabel = contextTypeLabel(ctx.context_type, ctx.context_uri);
  const itemPalette = useCoverAccent(isLive ? ctx.image_url : null, isLive ? itemKey : null);

  return (
    <button
      ref={buttonRef}
      type="button"
      onClick={onSelect}
      className={cn(
        'group tactile-press relative flex w-full items-center gap-3 rounded-[8px] px-2.5 py-2 text-left transition-colors duration-150 cursor-pointer',
        !isSelected && 'hover:bg-white/[0.055]',
      )}
    >
      {isSelected && (
        <motion.span
          layoutId="playlist-active-row"
          transition={SPRING_SNAPPY}
          className="absolute inset-0 rounded-[8px] bg-white/[0.1]"
        />
      )}

      <div className="relative z-10 shrink-0">
        <PlaylistCover
          imageUrl={ctx.image_url}
          name={displayName}
          contextType={ctx.context_type}
          isPrivate={isPrivate}
          size="md"
          hoverZoom
          className="h-11 w-11 rounded-[7px]"
        />
      </div>

      <div className="relative z-10 min-w-0 flex-1">
        <div className="flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-1.5">
            <span
              className={cn(
                'truncate text-[13px] font-medium leading-snug tracking-[-0.01em]',
                isSelected ? 'text-white' : 'text-[var(--ink-primary)]',
              )}
            >
              {displayName}
            </span>
            {isPrivate && (
              <Lock
                aria-label="Private"
                className="h-3 w-3 shrink-0 text-[var(--ink-muted)]"
              />
            )}
          </div>
          {isLive && <PixelEq active color={itemPalette.accent} className="h-3 shrink-0" />}
        </div>

        <p className="mt-0.5 truncate text-xs text-[var(--ink-muted)] tabular-nums">
          {typeLabel}
          {' · '}
          {ctx.unique_tracks} {ctx.unique_tracks === 1 ? 'track' : 'tracks'}
        </p>
      </div>
    </button>
  );
}

interface GalleryTileProps {
  ctx: ContextSummary;
  itemKey: string;
  liveHere: LiveFriendItem[];
  avatarMap: AvatarMap;
  index: number;
  onSelect: () => void;
}

function GalleryTile({
  ctx,
  itemKey,
  liveHere,
  avatarMap,
  index,
  onSelect,
}: GalleryTileProps) {
  const displayName = contextDisplayName(ctx);
  const isPrivate = ctx.is_private || isPersonalPrivateContext(ctx.context_type, ctx.context_uri);
  const ownerName = ctx.context_owner || ctx.user_name;
  const isLive = liveHere.length > 0;
  const listeners = resolveContextListeners(ctx);
  const tilePalette = useCoverAccent(isLive ? ctx.image_url : null, isLive ? itemKey : null);

  return (
    <StaggerItem index={index}>
      <button
        type="button"
        onClick={onSelect}
        className="group tactile-press flex w-full flex-col text-left cursor-pointer focus-visible:outline-none"
      >
        <div className="relative w-full">
          <PlaylistCover
            imageUrl={ctx.image_url}
            name={displayName}
            contextType={ctx.context_type}
            isPrivate={isPrivate}
            size="fluid"
            hoverZoom
            className="shadow-[var(--shadow-art)]"
          />

          {isLive && (
            <div className="pointer-events-none absolute bottom-2.5 left-2.5 inline-flex items-center gap-1.5 rounded-full bg-black/75 px-2.5 py-1 text-[11px] font-medium text-white ring-1 ring-white/15 backdrop-blur-md">
              <PixelEq active color={tilePalette.accent} className="h-2.5" />
              <span>Live</span>
            </div>
          )}

          {isPrivate && !isLive && (
            <div
              title="Private collection"
              className="pointer-events-none absolute top-2.5 right-2.5 flex h-6 w-6 items-center justify-center rounded-full bg-black/70 text-[var(--ink-secondary)] ring-1 ring-white/12 backdrop-blur-md"
            >
              <Lock className="h-3 w-3" />
            </div>
          )}
        </div>

        <div className="mt-2.5 space-y-1 px-0.5">
          <h3 className="line-clamp-1 text-[13.5px] font-medium leading-snug tracking-[-0.01em] text-white transition-colors group-hover:text-white">
            {displayName}
          </h3>
          <p className="truncate text-xs text-[var(--ink-secondary)] tabular-nums">
            {ownerName || contextTypeLabel(ctx.context_type, ctx.context_uri)}
            {' · '}
            {ctx.unique_tracks} {ctx.unique_tracks === 1 ? 'track' : 'tracks'}
            {' · '}
            {ctx.total_plays} {ctx.total_plays === 1 ? 'play' : 'plays'}
          </p>
          {listeners.length > 0 && (
            <div className="flex items-center gap-1.5 pt-0.5">
              <div className="flex items-center -space-x-1.5">
                {(isLive ? liveHere : listeners).slice(0, 3).map((person) => {
                  const uri = person.user_uri;
                  const name = person.user_name;
                  const img =
                    ('user_image_url' in person ? person.user_image_url : person.image_url) ??
                    lookupAvatar(avatarMap, uri, name);
                  return (
                    <UserAvatar
                      key={uri}
                      name={name}
                      imageUrl={img}
                      size="xs"
                      isLive={isLive}
                      ringColor={isLive ? tilePalette.accent : undefined}
                    />
                  );
                })}
              </div>
              <span className="truncate text-[11px] text-[var(--ink-muted)]">
                {isLive
                  ? liveHere.map((l) => l.user_name).join(', ')
                  : listeners.length > 1
                    ? `${listeners.length} listeners`
                    : listeners[0]?.user_name}
              </span>
            </div>
          )}
        </div>
      </button>
    </StaggerItem>
  );
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
  const [typeFilter, setTypeFilter] = useState<TypeFilter>('all');
  const [sortBy, setSortBy] = useState<SortOption>('plays');
  const [viewMode, setViewMode] = useState<ViewMode>('library');
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

  const privateOnly = typeFilter === 'private';
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

  // Apply type filter, search query, and sort (keeping live contexts pinned first)
  const sortedAndFilteredContexts = useMemo(() => {
    const q = playlistQuery.trim().toLowerCase();

    const filtered = contexts.filter((c) => {
      if (typeFilter === 'playlist' && !isPlaylistLikeContext(c)) return false;
      if (typeFilter === 'album' && !isAlbumContext(c)) return false;
      if (
        typeFilter === 'private' &&
        !c.is_private &&
        !isPersonalPrivateContext(c.context_type, c.context_uri)
      ) {
        return false;
      }

      if (!q) return true;
      const name = contextDisplayName(c).toLowerCase();
      const user = (c.user_name || '').toLowerCase();
      const owner = (c.context_owner || '').toLowerCase();
      const type = contextTypeLabel(c.context_type, c.context_uri).toLowerCase();
      return name.includes(q) || user.includes(q) || owner.includes(q) || type.includes(q);
    });

    return [...filtered].sort((a, b) => {
      const aLive = a.context_uri ? (liveByContext.get(a.context_uri)?.length ?? 0) : 0;
      const bLive = b.context_uri ? (liveByContext.get(b.context_uri)?.length ?? 0) : 0;
      if (aLive !== bLive) return bLive - aLive;

      switch (sortBy) {
        case 'recent':
          return (b.last_seen_ms ?? 0) - (a.last_seen_ms ?? 0);
        case 'tracks':
          if (b.unique_tracks !== a.unique_tracks) return b.unique_tracks - a.unique_tracks;
          return b.total_plays - a.total_plays;
        case 'name':
          return contextDisplayName(a).localeCompare(contextDisplayName(b));
        case 'plays':
        default:
          if (b.total_plays !== a.total_plays) return b.total_plays - a.total_plays;
          return (b.last_seen_ms ?? 0) - (a.last_seen_ms ?? 0);
      }
    });
  }, [contexts, typeFilter, playlistQuery, sortBy, liveByContext]);

  // Ensure a valid playlist stays selected when filtering narrows the list
  useEffect(() => {
    if (externalContextUri) return;
    if (sortedAndFilteredContexts.length === 0) return;
    const stillVisible = selectedKey
      ? sortedAndFilteredContexts.some((c) => contextKey(c) === selectedKey)
      : false;
    if (!stillVisible) {
      setSelectedKey(contextKey(sortedAndFilteredContexts[0]));
    }
  }, [sortedAndFilteredContexts, selectedKey, externalContextUri]);

  // Handle external deep-linking via focusedContextUri
  useEffect(() => {
    if (!focusedContextUri) return;
    const match = contexts.find((c) => c.context_uri === focusedContextUri);
    if (match) {
      const key = contextKey(match);
      setExternalContextUri(null);
      setSelectedKey(key);
      setTypeFilter('all');
      setPlaylistQuery('');
      setTrackQuery('');
      setViewMode('library');
      window.requestAnimationFrame(() => {
        itemRefs.current[key]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      });
      onClearFocusedContext?.();
    } else if (!loadingContexts) {
      setExternalContextUri(focusedContextUri);
      setSelectedKey(null);
      setTrackQuery('');
      setViewMode('library');
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
      isPersonalPrivateContext(selectedContext.context_type, selectedContext.context_uri)
    ) {
      if (selectedContext.user_uri) {
        params.set('friend', selectedContext.user_uri);
      }
    } else if (usersParam) {
      params.set('users', usersParam);
    } else if (
      selectedContext?.user_uri &&
      (!selectedContext.listeners || selectedContext.listeners.length <= 1)
    ) {
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

  const maxTrackPlays = useMemo(
    () => Math.max(1, ...tracks.map((t) => t.play_count)),
    [tracks],
  );

  const totalDurationMs = useMemo(
    () => tracks.reduce((sum, t) => sum + (t.duration_ms ?? 0), 0),
    [tracks],
  );

  const liveCollectionCount = useMemo(
    () =>
      sortedAndFilteredContexts.reduce(
        (count, c) =>
          count + (c.context_uri && (liveByContext.get(c.context_uri)?.length ?? 0) > 0 ? 1 : 0),
        0,
      ),
    [sortedAndFilteredContexts, liveByContext],
  );

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
  const headerIsDj = isDjContext(headerContextType, activeContextUri);
  const headerIsPersonalPrivate = headerIsLikedSongs || headerIsDj;
  const headerIsPrivate =
    headerIsPersonalPrivate ||
    (selectedContext?.is_private ?? activeLiveListeners[0]?.context_is_private ?? false);
  const headerSpotifyUrl = headerIsPersonalPrivate ? null : spotifyUriToUrl(activeContextUri);
  const headerListeners = selectedContext ? resolveContextListeners(selectedContext) : [];
  const headerOwnerName =
    selectedContext?.context_owner ||
    selectedContext?.user_name ||
    activeLiveListeners[0]?.user_name ||
    null;
  const exportBaseName =
    headerIsPersonalPrivate && headerOwnerName ? `${headerTitle}-${headerOwnerName}` : headerTitle;

  const selectedPalette = useCoverAccent(
    headerImageUrl,
    selectedContext?.context_uri || activeContextUri || headerTitle,
  );

  const inspectorKey = activeContextUri || selectedKey || 'empty';

  const heroTypeEyebrow = headerIsPrivate
    ? `Private · ${contextTypeLabel(headerContextType, activeContextUri)}`
    : contextTypeLabel(headerContextType, activeContextUri);

  // Combine live listeners first, then remaining historical listeners (deduped by user_uri)
  const mergedHeroListeners = useMemo(() => {
    const seen = new Set<string>();
    const result: Array<{
      user_uri: string;
      user_name: string;
      image_url: string | null;
      isLive: boolean;
      trackName?: string;
    }> = [];

    for (const lf of activeLiveListeners) {
      if (seen.has(lf.user_uri)) continue;
      seen.add(lf.user_uri);
      result.push({
        user_uri: lf.user_uri,
        user_name: lf.user_name,
        image_url: lf.user_image_url ?? lookupAvatar(avatarMap, lf.user_uri, lf.user_name),
        isLive: true,
        trackName: lf.track_name,
      });
    }

    for (const l of headerListeners) {
      if (seen.has(l.user_uri)) continue;
      seen.add(l.user_uri);
      result.push({
        user_uri: l.user_uri,
        user_name: l.user_name,
        image_url: l.image_url ?? lookupAvatar(avatarMap, l.user_uri, l.user_name),
        isLive: false,
      });
    }

    return result;
  }, [activeLiveListeners, headerListeners, avatarMap]);

  const handleSelectContext = (ctx: ContextSummary, switchToLibrary = false) => {
    const key = contextKey(ctx);
    setExternalContextUri(null);
    setSelectedKey(key);
    setTrackQuery('');
    if (switchToLibrary) {
      setViewMode('library');
      window.requestAnimationFrame(() => {
        itemRefs.current[key]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      });
    }
  };

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

  const showGlobalEmpty = !loadingContexts && contexts.length === 0 && !externalContextUri;

  if (showGlobalEmpty) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-[#09090b] p-8">
        <EmptyState
          title={privateOnly ? 'No private playlists discovered yet' : 'No playlists captured yet'}
          description={
            privateOnly
              ? 'Switch back to All to browse every playlist and album, or keep watching to capture Liked Songs and private collections.'
              : 'Once bside watch logs listens with playlist or album contexts, they will appear here.'
          }
          command="bside watch"
        />
      </div>
    );
  }

  /* Mode 2: Full-bleed Gallery Cover Grid */
  if (viewMode === 'gallery') {
    return (
      <div className="h-full w-full overflow-y-auto bg-[#09090b] px-7 lg:px-9 py-6">
        {/* Clean single top bar */}
        <div className="mb-6 flex flex-wrap items-center justify-between gap-3 border-b border-white/[0.07] pb-4">
          <div className="flex flex-1 flex-wrap items-center gap-2.5 min-w-[240px]">
            <div className="relative w-full sm:w-64">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
              <Input
                value={playlistQuery}
                onChange={(e) => setPlaylistQuery(e.target.value)}
                placeholder="Search playlists, albums, owners..."
                className="h-8 bg-white/[0.04] border-white/[0.08] pl-8 pr-7 text-xs"
              />
              {playlistQuery && (
                <button
                  type="button"
                  onClick={() => setPlaylistQuery('')}
                  aria-label="Clear playlist search"
                  className="absolute top-1/2 right-2 -translate-y-1/2 text-[var(--ink-muted)] transition-colors hover:text-white cursor-pointer"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              )}
            </div>

            <div
              role="tablist"
              aria-label="Filter collection type"
              className="inline-flex h-8 items-center rounded-[8px] border border-white/[0.08] bg-white/[0.03] p-0.5"
            >
              {TYPE_FILTERS.map((item) => {
                const active = typeFilter === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    role="tab"
                    aria-selected={active}
                    onClick={() => setTypeFilter(item.id)}
                    className={cn(
                      'relative inline-flex h-full items-center gap-1 rounded-[6px] px-2.5 text-xs font-medium transition-colors cursor-pointer select-none',
                      active ? 'text-white' : 'text-[var(--ink-secondary)] hover:text-white',
                    )}
                  >
                    {active && (
                      <motion.span
                        layoutId="playlist-filter-pill"
                        transition={SPRING_SNAPPY}
                        className="absolute inset-0 rounded-[6px] bg-white/[0.12]"
                      />
                    )}
                    <span className="relative z-10 inline-flex items-center gap-1">
                      {item.id === 'private' && <Lock className="h-3 w-3 opacity-75" />}
                      {item.label}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="flex items-center gap-2">
            <div className="relative inline-flex items-center">
              <ArrowUpDown className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-[var(--ink-muted)]" />
              <select
                aria-label="Sort playlists"
                value={sortBy}
                onChange={(e) => setSortBy(e.target.value as SortOption)}
                className="h-8 appearance-none rounded-[8px] border border-white/[0.08] bg-white/[0.03] pl-8 pr-7 text-xs font-medium text-white transition-colors hover:border-white/[0.16] focus-visible:border-white/30 focus-visible:outline-none cursor-pointer"
              >
                {SORT_OPTIONS.map((opt) => (
                  <option key={opt.id} value={opt.id} className="bg-[#121216] text-white">
                    {opt.label}
                  </option>
                ))}
              </select>
              <svg
                aria-hidden="true"
                viewBox="0 0 12 12"
                className="pointer-events-none absolute right-2 h-3 w-3 text-[var(--ink-muted)]"
                fill="none"
              >
                <path
                  d="M3 4.5L6 7.5L9 4.5"
                  stroke="currentColor"
                  strokeWidth="1.4"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </div>

            <div
              role="group"
              aria-label="View mode"
              className="inline-flex h-8 items-center rounded-[8px] border border-white/[0.08] bg-white/[0.03] p-0.5"
            >
              <button
                type="button"
                onClick={() => setViewMode('library')}
                aria-pressed={false}
                title="Split library view"
                className="relative inline-flex h-full items-center gap-1.5 rounded-[6px] px-2.5 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:text-white cursor-pointer select-none"
              >
                <PanelLeft className="relative z-10 h-3.5 w-3.5" />
                <span className="relative z-10 hidden sm:inline">Library</span>
              </button>
              <button
                type="button"
                onClick={() => setViewMode('gallery')}
                aria-pressed={true}
                title="Artwork gallery grid"
                className="relative inline-flex h-full items-center gap-1.5 rounded-[6px] px-2.5 text-xs font-medium text-white transition-colors cursor-pointer select-none"
              >
                <motion.span
                  layoutId="playlist-view-pill"
                  transition={SPRING_SNAPPY}
                  className="absolute inset-0 rounded-[6px] bg-white/[0.12]"
                />
                <LayoutGrid className="relative z-10 h-3.5 w-3.5" />
                <span className="relative z-10 hidden sm:inline">Gallery</span>
              </button>
            </div>
          </div>
        </div>

        {/* Summary strip */}
        <div className="mb-5 flex items-center justify-between text-xs text-[var(--ink-muted)] tabular-nums">
          <span>
            {sortedAndFilteredContexts.length}{' '}
            {sortedAndFilteredContexts.length === 1 ? 'collection' : 'collections'}
            {playlistQuery.trim() ? ` matching "${playlistQuery.trim()}"` : ''}
          </span>
          {liveCollectionCount > 0 && (
            <span className="inline-flex items-center gap-1.5 text-[var(--ink-secondary)]">
              <PixelEq active color={selectedPalette.accent} className="h-2.5" />
              <span>{liveCollectionCount} playing now</span>
            </span>
          )}
        </div>

        {loadingContexts && contexts.length === 0 ? (
          <div className="grid grid-cols-2 gap-6 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-6">
            {Array.from({ length: 12 }).map((_, idx) => (
              <div key={idx} className="space-y-2.5">
                <div className="aspect-square w-full rounded-[10px] bg-white/[0.04]" />
                <div className="h-3.5 w-3/4 rounded bg-white/[0.05]" />
                <div className="h-3 w-1/2 rounded bg-white/[0.03]" />
              </div>
            ))}
          </div>
        ) : sortedAndFilteredContexts.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <p className="text-sm font-medium text-white">No matching collections</p>
            <p className="mt-1 text-xs text-[var(--ink-muted)]">
              Try adjusting your search or type filter.
            </p>
            <Button
              variant="secondary"
              size="sm"
              className="mt-4"
              onClick={() => {
                setPlaylistQuery('');
                setTypeFilter('all');
              }}
            >
              Reset filters
            </Button>
          </div>
        ) : (
          <div className="grid grid-cols-2 gap-6 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-6 pb-8">
            {sortedAndFilteredContexts.map((ctx, idx) => {
              const key = contextKey(ctx);
              const liveHere = ctx.context_uri ? liveByContext.get(ctx.context_uri) ?? [] : [];
              return (
                <GalleryTile
                  key={key}
                  ctx={ctx}
                  itemKey={key}
                  liveHere={liveHere}
                  avatarMap={avatarMap}
                  index={idx}
                  onSelect={() => handleSelectContext(ctx, true)}
                />
              );
            })}
          </div>
        )}
      </div>
    );
  }

  /* Mode 1: Split Library Mode */
  return (
    <div className="flex h-full w-full overflow-hidden bg-[#09090b]">
      {/* Left Library Rail */}
      <aside className="flex h-full w-[290px] xl:w-[320px] shrink-0 flex-col border-r border-white/[0.07] bg-[#060608]">
        {/* Sticky top compact header */}
        <div className="shrink-0 space-y-2.5 border-b border-white/[0.06] p-3.5">
          {/* Search input */}
          <div className="relative">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
            <Input
              value={playlistQuery}
              onChange={(e) => setPlaylistQuery(e.target.value)}
              placeholder="Search library..."
              className="h-8 bg-white/[0.04] border-white/[0.07] pl-8 pr-7 text-xs"
            />
            {playlistQuery && (
              <button
                type="button"
                onClick={() => setPlaylistQuery('')}
                aria-label="Clear playlist search"
                className="absolute top-1/2 right-2 -translate-y-1/2 text-[var(--ink-muted)] transition-colors hover:text-white cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>

          {/* Compact sliding filter pills */}
          <div
            role="tablist"
            aria-label="Filter collection type"
            className="grid grid-cols-4 h-7 items-center rounded-[7px] bg-white/[0.04] p-0.5"
          >
            {TYPE_FILTERS.map((item) => {
              const active = typeFilter === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => setTypeFilter(item.id)}
                  className={cn(
                    'relative inline-flex h-full items-center justify-center rounded-[5px] px-1 text-[11.5px] font-medium transition-colors cursor-pointer select-none',
                    active ? 'text-white' : 'text-[var(--ink-secondary)] hover:text-white',
                  )}
                >
                  {active && (
                    <motion.span
                      layoutId="playlist-filter-pill"
                      transition={SPRING_SNAPPY}
                      className="absolute inset-0 rounded-[5px] bg-white/[0.12]"
                    />
                  )}
                  <span className="relative z-10 truncate">{item.label}</span>
                </button>
              );
            })}
          </div>

          {/* Sort select + Library/Gallery toggle */}
          <div className="flex items-center gap-2">
            <div className="relative flex-1 min-w-0">
              <ArrowUpDown className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
              <select
                aria-label="Sort playlists"
                value={sortBy}
                onChange={(e) => setSortBy(e.target.value as SortOption)}
                className="h-7 w-full appearance-none rounded-[7px] border border-white/[0.07] bg-white/[0.03] pl-8 pr-6 text-[11.5px] font-medium text-[var(--ink-primary)] transition-colors hover:border-white/[0.15] focus-visible:border-white/30 focus-visible:outline-none cursor-pointer"
              >
                {SORT_OPTIONS.map((opt) => (
                  <option key={opt.id} value={opt.id} className="bg-[#121216] text-white">
                    {opt.label}
                  </option>
                ))}
              </select>
              <svg
                aria-hidden="true"
                viewBox="0 0 12 12"
                className="pointer-events-none absolute top-1/2 right-2 h-3 w-3 -translate-y-1/2 text-[var(--ink-muted)]"
                fill="none"
              >
                <path
                  d="M3 4.5L6 7.5L9 4.5"
                  stroke="currentColor"
                  strokeWidth="1.4"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </div>

            <div
              role="group"
              aria-label="View mode"
              className="inline-flex h-7 shrink-0 items-center rounded-[7px] border border-white/[0.07] bg-white/[0.03] p-0.5"
            >
              <button
                type="button"
                onClick={() => setViewMode('library')}
                aria-pressed={true}
                title="Split library view"
                className="relative inline-flex h-full w-7 items-center justify-center rounded-[5px] text-white cursor-pointer"
              >
                <motion.span
                  layoutId="playlist-view-pill"
                  transition={SPRING_SNAPPY}
                  className="absolute inset-0 rounded-[5px] bg-white/[0.12]"
                />
                <PanelLeft className="relative z-10 h-3.5 w-3.5" />
              </button>
              <button
                type="button"
                onClick={() => setViewMode('gallery')}
                aria-pressed={false}
                title="Switch to gallery cover grid"
                className="relative inline-flex h-full w-7 items-center justify-center rounded-[5px] text-[var(--ink-secondary)] transition-colors hover:text-white cursor-pointer"
              >
                <LayoutGrid className="relative z-10 h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        </div>

        {/* Independently scrollable playlist list */}
        <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-0.5">
          {loadingContexts && contexts.length === 0 ? (
            <div className="space-y-1 p-1">
              {Array.from({ length: 8 }).map((_, idx) => (
                <div key={idx} className="flex items-center gap-3 rounded-[8px] px-2 py-2">
                  <div className="h-11 w-11 shrink-0 rounded-[7px] bg-white/[0.05]" />
                  <div className="flex-1 space-y-1.5">
                    <div className="h-3.5 w-3/4 rounded bg-white/[0.06]" />
                    <div className="h-3 w-1/2 rounded bg-white/[0.035]" />
                  </div>
                </div>
              ))}
            </div>
          ) : sortedAndFilteredContexts.length === 0 ? (
            <div className="flex flex-col items-center justify-center px-4 py-14 text-center">
              <p className="text-xs font-medium text-[var(--ink-secondary)]">
                No matching playlists
              </p>
              <button
                type="button"
                onClick={() => {
                  setPlaylistQuery('');
                  setTypeFilter('all');
                }}
                className="mt-2 text-xs text-white/80 hover:text-white hover:underline cursor-pointer"
              >
                Clear filters
              </button>
            </div>
          ) : (
            sortedAndFilteredContexts.map((ctx) => {
              const key = contextKey(ctx);
              const isSelected = !externalContextUri && key === selectedKey;
              const liveHere = ctx.context_uri ? liveByContext.get(ctx.context_uri) ?? [] : [];
              const isLive = liveHere.length > 0;

              return (
                <PlaylistRailItem
                  key={key}
                  ctx={ctx}
                  itemKey={key}
                  isSelected={isSelected}
                  isLive={isLive}
                  onSelect={() => handleSelectContext(ctx)}
                  buttonRef={(el) => {
                    itemRefs.current[key] = el;
                  }}
                />
              );
            })
          )}
        </div>
      </aside>

      {/* Right Full-Bleed Playlist Stage */}
      <section className="flex-1 min-w-0 h-full overflow-y-auto bg-[#09090b]">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={inspectorKey}
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.18, ease: EASE_OUT_EXPO }}
            className="min-h-full"
          >
            {/* Full-Bleed Spotify-Grade Hero */}
            <div className="relative w-full overflow-hidden px-7 lg:px-9 pt-8 pb-7">
              <AmbientArtGlow palette={selectedPalette} intensity="vivid" />

              <div className="relative z-10 flex flex-col gap-6 sm:flex-row sm:items-end">
                <PlaylistCover
                  imageUrl={headerImageUrl}
                  name={headerTitle}
                  contextType={headerContextType}
                  isPrivate={headerIsPrivate}
                  size="fluid"
                  className="h-44 w-44 rounded-[10px] shadow-[var(--shadow-art)]"
                />

                <div className="min-w-0 flex-1 pb-0.5">
                  {/* Quiet type label */}
                  <div className="flex items-center gap-1.5 text-xs font-medium text-white/75">
                    {headerIsPrivate && <Lock className="h-3 w-3 opacity-80" />}
                    <span>{heroTypeEyebrow}</span>
                  </div>

                  {/* Hero Title */}
                  <div className="mt-1.5">
                    {headerSpotifyUrl ? (
                      <a
                        href={headerSpotifyUrl}
                        target="_blank"
                        rel="noreferrer"
                        className="group inline-flex max-w-full items-center gap-2.5 text-3xl xl:text-4xl font-bold tracking-[-0.03em] text-white leading-tight hover:underline decoration-white/30 underline-offset-4"
                      >
                        <span className="truncate">{headerTitle}</span>
                        <ExternalLink className="h-4 w-4 shrink-0 text-white/50 opacity-0 transition-opacity group-hover:opacity-100" />
                      </a>
                    ) : (
                      <h1 className="truncate text-3xl xl:text-4xl font-bold tracking-[-0.03em] text-white leading-tight">
                        {headerTitle}
                      </h1>
                    )}
                  </div>

                  {/* Listeners + clean dot-separated stats */}
                  <div className="mt-3.5 flex flex-wrap items-center gap-x-2 gap-y-1.5 text-[13px] text-[var(--ink-secondary)] tabular-nums">
                    {mergedHeroListeners.length > 0 && (
                      <div className="flex items-center -space-x-1.5 mr-0.5">
                        {mergedHeroListeners.slice(0, 6).map((person) => (
                          <button
                            key={person.user_uri}
                            type="button"
                            onClick={() => onFilterByUser?.(person.user_uri)}
                            title={
                              person.isLive && person.trackName
                                ? `Listening now: ${person.user_name} — ${person.trackName}`
                                : `Filter by ${person.user_name}`
                            }
                            className="tactile-press relative inline-flex rounded-full transition-transform hover:z-10 hover:scale-110 cursor-pointer focus-visible:outline-none"
                          >
                            <UserAvatar
                              name={person.user_name}
                              imageUrl={person.image_url}
                              size="xs"
                              isLive={person.isLive}
                              ringColor={person.isLive ? selectedPalette.accent : undefined}
                            />
                          </button>
                        ))}
                      </div>
                    )}

                    {headerOwnerName && (
                      <>
                        <span className="font-medium text-white">{headerOwnerName}</span>
                        <span className="text-white/35">·</span>
                      </>
                    )}

                    <span>
                      {tracks.length} {tracks.length === 1 ? 'track' : 'tracks'}
                    </span>

                    {selectedContext && (
                      <>
                        <span className="text-white/35">·</span>
                        <span>
                          {selectedContext.total_plays}{' '}
                          {selectedContext.total_plays === 1 ? 'play' : 'plays'}
                        </span>
                      </>
                    )}

                    {totalDurationMs > 0 && (
                      <>
                        <span className="text-white/35">·</span>
                        <span>{formatTotalTime(totalDurationMs)}</span>
                      </>
                    )}

                    {selectedContext?.last_seen_ms ? (
                      <>
                        <span className="text-white/35">·</span>
                        <span>Last played {formatRelativeTime(selectedContext.last_seen_ms)}</span>
                      </>
                    ) : null}
                  </div>

                  {/* Subtle inline live-spinning line when active */}
                  {activeLiveListeners.length > 0 && (
                    <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-white/85">
                      <PixelEq active color={selectedPalette.accent} className="h-3" />
                      {activeLiveListeners.map((lf, idx) => (
                        <button
                          key={lf.user_uri}
                          type="button"
                          onClick={() => onFilterByUser?.(lf.user_uri)}
                          className="inline-flex items-center gap-1 hover:underline cursor-pointer"
                        >
                          <span className="font-medium text-white">{lf.user_name}</span>
                          <span className="text-white/50">listening to</span>
                          <span
                            className="max-w-[220px] truncate font-medium"
                            style={{ color: selectedPalette.accent }}
                          >
                            {lf.track_name}
                          </span>
                          {idx < activeLiveListeners.length - 1 && (
                            <span className="text-white/35">·</span>
                          )}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>

              {/* Clean Action Row & Right-Aligned Inline Track Filter */}
              <div className="relative z-10 mt-6 flex flex-wrap items-center justify-between gap-3">
                <div className="flex flex-wrap items-center gap-1.5">
                  {headerIsPersonalPrivate ? (
                    <span
                      title={
                        headerIsDj
                          ? 'Private DJ sessions cannot be opened directly on Spotify'
                          : 'Private Liked Songs cannot be opened directly on Spotify'
                      }
                      className="inline-flex"
                    >
                      <button
                        type="button"
                        disabled
                        className="inline-flex h-8 items-center gap-1.5 rounded-full bg-white/10 px-4 text-xs font-medium text-white/45 cursor-not-allowed"
                      >
                        <Lock className="h-3.5 w-3.5" />
                        Open in Spotify
                      </button>
                    </span>
                  ) : headerSpotifyUrl ? (
                    <a
                      href={headerSpotifyUrl}
                      target="_blank"
                      rel="noreferrer"
                      className="tactile-press inline-flex h-8 items-center gap-1.5 rounded-full bg-white px-4 text-xs font-semibold tracking-[-0.01em] text-[#09090b] transition-colors hover:bg-white/90"
                    >
                      <ExternalLink className="h-3.5 w-3.5" />
                      Open in Spotify
                    </a>
                  ) : null}

                  <button
                    type="button"
                    disabled={tracks.length === 0}
                    onClick={handleExportM3u8}
                    className="tactile-press inline-flex h-8 items-center gap-1.5 rounded-[7px] px-2.5 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:bg-white/[0.08] hover:text-white disabled:pointer-events-none disabled:opacity-40 cursor-pointer"
                  >
                    <Download className="h-3.5 w-3.5" />
                    .m3u8
                  </button>

                  <button
                    type="button"
                    disabled={tracks.length === 0}
                    onClick={handleExportCsv}
                    className="tactile-press inline-flex h-8 items-center gap-1.5 rounded-[7px] px-2.5 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:bg-white/[0.08] hover:text-white disabled:pointer-events-none disabled:opacity-40 cursor-pointer"
                  >
                    <FileSpreadsheet className="h-3.5 w-3.5" />
                    .csv
                  </button>

                  <button
                    type="button"
                    disabled={filteredTracks.length === 0}
                    onClick={handleCopyUrls}
                    className="tactile-press inline-flex h-8 items-center gap-1.5 rounded-[7px] px-2.5 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:bg-white/[0.08] hover:text-white disabled:pointer-events-none disabled:opacity-40 cursor-pointer"
                  >
                    {copiedUrls ? (
                      <Check className="h-3.5 w-3.5" style={{ color: selectedPalette.accent }} />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                    {copiedUrls ? 'Copied URLs' : 'Copy URLs'}
                  </button>

                  {activeContextUri && (
                    <button
                      type="button"
                      onClick={handleCopyUri}
                      className="tactile-press inline-flex h-8 items-center gap-1.5 rounded-[7px] px-2.5 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:bg-white/[0.08] hover:text-white cursor-pointer"
                    >
                      {copiedUri ? (
                        <Check className="h-3.5 w-3.5" style={{ color: selectedPalette.accent }} />
                      ) : (
                        <Link2 className="h-3.5 w-3.5" />
                      )}
                      {copiedUri ? 'Copied URI' : 'Copy URI'}
                    </button>
                  )}
                </div>

                {/* Right-aligned inline track filter */}
                <div className="flex items-center gap-2 w-full sm:w-auto">
                  <div className="relative flex-1 sm:w-56">
                    <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
                    <Input
                      value={trackQuery}
                      onChange={(e) => setTrackQuery(e.target.value)}
                      placeholder="Filter tracks..."
                      className="h-8 bg-black/35 border-white/[0.08] pl-8 pr-7 text-xs backdrop-blur-sm"
                    />
                    {trackQuery && (
                      <button
                        type="button"
                        onClick={() => setTrackQuery('')}
                        aria-label="Clear track search"
                        className="absolute top-1/2 right-2 -translate-y-1/2 text-[var(--ink-muted)] hover:text-white cursor-pointer"
                      >
                        <X className="h-3.5 w-3.5" />
                      </button>
                    )}
                  </div>
                  {trackQuery.trim() && (
                    <span className="shrink-0 text-xs text-[var(--ink-muted)] tabular-nums">
                      {filteredTracks.length} / {tracks.length}
                    </span>
                  )}
                </div>
              </div>
            </div>

            {/* Full-Width Aligned Spotify Tracklist */}
            <div className="px-7 lg:px-9 pb-10">
              {loadingTracks ? (
                <div className="flex flex-col items-center justify-center gap-2 py-24 text-xs text-[var(--ink-muted)]">
                  <ListMusic className="h-5 w-5 text-[var(--ink-muted)]" />
                  <span>Loading tracklist…</span>
                </div>
              ) : filteredTracks.length === 0 ? (
                <div className="py-24 text-center text-xs text-[var(--ink-muted)]">
                  {trackQuery.trim()
                    ? `No tracks matched "${trackQuery.trim()}".`
                    : 'No tracks captured for this collection yet.'}
                </div>
              ) : (
                <div>
                  {/* Sticky subtle column header */}
                  <div className="sticky top-0 z-20 mb-1 grid grid-cols-[36px_minmax(0,1fr)_96px_64px_32px] xl:grid-cols-[36px_minmax(0,1fr)_minmax(0,0.45fr)_96px_64px_32px] items-center gap-3 border-b border-white/[0.07] bg-[#09090b]/95 py-2 text-xs text-[var(--ink-muted)] backdrop-blur-md">
                    <span className="text-center">#</span>
                    <span>Title</span>
                    <span className="hidden xl:block">Album</span>
                    <span className="text-right">Plays</span>
                    <span className="text-right">Time</span>
                    <span />
                  </div>

                  {/* Track rows */}
                  <div className="space-y-0.5 pt-1">
                    {filteredTracks.map((track, idx) => {
                      const trackUrl =
                        (track.spotify_url && track.spotify_url.startsWith('https://')
                          ? track.spotify_url
                          : spotifyUriToUrl(track.track_uri)) ?? null;
                      const spinningNow = liveByTrackUri.get(track.track_uri) ?? [];
                      const isSpinning = spinningNow.length > 0;
                      const playBarPct = Math.max(
                        8,
                        Math.round((track.play_count / maxTrackPlays) * 100),
                      );

                      const rowContent = (
                        <div
                          style={
                            isSpinning
                              ? { backgroundColor: `rgba(${selectedPalette.rgb}, 0.12)` }
                              : undefined
                          }
                          className={cn(
                            'group grid grid-cols-[36px_minmax(0,1fr)_96px_64px_32px] xl:grid-cols-[36px_minmax(0,1fr)_minmax(0,0.45fr)_96px_64px_32px] items-center gap-3 rounded-[7px] px-2 -mx-2 py-2 transition-colors duration-150',
                            !isSpinning && 'hover:bg-white/[0.06]',
                          )}
                        >
                          {/* # Index or Live EQ */}
                          <div className="flex items-center justify-center">
                            {isSpinning ? (
                              <PixelEq active color={selectedPalette.accent} className="h-3.5" />
                            ) : (
                              <span className="text-xs text-[var(--ink-muted)] tabular-nums">
                                {idx + 1}
                              </span>
                            )}
                          </div>

                          {/* 40x40 Cover + Title + Artist */}
                          <div className="flex min-w-0 items-center gap-3">
                            <CoverArt
                              src={track.cover_url}
                              alt={track.track_name}
                              size="sm"
                              kind="track"
                              hoverZoom
                              className="h-10 w-10 rounded-[6px]"
                            />
                            <div className="min-w-0 flex-1">
                              {trackUrl ? (
                                <a
                                  href={trackUrl}
                                  target="_blank"
                                  rel="noreferrer"
                                  style={isSpinning ? { color: selectedPalette.accent } : undefined}
                                  className={cn(
                                    'block truncate text-[13.5px] font-medium leading-snug transition-colors hover:underline',
                                    !isSpinning && 'text-white',
                                  )}
                                >
                                  {track.track_name}
                                </a>
                              ) : (
                                <div
                                  style={isSpinning ? { color: selectedPalette.accent } : undefined}
                                  className={cn(
                                    'truncate text-[13.5px] font-medium leading-snug',
                                    !isSpinning && 'text-white',
                                  )}
                                >
                                  {track.track_name}
                                </div>
                              )}
                              <div className="mt-0.5 flex items-center gap-1.5">
                                <span className="truncate text-xs text-[var(--ink-secondary)]">
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
                                        ringColor={selectedPalette.accent}
                                      />
                                    ))}
                                  </span>
                                )}
                              </div>
                            </div>
                          </div>

                          {/* Album */}
                          <div className="hidden xl:block min-w-0 truncate text-xs text-[var(--ink-secondary)]">
                            {track.album_name || '—'}
                          </div>

                          {/* Proportional Accent Play Bar + Count */}
                          <div className="flex items-center justify-end gap-2">
                            <div className="h-1 w-12 overflow-hidden rounded-full bg-white/[0.08]">
                              <div
                                className="h-full rounded-full transition-[width,background-color] duration-300"
                                style={{
                                  width: `${playBarPct}%`,
                                  backgroundColor: selectedPalette.accent,
                                }}
                              />
                            </div>
                            <span
                              style={isSpinning ? { color: selectedPalette.accent } : undefined}
                              className={cn(
                                'min-w-[22px] text-right text-xs font-medium tabular-nums',
                                !isSpinning && 'text-[var(--ink-primary)]',
                              )}
                            >
                              {track.play_count}
                            </span>
                          </div>

                          {/* Duration */}
                          <div className="text-right font-mono text-xs text-[var(--ink-muted)] tabular-nums">
                            {formatDuration(track.duration_ms)}
                          </div>

                          {/* Hover-Revealed External Link */}
                          <div className="flex items-center justify-end">
                            {trackUrl && (
                              <a
                                href={trackUrl}
                                target="_blank"
                                rel="noreferrer"
                                title="Open in Spotify"
                                aria-label={`Open ${track.track_name} in Spotify`}
                                className="tactile-press inline-flex h-7 w-7 items-center justify-center rounded-[6px] text-[var(--ink-muted)] opacity-0 transition-all hover:bg-white/[0.08] hover:text-white group-hover:opacity-100 focus-visible:opacity-100"
                              >
                                <ExternalLink className="h-3.5 w-3.5" />
                              </a>
                            )}
                          </div>
                        </div>
                      );

                      if (idx < 14) {
                        return (
                          <StaggerItem key={track.track_uri} index={idx}>
                            {rowContent}
                          </StaggerItem>
                        );
                      }

                      return <div key={track.track_uri}>{rowContent}</div>;
                    })}
                  </div>
                </div>
              )}
            </div>
          </motion.div>
        </AnimatePresence>
      </section>
    </div>
  );
}
