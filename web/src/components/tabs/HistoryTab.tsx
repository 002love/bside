import { useEffect, useState } from 'react';
import { Disc3, ExternalLink, Lock, Search } from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { EmptyState } from '@/components/EmptyState';
import { formatContextLabel, formatDuration } from '@/lib/utils';
import type { ListenRecord, TimeframeKey } from '@/types/api';

interface HistoryTabProps {
  friend: string;
  timeframe: TimeframeKey;
  refreshTick: number;
}

export function HistoryTab({ friend, timeframe, refreshTick }: HistoryTabProps) {
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [rows, setRows] = useState<ListenRecord[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const id = window.setTimeout(() => setDebouncedQuery(query.trim()), 200);
    return () => window.clearTimeout(id);
  }, [query]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    const params = new URLSearchParams();
    if (friend) params.set('friend', friend);
    if (timeframe !== 'all') params.set('days', timeframe);
    if (debouncedQuery) params.set('q', debouncedQuery);
    params.set('limit', '200');

    fetch(`/api/history?${params.toString()}`)
      .then((res) => res.json())
      .then((data: ListenRecord[]) => {
        if (active) setRows(Array.isArray(data) ? data : []);
      })
      .catch(() => {
        if (active) setRows([]);
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [friend, timeframe, debouncedQuery, refreshTick]);

  return (
    <div className="space-y-3">
      <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
        01 // CHRONOLOGICAL PLAY LOG
      </div>

      <Card>
        <CardHeader className="flex-row flex-wrap items-center justify-between gap-3 border-b border-[#2b2b27] pb-4">
          <div>
            <CardTitle>Listening History</CardTitle>
            <p className="mt-0.5 font-mono text-[11px] uppercase tracking-wider text-[#8d8c84]">
              {loading
                ? 'LOADING PLAYS...'
                : `${rows.length} RECENT ${rows.length === 1 ? 'PLAY' : 'PLAYS'}`}
            </p>
          </div>
          <div className="relative w-full sm:w-80">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[#8d8c84]" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="SEARCH TRACKS, ARTISTS, ALBUMS..."
              className="pl-8"
            />
          </div>
        </CardHeader>
        <CardContent className="p-0">
          {!loading && rows.length === 0 ? (
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
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead>Track</TableHead>
                  <TableHead>Album</TableHead>
                  <TableHead>Friend</TableHead>
                  <TableHead>Context</TableHead>
                  <TableHead className="text-right">Duration</TableHead>
                  <TableHead>Flags</TableHead>
                  <TableHead className="text-right">Played At</TableHead>
                  <TableHead className="w-10" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((item) => {
                  const artists = item.all_artists || item.artist_name;
                  const contextLabel =
                    item.context_name ||
                    (item.context_is_private
                      ? formatContextLabel(item.context_type)
                      : item.context_type
                        ? formatContextLabel(item.context_type)
                        : '—');

                  return (
                    <TableRow key={item.id}>
                      <TableCell className="max-w-[280px]">
                        <div className="flex items-center gap-3">
                          {item.cover_url ? (
                            <img
                              src={item.cover_url}
                              alt={item.track_name}
                              loading="lazy"
                              className="h-9 w-9 shrink-0 rounded-none border border-[#2b2b27] object-cover"
                            />
                          ) : (
                            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-none border border-[#2b2b27] bg-[#141412] text-[#8d8c84]">
                              <Disc3 className="h-4 w-4" />
                            </div>
                          )}
                          <div className="min-w-0">
                            <div className="truncate font-medium text-[#efeee8]">
                              {item.track_name}
                            </div>
                            <div className="truncate text-[10px] text-[#8d8c84]">{artists}</div>
                          </div>
                        </div>
                      </TableCell>
                      <TableCell className="max-w-[180px] truncate text-[#8d8c84]">
                        {item.album_name || '—'}
                      </TableCell>
                      <TableCell className="whitespace-nowrap text-[#efeee8]">
                        {item.user_name}
                      </TableCell>
                      <TableCell className="max-w-[210px]">
                        <div className="flex items-center gap-1.5">
                          {item.context_is_private && (
                            <Badge variant="warning" className="shrink-0 gap-1 px-1.5 py-0 text-[9px]">
                              <Lock className="h-2.5 w-2.5" />
                              PRIVATE
                            </Badge>
                          )}
                          <span className="truncate text-[#8d8c84]">{contextLabel}</span>
                        </div>
                      </TableCell>
                      <TableCell className="text-right font-mono text-[11px] text-[#8d8c84] tabular-nums">
                        {formatDuration(item.duration_ms)}
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-1">
                          {item.was_skipped && <Badge variant="destructive">SKIP</Badge>}
                          {item.inferred_repeat && <Badge variant="info">REPEAT</Badge>}
                          {item.explicit && <Badge variant="outline">E</Badge>}
                        </div>
                      </TableCell>
                      <TableCell className="whitespace-nowrap text-right font-mono text-[11px] text-[#8d8c84] tabular-nums">
                        {item.played_at_local}
                      </TableCell>
                      <TableCell className="text-right">
                        {item.spotify_url.startsWith('https://') && (
                          <a
                            href={item.spotify_url}
                            target="_blank"
                            rel="noreferrer"
                            title="Open in Spotify"
                            className="inline-flex h-7 w-7 items-center justify-center rounded-none border border-transparent text-[#8d8c84] transition-colors hover:border-[#45453e] hover:bg-[#141412] hover:text-[#89b471]"
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
        </CardContent>
      </Card>
    </div>
  );
}
