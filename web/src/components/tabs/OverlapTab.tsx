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
    return <Card className="h-48 animate-pulse bg-[#181816]" />;
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
    <div className="space-y-6">
      {/* 01 // Pairwise Compatibility & Trendsetter */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          01 // TASTE COMPATIBILITY & DISCOVERY
        </div>

        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          <Card className="lg:col-span-2">
            <CardHeader>
              <div className="flex items-center gap-2">
                <GitCompare className="h-4 w-4 text-[#89b471]" />
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
                    className="rounded-none border border-[#2b2b27] bg-[#141412] p-3.5"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <div className="font-mono text-xs font-semibold uppercase tracking-wider text-[#efeee8]">
                        {pair.friend_a} <span className="text-[#8d8c84]">&times;</span> {pair.friend_b}
                      </div>
                      <div className="flex items-baseline gap-1 font-display text-2xl font-extrabold leading-none text-[#89b471] tabular-nums">
                        <span>{scorePct}%</span>
                        <span className="font-mono text-[9px] font-semibold uppercase tracking-wider text-[#8d8c84]">
                          MATCH
                        </span>
                      </div>
                    </div>
                    <div className="mt-2.5 h-1.5 w-full bg-[#2b2b27]">
                      <div
                        className="h-full bg-[#89b471]"
                        style={{ width: `${Math.max(2, scorePct)}%` }}
                      />
                    </div>
                    <div className="mt-2.5 flex flex-wrap gap-4 font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                      <span>
                        ARTISTS:{' '}
                        <strong className="text-[#efeee8]">{artistPct}%</strong> (
                        {pair.shared_artists}/{pair.total_union_artists})
                      </span>
                      <span>
                        TRACKS:{' '}
                        <strong className="text-[#efeee8]">{trackPct}%</strong> (
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
                <Award className="h-4 w-4 text-[#89b471]" />
                <CardTitle>Trendsetters</CardTitle>
              </div>
              <CardDescription>Who played shared tracks first</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {report.trendsetter_leaderboard.length === 0 ? (
                <p className="font-mono text-xs text-[#8d8c84]">No shared tracks detected yet.</p>
              ) : (
                report.trendsetter_leaderboard.map(([name, count], idx) => (
                  <div key={name} className="space-y-1.5">
                    <div className="flex items-center justify-between font-mono text-xs">
                      <div className="flex items-center gap-2">
                        <span className="w-5 text-[10px] text-[#8d8c84]">
                          {String(idx + 1).padStart(2, '0')}
                        </span>
                        <span className="font-semibold text-[#efeee8]">{name}</span>
                      </div>
                      <span className="text-[11px] font-semibold text-[#89b471] tabular-nums">
                        {count} {count === 1 ? 'FIRST' : 'FIRSTS'}
                      </span>
                    </div>
                    <div className="h-1.5 w-full bg-[#2b2b27]">
                      <div
                        className="h-full bg-[#557445]"
                        style={{ width: `${Math.max(5, (count / maxTrendsetter) * 100)}%` }}
                      />
                    </div>
                  </div>
                ))
              )}
            </CardContent>
          </Card>
        </div>
      </section>

      {/* 02 // Shared Tracks */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          02 // SHARED TRACKS
        </div>
        <Card>
          <CardHeader className="border-b border-[#2b2b27]">
            <CardTitle>Shared Tracks</CardTitle>
            <CardDescription>
              Tracks played by multiple friends, highlighting who discovered them first
            </CardDescription>
          </CardHeader>
          <CardContent className="p-0">
            {report.shared_tracks.length === 0 ? (
              <p className="py-8 text-center font-mono text-xs text-[#8d8c84]">No shared tracks yet.</p>
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
                          <div className="font-medium text-[#efeee8]">{t.track_name}</div>
                          <div className="text-[10px] text-[#8d8c84]">{t.artist_name}</div>
                        </TableCell>
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <Badge variant="default">{t.first_player_name}</Badge>
                            <span className="font-mono text-[10px] uppercase text-[#8d8c84]">
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
                        <TableCell className="text-right font-mono text-xs font-semibold text-[#efeee8] tabular-nums">
                          {t.total_plays}
                        </TableCell>
                        <TableCell className="text-right">
                          {url && (
                            <a
                              href={url}
                              target="_blank"
                              rel="noreferrer"
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
      </section>

      {/* 03 // Shared Artists */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          03 // SHARED ARTISTS
        </div>
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <Users className="h-4 w-4 text-[#89b471]" />
              <CardTitle>Shared Artists</CardTitle>
            </div>
            <CardDescription>Artists listened to by two or more friends</CardDescription>
          </CardHeader>
          <CardContent>
            {report.shared_artists.length === 0 ? (
              <p className="font-mono text-xs text-[#8d8c84]">No shared artists yet.</p>
            ) : (
              <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
                {report.shared_artists.slice(0, 30).map(([artist, friends, plays]) => (
                  <div
                    key={artist}
                    className="flex flex-col justify-between gap-2.5 rounded-none border border-[#2b2b27] bg-[#141412] p-3"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate font-mono text-xs font-semibold text-[#efeee8]">
                        {artist}
                      </span>
                      <span className="shrink-0 font-mono text-[10px] uppercase text-[#89b471] tabular-nums">
                        {plays} PLAYS
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
      </section>
    </div>
  );
}
