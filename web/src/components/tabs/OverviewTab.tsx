import { useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip as RechartsTooltip,
  XAxis,
  YAxis,
} from 'recharts';
import {
  Activity,
  Clock,
  Disc3,
  ExternalLink,
  Flame,
  Music,
  SkipForward,
  Sparkles,
  Users,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { EmptyState } from '@/components/EmptyState';
import { InfoTooltip } from '@/components/InfoTooltip';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { cn, formatContextLabel, formatRelativeTime, formatTotalTime } from '@/lib/utils';
import { buildUsersParam, type AvatarMap, type OverviewResponse } from '@/types/api';

const WEEKDAYS = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
const HOURS = Array.from({ length: 24 }, (_, i) => i);

export interface OverviewTabProps {
  selectedUsers: string[];
  days: number | null;
  refreshTick: number;
  avatarMap: AvatarMap;
  liveStrip?: ReactNode;
  onSelectPlaylist?: (contextUri: string) => void;
  onFilterByUser?: (userUri: string) => void;
}

function heatCellStyle(count: number, max: number): string {
  if (count <= 0 || max <= 0) return 'var(--heat-0)';
  const ratio = count / max;
  if (ratio < 0.25) return 'var(--heat-1)';
  if (ratio < 0.5) return 'var(--heat-2)';
  if (ratio < 0.75) return 'var(--heat-3)';
  return 'var(--heat-4)';
}

interface CustomTooltipPayloadItem {
  value?: number;
  name?: string;
}

function ChartTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: CustomTooltipPayloadItem[];
  label?: string;
}) {
  if (!active || !payload || payload.length === 0) return null;
  const val = payload[0]?.value ?? 0;

  return (
    <div
      className="px-3 py-2 text-xs"
      style={{
        backgroundColor: 'var(--chart-tooltip-bg)',
        border: '1px solid var(--chart-tooltip-border)',
        borderRadius: 'var(--radius-control)',
        boxShadow: 'var(--layer-shadow-popover)',
      }}
    >
      <div className="font-mono text-[10px] font-semibold uppercase tracking-wider text-[var(--ink-muted)]">
        {label}
      </div>
      <div className="mt-0.5 font-mono text-xs font-semibold text-[var(--ink-primary)] tabular-nums">
        {val.toLocaleString()} {val === 1 ? 'play' : 'plays'}
      </div>
    </div>
  );
}

export function OverviewTab({
  selectedUsers,
  days,
  refreshTick,
  avatarMap,
  liveStrip,
  onSelectPlaylist,
  onFilterByUser,
}: OverviewTabProps) {
  const [data, setData] = useState<OverviewResponse | null>(null);
  const [loading, setLoading] = useState(true);

  const usersParam = buildUsersParam(selectedUsers);

  useEffect(() => {
    let active = true;
    setLoading(true);

    const params = new URLSearchParams();
    if (usersParam) params.set('users', usersParam);
    if (days != null) params.set('days', String(days));

    fetch(`/api/overview?${params.toString()}`)
      .then((res) => res.json())
      .then((json: OverviewResponse) => {
        if (!active) return;
        setData(json);
      })
      .catch(() => {
        if (active) setData(null);
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [usersParam, days, refreshTick]);

  const stats = data?.stats;

  const hourlyData = useMemo(() => {
    if (!stats) return [];
    return stats.hourly.map((plays, hour) => ({
      hour: `${String(hour).padStart(2, '0')}H`,
      plays,
    }));
  }, [stats]);

  const weekdayData = useMemo(() => {
    if (!stats) return [];
    return stats.weekday.map((plays, idx) => ({
      day: WEEKDAYS[idx] ?? `D${idx}`,
      plays,
    }));
  }, [stats]);

  const heatmapMax = useMemo(() => {
    if (!stats) return 0;
    let max = 0;
    for (const row of stats.heatmap_grid) {
      for (const val of row) {
        if (val > max) max = val;
      }
    }
    return max;
  }, [stats]);

  const trackCoverLookup = useMemo(() => {
    const map = new Map<string, { coverUrl: string | null; spotifyUrl: string }>();
    for (const listen of data?.recent_listens ?? []) {
      const key = `${listen.artist_name.toLowerCase()}||${listen.track_name.toLowerCase()}`;
      if (!map.has(key)) {
        map.set(key, { coverUrl: listen.cover_url, spotifyUrl: listen.spotify_url });
      }
    }
    return map;
  }, [data?.recent_listens]);

  const selectedSet = useMemo(
    () => new Set(selectedUsers.map((u) => u.toLowerCase())),
    [selectedUsers],
  );

  if (loading && !stats) {
    return (
      <div className="space-y-5">
        {liveStrip}
        <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-5">
          {Array.from({ length: 5 }).map((_, i) => (
            <Card key={i} className="h-28 animate-pulse" />
          ))}
        </div>
      </div>
    );
  }

  if (!stats || stats.total_plays === 0) {
    return (
      <div className="space-y-5">
        {liveStrip}
        <EmptyState
          title="No listening activity recorded yet"
          description="Start polling Spotify Friend Activity to populate your local SQLite database with plays, skips, repeats, and listening sessions."
          command="bside watch"
        />
      </div>
    );
  }

  const judgedPlays = stats.skipped_plays + stats.completed_plays;
  const skipRate = judgedPlays > 0 ? (stats.skipped_plays / judgedPlays) * 100 : null;
  const maxTopTrack = stats.top_tracks[0]?.[2] ?? 1;
  const maxTopArtist = stats.top_artists[0]?.[1] ?? 1;
  const maxContext = stats.context_breakdown[0]?.[1] ?? 1;
  const maxDecade = stats.release_decades[0]?.[1] ?? 1;

  // Support both `most_obscure_track` (current API) and optional `deep_cut_track`
  const deepCut =
    stats.most_obscure_track ??
    (stats as unknown as { deep_cut_track?: [string, string, number] | null }).deep_cut_track ??
    null;

  const friendsSummary = data?.friends_summary ?? [];
  const recentListens = data?.recent_listens ?? [];

  return (
    <div className="space-y-6 animate-fade-up">
      {/* Compact Collapsible On Air Strip */}
      {liveStrip}

      {/* KPI Cards */}
      <section className="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-5">
        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
                Total Plays
              </span>
              <InfoTooltip
                iconOnly
                content="Total deduplicated track listens recorded in the selected time window and listener filter."
              />
            </div>
            <Activity className="h-3.5 w-3.5 text-[var(--accent-primary)]" />
          </CardHeader>
          <CardContent>
            <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[var(--ink-primary)] tabular-nums">
              {stats.total_plays.toLocaleString()}
            </div>
            <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
              {stats.active_days} {stats.active_days === 1 ? 'day' : 'days'} &middot;{' '}
              {stats.session_count} sessions
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
                Listening Time
              </span>
              <InfoTooltip
                iconOnly
                content="Estimated total playback time based on actual listened duration per play."
              />
            </div>
            <Clock className="h-3.5 w-3.5 text-[var(--accent-primary)]" />
          </CardHeader>
          <CardContent>
            <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[var(--ink-primary)] tabular-nums">
              {formatTotalTime(stats.total_listened_ms)}
            </div>
            <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
              Avg session:{' '}
              {stats.avg_session_ms ? formatTotalTime(stats.avg_session_ms) : 'N/A'}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
                Unique Catalog
              </span>
              <InfoTooltip
                iconOnly
                content="Count of distinct tracks and artists heard across the selected listens."
              />
            </div>
            <Disc3 className="h-3.5 w-3.5 text-[var(--accent-primary)]" />
          </CardHeader>
          <CardContent>
            <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[var(--ink-primary)] tabular-nums">
              {stats.unique_tracks.toLocaleString()}
            </div>
            <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
              Tracks / {stats.unique_artists.toLocaleString()} artists
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
                Skip Rate
              </span>
              <InfoTooltip
                iconOnly
                content="Percentage of plays switched to another track before completion (after filtering out mid-track pauses & heartbeats)."
              />
            </div>
            <SkipForward
              className={
                skipRate != null && skipRate >= 35
                  ? 'h-3.5 w-3.5 text-[var(--warning)]'
                  : 'h-3.5 w-3.5 text-[var(--accent-primary)]'
              }
            />
          </CardHeader>
          <CardContent>
            <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[var(--ink-primary)] tabular-nums">
              {skipRate != null ? `${skipRate.toFixed(1)}%` : '—'}
            </div>
            <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
              {stats.skipped_plays} skip / {stats.completed_plays} full
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
                Popularity &amp; Streak
              </span>
              <InfoTooltip
                iconOnly
                content="Average Spotify popularity score (0–100). 0 = obscure underground/deep cut, 100 = global mainstream hit."
              />
            </div>
            <Flame className="h-3.5 w-3.5 text-[var(--accent-primary)]" />
          </CardHeader>
          <CardContent>
            <div className="flex items-baseline gap-1.5">
              <span className="font-display text-4xl font-extrabold leading-none tracking-tight text-[var(--ink-primary)] tabular-nums">
                {stats.avg_popularity != null ? Math.round(stats.avg_popularity) : '—'}
              </span>
              <span className="font-mono text-[10px] text-[var(--ink-muted)]">/100</span>
              <InfoTooltip
                content="Longest run of consecutive calendar days with at least one recorded listen."
                className="ml-auto"
              >
                <Badge variant="default">{stats.current_streak_days}d streak</Badge>
              </InfoTooltip>
            </div>
            <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
              Best streak: {stats.longest_streak_days}d
            </p>
          </CardContent>
        </Card>
      </section>

      {/* 7x24 Listening Heatmap */}
      <Card>
        <CardHeader className="flex-row flex-wrap items-center justify-between gap-2 pb-3">
          <div>
            <CardTitle>7&times;24 Listening Heatmap</CardTitle>
            <CardDescription>Listening cadence across weekdays and hours</CardDescription>
          </div>
          <div className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
            <span>Less</span>
            {['var(--heat-0)', 'var(--heat-1)', 'var(--heat-2)', 'var(--heat-3)', 'var(--heat-4)'].map(
              (color, idx) => (
                <span
                  key={idx}
                  className="h-3 w-3 rounded-[min(3px,var(--radius-control))] border border-[var(--border-subtle)]"
                  style={{ backgroundColor: color }}
                />
              ),
            )}
            <span>More</span>
          </div>
        </CardHeader>
        <CardContent>
          <div className="overflow-x-auto">
            <div className="min-w-[640px]">
              <div className="mb-1.5 grid grid-cols-[44px_repeat(24,minmax(0,1fr))] gap-1 font-mono text-[10px] text-[var(--ink-muted)]">
                <div />
                {HOURS.map((h) => (
                  <div key={h} className="text-center">
                    {h % 3 === 0 ? String(h).padStart(2, '0') : ''}
                  </div>
                ))}
              </div>
              <div className="space-y-1">
                {WEEKDAYS.map((day, dIdx) => (
                  <div
                    key={day}
                    className="grid grid-cols-[44px_repeat(24,minmax(0,1fr))] items-center gap-1"
                  >
                    <span className="font-mono text-[10px] font-semibold text-[var(--ink-muted)]">
                      {day}
                    </span>
                    {HOURS.map((h) => {
                      const count = stats.heatmap_grid[dIdx]?.[h] ?? 0;
                      return (
                        <Tooltip key={h}>
                          <TooltipTrigger asChild>
                            <div
                              className="h-5 rounded-[min(3px,var(--radius-control))] border border-[var(--border-subtle)] transition-all hover:scale-[1.06] hover:border-[var(--ink-primary)]"
                              style={{ backgroundColor: heatCellStyle(count, heatmapMax) }}
                            />
                          </TooltipTrigger>
                          <TooltipContent>
                            <div className="font-semibold text-[var(--ink-primary)]">
                              {day} &middot; {String(h).padStart(2, '0')}:00–
                              {String((h + 1) % 24).padStart(2, '0')}:00
                            </div>
                            <div className="text-[var(--accent-primary)]">
                              {count} {count === 1 ? 'play' : 'plays'}
                            </div>
                          </TooltipContent>
                        </Tooltip>
                      );
                    })}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Hourly & Weekday Charts */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Plays by Hour</CardTitle>
            <CardDescription>Distribution across 24 hours (local time)</CardDescription>
          </CardHeader>
          <CardContent className="h-56">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={hourlyData} margin={{ top: 4, right: 8, left: -24, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="var(--chart-grid)" vertical={false} />
                <XAxis
                  dataKey="hour"
                  tick={{ fill: 'var(--chart-axis)', fontSize: 10, fontFamily: 'var(--font-mono)' }}
                  axisLine={{ stroke: 'var(--chart-grid)' }}
                  tickLine={false}
                  interval={2}
                />
                <YAxis
                  tick={{ fill: 'var(--chart-axis)', fontSize: 10, fontFamily: 'var(--font-mono)' }}
                  axisLine={false}
                  tickLine={false}
                  allowDecimals={false}
                />
                <RechartsTooltip
                  cursor={{ fill: 'var(--bg-hover)', opacity: 0.5 }}
                  content={<ChartTooltip />}
                />
                <Bar dataKey="plays" fill="var(--chart-1)" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Plays by Weekday</CardTitle>
            <CardDescription>Monday through Sunday listening volume</CardDescription>
          </CardHeader>
          <CardContent className="h-56">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={weekdayData} margin={{ top: 4, right: 8, left: -24, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="var(--chart-grid)" vertical={false} />
                <XAxis
                  dataKey="day"
                  tick={{ fill: 'var(--chart-axis)', fontSize: 10, fontFamily: 'var(--font-mono)' }}
                  axisLine={{ stroke: 'var(--chart-grid)' }}
                  tickLine={false}
                />
                <YAxis
                  tick={{ fill: 'var(--chart-axis)', fontSize: 10, fontFamily: 'var(--font-mono)' }}
                  axisLine={false}
                  tickLine={false}
                  allowDecimals={false}
                />
                <RechartsTooltip
                  cursor={{ fill: 'var(--bg-hover)', opacity: 0.5 }}
                  content={<ChartTooltip />}
                />
                <Bar dataKey="plays" fill="var(--chart-2)" radius={[4, 4, 0, 0]} maxBarSize={42} />
              </BarChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      </div>

      {/* Top Tracks & Top Artists */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Top Tracks</CardTitle>
            <CardDescription>Most played songs in the selected timeframe</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {stats.top_tracks.length === 0 ? (
              <p className="text-xs text-[var(--ink-muted)]">No tracks yet.</p>
            ) : (
              stats.top_tracks.map(([artist, track, plays], idx) => {
                const match = trackCoverLookup.get(
                  `${artist.toLowerCase()}||${track.toLowerCase()}`,
                );
                return (
                  <div key={`${artist}-${track}-${idx}`} className="space-y-1.5">
                    <div className="flex items-center justify-between gap-3 text-xs">
                      <div className="flex min-w-0 items-center gap-2.5">
                        <span className="w-5 shrink-0 font-mono text-[10px] text-[var(--ink-muted)]">
                          {String(idx + 1).padStart(2, '0')}
                        </span>
                        {match?.coverUrl ? (
                          <img
                            src={match.coverUrl}
                            alt={track}
                            loading="lazy"
                            className="h-8 w-8 shrink-0 rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] object-cover"
                          />
                        ) : (
                          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] text-[var(--ink-muted)]">
                            <Music className="h-3.5 w-3.5" />
                          </div>
                        )}
                        <div className="min-w-0">
                          {match?.spotifyUrl ? (
                            <a
                              href={match.spotifyUrl}
                              target="_blank"
                              rel="noreferrer"
                              className="block truncate font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)]"
                            >
                              {track}
                            </a>
                          ) : (
                            <div className="truncate font-medium text-[var(--ink-primary)]">
                              {track}
                            </div>
                          )}
                          <div className="truncate text-[11px] text-[var(--ink-muted)]">
                            {artist}
                          </div>
                        </div>
                      </div>
                      <span className="shrink-0 font-mono text-[11px] font-semibold text-[var(--accent-primary)] tabular-nums">
                        {plays}
                      </span>
                    </div>
                    <div className="h-1.5 w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--bg-inset)]">
                      <div
                        className="h-full rounded-[var(--radius-pill)] bg-[var(--accent-primary)] transition-[width] duration-300 ease-out"
                        style={{ width: `${Math.max(4, (plays / maxTopTrack) * 100)}%` }}
                      />
                    </div>
                  </div>
                );
              })
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Top Artists</CardTitle>
            <CardDescription>Highest rotation artists</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {stats.top_artists.length === 0 ? (
              <p className="text-xs text-[var(--ink-muted)]">No artists yet.</p>
            ) : (
              stats.top_artists.map(([artist, plays], idx) => (
                <div key={`${artist}-${idx}`} className="space-y-1.5">
                  <div className="flex items-center justify-between gap-3 text-xs">
                    <div className="flex min-w-0 items-center gap-2.5">
                      <span className="w-5 shrink-0 font-mono text-[10px] text-[var(--ink-muted)]">
                        {String(idx + 1).padStart(2, '0')}
                      </span>
                      <span className="truncate font-medium text-[var(--ink-primary)]">
                        {artist}
                      </span>
                    </div>
                    <span className="shrink-0 font-mono text-[11px] font-semibold text-[var(--accent-primary)] tabular-nums">
                      {plays}
                    </span>
                  </div>
                  <div className="h-1.5 w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--bg-inset)]">
                    <div
                      className="h-full rounded-[var(--radius-pill)] bg-[var(--accent-muted)] transition-[width] duration-300 ease-out"
                      style={{ width: `${Math.max(4, (plays / maxTopArtist) * 100)}%` }}
                    />
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      </div>

      {/* Context Breakdown & Release Decades */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Context Breakdown</CardTitle>
            <CardDescription>Playlists, albums, and Liked Songs</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {stats.context_breakdown.map(([ctx, plays]) => (
              <div key={ctx} className="space-y-1.5">
                <div className="flex items-center justify-between text-xs">
                  <span className="font-medium text-[var(--ink-primary)]">
                    {formatContextLabel(ctx)}
                  </span>
                  <span className="font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                    {plays} ({Math.round((plays / stats.total_plays) * 100)}%)
                  </span>
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--bg-inset)]">
                  <div
                    className="h-full rounded-[var(--radius-pill)] bg-[var(--accent-muted)] transition-[width] duration-300 ease-out"
                    style={{ width: `${Math.max(3, (plays / maxContext) * 100)}%` }}
                  />
                </div>
              </div>
            ))}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div>
                <CardTitle>Release Decades</CardTitle>
                <CardDescription>Era distribution of played tracks</CardDescription>
              </div>
              {deepCut && (
                <InfoTooltip
                  content="Least popular track listened to in this period (lowest Spotify 0–100 score)"
                  side="top"
                >
                  <Badge variant="default" className="gap-1">
                    <Sparkles className="h-3 w-3" />
                    Deep Cut · POP {deepCut[2]}
                  </Badge>
                </InfoTooltip>
              )}
            </div>
            {deepCut && (
              <div className="mt-2 flex items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2.5 py-1.5 text-xs">
                <Sparkles className="h-3.5 w-3.5 shrink-0 text-[var(--accent-primary)]" />
                <span className="truncate font-medium text-[var(--ink-primary)]">{deepCut[1]}</span>
                <span className="text-[var(--ink-muted)]">&mdash;</span>
                <span className="truncate text-[var(--ink-secondary)]">{deepCut[0]}</span>
              </div>
            )}
          </CardHeader>
          <CardContent className="space-y-3">
            {stats.release_decades.length === 0 ? (
              <p className="text-xs text-[var(--ink-muted)]">No release metadata available yet.</p>
            ) : (
              stats.release_decades.map(([decade, plays]) => (
                <div key={decade} className="space-y-1.5">
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-mono font-medium text-[var(--ink-primary)]">
                      {decade}
                    </span>
                    <span className="font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                      {plays}
                    </span>
                  </div>
                  <div className="h-1.5 w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--bg-inset)]">
                    <div
                      className="h-full rounded-[var(--radius-pill)] bg-[var(--accent-primary)] transition-[width] duration-300 ease-out"
                      style={{ width: `${Math.max(3, (plays / maxDecade) * 100)}%` }}
                    />
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      </div>

      {/* Listeners Leaderboard & Recent Listens */}
      {(friendsSummary.length > 0 || recentListens.length > 0) && (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {friendsSummary.length > 0 && (
            <Card>
              <CardHeader className="flex-row items-center justify-between space-y-0">
                <div>
                  <CardTitle>Listeners</CardTitle>
                  <CardDescription>Click a listener to filter analytics</CardDescription>
                </div>
                <Users className="h-4 w-4 text-[var(--accent-primary)]" />
              </CardHeader>
              <CardContent className="space-y-2">
                {friendsSummary.slice(0, 8).map((friend) => {
                  const isActive = selectedSet.has(friend.user_uri.toLowerCase());
                  const avatarUrl =
                    friend.image_url ??
                    lookupAvatar(avatarMap, friend.user_uri, friend.user_name);

                  return (
                    <button
                      key={friend.user_uri}
                      type="button"
                      onClick={() => onFilterByUser?.(friend.user_uri)}
                      className={cn(
                        'flex w-full items-center justify-between gap-3 rounded-[var(--radius-control)] border px-3 py-2 text-left transition-colors cursor-pointer',
                        isActive
                          ? 'border-[var(--border-accent)] bg-[var(--accent-soft)]'
                          : 'border-[var(--border-subtle)] bg-[var(--bg-inset)] hover:border-[var(--border-strong)] hover:bg-[var(--bg-hover)]',
                      )}
                    >
                      <div className="flex min-w-0 items-center gap-2.5">
                        <UserAvatar
                          name={friend.user_name}
                          imageUrl={avatarUrl}
                          size="sm"
                          isLive={friend.is_live}
                          isSelf={friend.is_self}
                        />
                        <div className="min-w-0">
                          <div className="flex items-center gap-1.5">
                            <span className="truncate text-xs font-semibold text-[var(--ink-primary)]">
                              {friend.user_name}
                            </span>
                            {friend.is_self && (
                              <Badge variant="default" className="px-1.5 py-0 text-[9px]">
                                YOU
                              </Badge>
                            )}
                          </div>
                          <div className="truncate text-[11px] text-[var(--ink-muted)]">
                            {friend.latest_track_name} &middot; {friend.latest_artist_name}
                          </div>
                        </div>
                      </div>
                      <div className="shrink-0 text-right">
                        <div className="font-mono text-xs font-semibold text-[var(--accent-primary)] tabular-nums">
                          {friend.total_plays.toLocaleString()}
                        </div>
                        <div className="font-mono text-[10px] text-[var(--ink-muted)]">
                          {formatRelativeTime(friend.last_active_ms)}
                        </div>
                      </div>
                    </button>
                  );
                })}
              </CardContent>
            </Card>
          )}

          {recentListens.length > 0 && (
            <Card>
              <CardHeader>
                <CardTitle>Recent Listens</CardTitle>
                <CardDescription>Latest recorded plays in this scope</CardDescription>
              </CardHeader>
              <CardContent className="space-y-2">
                {recentListens.slice(0, 8).map((listen) => {
                  const avatarUrl =
                    listen.user_image_url ??
                    lookupAvatar(avatarMap, listen.user_uri, listen.user_name);
                  const contextLabel =
                    listen.context_name || formatContextLabel(listen.context_type);
                  const canOpenPlaylist = Boolean(listen.context_uri && onSelectPlaylist);

                  return (
                    <div
                      key={listen.id}
                      className="flex items-center justify-between gap-3 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-3 py-2 transition-colors hover:border-[var(--border-strong)]"
                    >
                      <div className="flex min-w-0 items-center gap-2.5">
                        {listen.cover_url ? (
                          <img
                            src={listen.cover_url}
                            alt={listen.track_name}
                            loading="lazy"
                            className="h-9 w-9 shrink-0 rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] object-cover"
                          />
                        ) : (
                          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[calc(var(--radius-control)*0.6)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] text-[var(--ink-muted)]">
                            <Music className="h-4 w-4" />
                          </div>
                        )}
                        <div className="min-w-0">
                          <a
                            href={listen.spotify_url}
                            target="_blank"
                            rel="noreferrer"
                            className="inline-flex max-w-full items-center gap-1 text-xs font-semibold text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)]"
                          >
                            <span className="truncate">{listen.track_name}</span>
                            <ExternalLink className="h-3 w-3 shrink-0 opacity-50" />
                          </a>
                          <div className="truncate text-[11px] text-[var(--ink-muted)]">
                            {listen.all_artists || listen.artist_name}
                          </div>
                          <div className="mt-1 flex flex-wrap items-center gap-2">
                            <button
                              type="button"
                              onClick={() => onFilterByUser?.(listen.user_uri)}
                              className="inline-flex items-center gap-1 text-[10px] font-medium text-[var(--ink-secondary)] transition-colors hover:text-[var(--accent-primary)] cursor-pointer"
                            >
                              <UserAvatar
                                name={listen.user_name}
                                imageUrl={avatarUrl}
                                size="xs"
                              />
                              <span className="truncate max-w-[100px]">{listen.user_name}</span>
                            </button>
                            {canOpenPlaylist && listen.context_uri && (
                              <button
                                type="button"
                                onClick={() => onSelectPlaylist?.(listen.context_uri!)}
                                className="inline-flex max-w-[140px] items-center gap-1 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-1.5 py-0.5 text-[10px] text-[var(--ink-muted)] transition-colors hover:border-[var(--border-accent)] hover:text-[var(--accent-primary)] cursor-pointer"
                                title="Open playlist in Playlists tab"
                              >
                                <PlaylistCover
                                  name={contextLabel}
                                  imageUrl={listen.context_image_url}
                                  contextType={listen.context_type}
                                  isPrivate={listen.context_is_private}
                                  size="xs"
                                />
                                <span className="truncate">{contextLabel}</span>
                              </button>
                            )}
                          </div>
                        </div>
                      </div>
                      <span className="shrink-0 font-mono text-[10px] text-[var(--ink-muted)]">
                        {formatRelativeTime(listen.played_at_ms)}
                      </span>
                    </div>
                  );
                })}
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}
