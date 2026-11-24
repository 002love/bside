import { useEffect, useState } from 'react';
import { Award, ExternalLink, GitCompare, Users } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
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
import { formatRelativeTime, spotifyUriToUrl } from '@/lib/utils';
import type { OverlapReport, TimeframeKey } from '@/types/api';

interface OverlapTabProps {
  timeframe: TimeframeKey;
  refreshTick: number;
}

export function OverlapTab({ timeframe, refreshTick }: OverlapTabProps) {
  const [report, setReport] = useState<OverlapReport | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    setLoading(true);
    const params = new URLSearchParams();
    if (timeframe !== 'all') params.set('days', timeframe);

    fetch(`/api/overlap?${params.toString()}`)
      .then((res) => res.json())
      .then((data: OverlapReport) => {
        if (active) setReport(data);
      })
      .catch(() => {
        if (active) setReport(null);
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [timeframe, refreshTick]);

  if (loading && !report) {
    return <Card className="h-48 animate-pulse bg-zinc-900/40" />;
  }

  if (
    !report ||
    (report.friend_pairs.length === 0 &&
      report.shared_tracks.length === 0 &&
      report.shared_artists.length === 0)
  ) {
    return (
      <EmptyState
        title="Not enough friend overlap data yet"
        description="Taste compatibility and trendsetter leaderboards appear once listens from at least two friends have been logged."
        command="bside watch"
      />
    );
  }

  const maxTrendsetter = report.trendsetter_leaderboard[0]?.[1] ?? 1;

  return (
    <div className="space-y-5">
      {/* Pairwise Compatibility */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <CardHeader>
            <div className="flex items-center gap-2">
              <GitCompare className="h-4 w-4 text-emerald-400" />
              <CardTitle>Friend Compatibility</CardTitle>
            </div>
            <CardDescription>
              Weighted Jaccard similarity across shared artists (60%) and tracks (40%)
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {report.friend_pairs.map((pair) => {
              const scorePct = Math.round(pair.compatibility_score * 100);
              const artistPct = Math.round(pair.artist_jaccard * 100);
              const trackPct = Math.round(pair.track_jaccard * 100);
              return (
                <div
                  key={`${pair.friend_a}-${pair.friend_b}`}
                  className="rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-3.5"
                >
                  <div className="flex items-center justify-between gap-2">
                    <div className="text-xs font-medium text-zinc-100">
                      {pair.friend_a} <span className="text-zinc-500">&times;</span> {pair.friend_b}
                    </div>
                    <Badge variant="default" className="font-mono">
                      {scorePct}% match
                    </Badge>
                  </div>
                  <div className="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-zinc-800">
                    <div
                      className="h-full rounded-full bg-emerald-500"
                      style={{ width: `${Math.max(2, scorePct)}%` }}
                    />
                  </div>
                  <div className="mt-2.5 flex flex-wrap gap-4 text-[11px] text-zinc-400">
                    <span>
                      Artists:{' '}
                      <strong className="font-mono text-zinc-200">{artistPct}%</strong> (
                      {pair.shared_artists}/{pair.total_union_artists})
                    </span>
                    <span>
                      Tracks:{' '}
                      <strong className="font-mono text-zinc-200">{trackPct}%</strong> (
                      {pair.shared_tracks}/{pair.total_union_tracks})
                    </span>
                  </div>
                </div>
              );
            })}
          </CardContent>
        </Card>

        {/* Trendsetter Leaderboard */}
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <Award className="h-4 w-4 text-emerald-400" />
              <CardTitle>Trendsetter Leaderboard</CardTitle>
            </div>
            <CardDescription>Who played shared tracks first</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {report.trendsetter_leaderboard.length === 0 ? (
              <p className="text-xs text-zinc-500">No shared tracks detected yet.</p>
            ) : (
              report.trendsetter_leaderboard.map(([name, count], idx) => (
                <div key={name} className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <span className="w-4 font-mono text-[11px] text-zinc-500">{idx + 1}</span>
                      <span className="font-medium text-zinc-100">{name}</span>
                    </div>
                    <span className="font-mono text-[11px] text-emerald-400">
                      {count} {count === 1 ? 'first' : 'firsts'}
                    </span>
                  </div>
                  <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800">
                    <div
                      className="h-full rounded-full bg-emerald-500/80"
                      style={{ width: `${Math.max(5, (count / maxTrendsetter) * 100)}%` }}
                    />
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      </div>

      {/* Shared Tracks */}
      <Card>
        <CardHeader>
          <CardTitle>Shared Tracks</CardTitle>
          <CardDescription>
            Tracks played by multiple friends, highlighting who discovered them first
          </CardDescription>
        </CardHeader>
        <CardContent className="pt-0">
          {report.shared_tracks.length === 0 ? (
            <p className="py-6 text-center text-xs text-zinc-500">No shared tracks yet.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead>Track</TableHead>
                  <TableHead>First Played By</TableHead>
                  <TableHead>Listeners</TableHead>
                  <TableHead className="text-right">Total Plays</TableHead>
                  <TableHead className="w-10" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {report.shared_tracks.slice(0, 50).map((t) => {
                  const url = spotifyUriToUrl(t.track_uri);
                  return (
                    <TableRow key={t.track_uri}>
                      <TableCell>
                        <div className="font-medium text-zinc-100">{t.track_name}</div>
                        <div className="text-[11px] text-zinc-400">{t.artist_name}</div>
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Badge variant="default">{t.first_player_name}</Badge>
                          <span className="text-[11px] text-zinc-500">
                            {formatRelativeTime(t.first_played_ms)}
                          </span>
                        </div>
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-1">
                          {t.friends_who_played.map((f) => (
                            <Badge key={f} variant="secondary">
                              {f}
                            </Badge>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell className="text-right font-mono text-xs text-zinc-300">
                        {t.total_plays}
                      </TableCell>
                      <TableCell className="text-right">
                        {url && (
                          <a
                            href={url}
                            target="_blank"
                            rel="noreferrer"
                            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-emerald-400"
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

      {/* Shared Artists */}
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Users className="h-4 w-4 text-emerald-400" />
            <CardTitle>Shared Artists</CardTitle>
          </div>
          <CardDescription>Artists listened to by two or more friends</CardDescription>
        </CardHeader>
        <CardContent>
          {report.shared_artists.length === 0 ? (
            <p className="text-xs text-zinc-500">No shared artists yet.</p>
          ) : (
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
              {report.shared_artists.slice(0, 30).map(([artist, friends, plays]) => (
                <div
                  key={artist}
                  className="flex flex-col justify-between gap-2 rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-3"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-xs font-medium text-zinc-100">{artist}</span>
                    <span className="shrink-0 font-mono text-[11px] text-zinc-400">
                      {plays} plays
                    </span>
                  </div>
                  <div className="flex flex-wrap gap-1">
                    {friends.map((f) => (
                      <Badge key={f} variant="outline">
                        {f}
                      </Badge>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
