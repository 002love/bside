import { useEffect, useMemo, useState } from 'react';
import {
  Check,
  Copy,
  Download,
  ExternalLink,
  FileSpreadsheet,
  ListMusic,
  Lock,
} from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { EmptyState } from '@/components/EmptyState';
import {
  cn,
  downloadTextFile,
  escapeCsvCell,
  formatContextLabel,
  formatDuration,
} from '@/lib/utils';
import type { ContextSummary, RebuiltTrack } from '@/types/api';

interface PlaylistsTabProps {
  friend: string;
  refreshTick: number;
}

export function PlaylistsTab({ friend, refreshTick }: PlaylistsTabProps) {
  const [privateOnly, setPrivateOnly] = useState(false);
  const [contexts, setContexts] = useState<ContextSummary[]>([]);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [tracks, setTracks] = useState<RebuiltTrack[]>([]);
  const [loadingContexts, setLoadingContexts] = useState(true);
  const [loadingTracks, setLoadingTracks] = useState(false);
  const [copied, setCopied] = useState(false);

  const contextKey = (c: ContextSummary) =>
    `${c.user_uri}::${c.context_uri || c.context_name || c.context_type}`;

  useEffect(() => {
    let active = true;
    setLoadingContexts(true);
    const params = new URLSearchParams();
    if (friend) params.set('friend', friend);
    if (privateOnly) params.set('private_only', 'true');

    fetch(`/api/contexts?${params.toString()}`)
      .then((res) => res.json())
      .then((data: ContextSummary[]) => {
        if (!active) return;
        const list = Array.isArray(data) ? data : [];
        setContexts(list);
        if (list.length === 0) {
          setSelectedKey(null);
        } else if (!selectedKey || !list.some((c) => contextKey(c) === selectedKey)) {
          setSelectedKey(contextKey(list[0]));
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
  }, [friend, privateOnly, refreshTick]);

  const selectedContext = useMemo(
    () => contexts.find((c) => contextKey(c) === selectedKey) ?? null,
    [contexts, selectedKey],
  );

  useEffect(() => {
    if (!selectedContext) {
      setTracks([]);
      return;
    }
    let active = true;
    setLoadingTracks(true);
    const params = new URLSearchParams();
    params.set('friend', selectedContext.user_uri);
    params.set(
      'context',
      selectedContext.context_uri || selectedContext.context_name || selectedContext.context_type,
    );

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
  }, [selectedContext, refreshTick]);

  const handleCopyUrls = async () => {
    const urls = tracks.map((t) => t.spotify_url).join('\n');
    if (!urls) return;
    try {
      await navigator.clipboard.writeText(urls);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      // Fallback ignored if clipboard blocked
    }
  };

  const safeSlug = (name: string) =>
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'playlist';

  const handleExportM3u8 = () => {
    if (!selectedContext || tracks.length === 0) return;
    const title = selectedContext.context_name || formatContextLabel(selectedContext.context_type);
    const lines = ['#EXTM3U', `#PLAYLIST:${title}`];
    for (const t of tracks) {
      const secs = t.duration_ms ? Math.round(t.duration_ms / 1000) : -1;
      lines.push(`#EXTINF:${secs},${t.all_artists} - ${t.track_name}`);
      lines.push(t.spotify_url);
    }
    downloadTextFile(`${safeSlug(title)}.m3u8`, `${lines.join('\n')}\n`, 'audio/x-mpegurl');
  };

  const handleExportCsv = () => {
    if (!selectedContext || tracks.length === 0) return;
    const title = selectedContext.context_name || formatContextLabel(selectedContext.context_type);
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
    downloadTextFile(`${safeSlug(title)}.csv`, `${[header, ...rows].join('\n')}\n`, 'text/csv');
  };

  if (!loadingContexts && contexts.length === 0) {
    return (
      <div className="space-y-4">
        <div className="flex items-center justify-end gap-2">
          <label
            htmlFor="private-only-empty"
            className="text-xs font-medium text-zinc-400 cursor-pointer"
          >
            Private only
          </label>
          <Switch
            id="private-only-empty"
            checked={privateOnly}
            onCheckedChange={setPrivateOnly}
          />
        </div>
        <EmptyState
          title={privateOnly ? 'No private contexts discovered yet' : 'No listening contexts yet'}
          description={
            privateOnly
              ? 'Toggle off "Private only" to see all playlists and albums, or keep watching to capture Liked Songs and hidden playlists.'
              : 'Once bside watch logs listens with playlist or album contexts, you can inspect and reconstruct them here.'
          }
          command="bside watch"
        />
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
      {/* Left list: Discovered Contexts */}
      <Card className="lg:col-span-4">
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between gap-2">
            <div>
              <CardTitle>Discovered Contexts</CardTitle>
              <CardDescription>{contexts.length} playlists & collections</CardDescription>
            </div>
            <div className="flex items-center gap-2">
              <label
                htmlFor="private-only-toggle"
                className="text-[11px] font-medium text-zinc-400 cursor-pointer"
              >
                Private only
              </label>
              <Switch
                id="private-only-toggle"
                checked={privateOnly}
                onCheckedChange={setPrivateOnly}
              />
            </div>
          </div>
        </CardHeader>
        <CardContent className="p-2 pt-0">
          <ScrollArea className="h-[560px] pr-2">
            <div className="space-y-1">
              {contexts.map((ctx) => {
                const key = contextKey(ctx);
                const isSelected = key === selectedKey;
                const displayName =
                  ctx.context_name ||
                  (ctx.is_private
                    ? `Private ${formatContextLabel(ctx.context_type)}`
                    : ctx.context_uri || formatContextLabel(ctx.context_type));

                return (
                  <button
                    key={key}
                    type="button"
                    onClick={() => setSelectedKey(key)}
                    className={cn(
                      'w-full rounded-lg border px-3 py-2.5 text-left transition-colors cursor-pointer',
                      isSelected
                        ? 'border-emerald-500/40 bg-emerald-500/10'
                        : 'border-transparent bg-transparent hover:border-zinc-800 hover:bg-zinc-800/40',
                    )}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <div className="truncate text-xs font-medium text-zinc-100">
                          {displayName}
                        </div>
                        <div className="mt-0.5 truncate text-[11px] text-zinc-400">
                          {ctx.user_name} &middot; {formatContextLabel(ctx.context_type)}
                        </div>
                      </div>
                      {ctx.is_private && (
                        <Badge variant="warning" className="shrink-0 gap-1 px-1.5 py-0 text-[10px]">
                          <Lock className="h-2.5 w-2.5" />
                          PRIVATE
                        </Badge>
                      )}
                    </div>
                    <div className="mt-2 flex items-center gap-3 font-mono text-[11px] text-zinc-500">
                      <span>{ctx.unique_tracks} tracks</span>
                      <span>&middot;</span>
                      <span>{ctx.total_plays} plays</span>
                    </div>
                  </button>
                );
              })}
            </div>
          </ScrollArea>
        </CardContent>
      </Card>

      {/* Right panel: Rebuilt Tracklist */}
      <Card className="lg:col-span-8">
        <CardHeader className="flex-row flex-wrap items-start justify-between gap-3 pb-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <ListMusic className="h-4 w-4 shrink-0 text-emerald-400" />
              <CardTitle className="truncate">
                {selectedContext
                  ? selectedContext.context_name ||
                    formatContextLabel(selectedContext.context_type)
                  : 'Select a context'}
              </CardTitle>
              {selectedContext?.is_private && (
                <Badge variant="warning" className="gap-1">
                  <Lock className="h-2.5 w-2.5" />
                  PRIVATE
                </Badge>
              )}
            </div>
            <CardDescription className="mt-1">
              {selectedContext
                ? `Reconstructed from ${selectedContext.user_name}'s logged listens (${tracks.length} unique tracks)`
                : 'Choose a playlist or Liked Songs collection from the list'}
            </CardDescription>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="primary"
              size="sm"
              disabled={tracks.length === 0}
              onClick={handleCopyUrls}
            >
              {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
              {copied ? 'Copied URLs' : 'Copy Spotify URLs'}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              disabled={tracks.length === 0}
              onClick={handleExportM3u8}
            >
              <Download className="h-3.5 w-3.5" />
              Export .m3u8
            </Button>
            <Button
              variant="secondary"
              size="sm"
              disabled={tracks.length === 0}
              onClick={handleExportCsv}
            >
              <FileSpreadsheet className="h-3.5 w-3.5" />
              Export CSV
            </Button>
          </div>
        </CardHeader>

        <CardContent className="pt-0">
          {loadingTracks ? (
            <div className="py-12 text-center text-xs text-zinc-500">
              Reconstructing tracklist...
            </div>
          ) : tracks.length === 0 ? (
            <div className="py-12 text-center text-xs text-zinc-500">
              No tracks captured for this context yet.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead className="w-10">#</TableHead>
                  <TableHead>Track</TableHead>
                  <TableHead>Album</TableHead>
                  <TableHead className="text-right">Plays</TableHead>
                  <TableHead className="text-right">Duration</TableHead>
                  <TableHead className="w-10" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {tracks.map((track, idx) => (
                  <TableRow key={track.track_uri}>
                    <TableCell className="font-mono text-[11px] text-zinc-500">{idx + 1}</TableCell>
                    <TableCell className="max-w-[260px]">
                      <div className="truncate font-medium text-zinc-100">{track.track_name}</div>
                      <div className="truncate text-[11px] text-zinc-400">{track.all_artists}</div>
                    </TableCell>
                    <TableCell className="max-w-[180px] truncate text-zinc-400">
                      {track.album_name || '—'}
                    </TableCell>
                    <TableCell className="text-right font-mono text-xs text-emerald-400">
                      {track.play_count}
                    </TableCell>
                    <TableCell className="text-right font-mono text-[11px] text-zinc-400">
                      {formatDuration(track.duration_ms)}
                    </TableCell>
                    <TableCell className="text-right">
                      {track.spotify_url.startsWith('https://') && (
                        <a
                          href={track.spotify_url}
                          target="_blank"
                          rel="noreferrer"
                          title="Open in Spotify"
                          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-emerald-400"
                        >
                          <ExternalLink className="h-3.5 w-3.5" />
                        </a>
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
