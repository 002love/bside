import { useMemo } from 'react';
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
  Flame,
  SkipForward,
  Sparkles,
} from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { EmptyState } from '@/components/EmptyState';
import { formatContextLabel, formatTotalTime } from '@/lib/utils';
import type { OverviewResponse } from '@/types/api';

const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
const HOURS = Array.from({ length: 24 }, (_, i) => i);

interface OverviewTabProps {
  data: OverviewResponse | null;
  loading: boolean;
}

function heatCellClass(count: number, max: number): string {
  if (count <= 0 || max <= 0) return 'bg-zinc-900/90 border-zinc-800/70';
  const ratio = count / max;
  if (ratio < 0.25) return 'bg-emerald-950/90 border-emerald-900/80';
  if (ratio < 0.5) return 'bg-emerald-800/75 border-emerald-700/70';
  if (ratio < 0.75) return 'bg-emerald-600/85 border-emerald-500/70';
  return 'bg-emerald-400 border-emerald-300/80';
}

export function OverviewTab({ data, loading }: OverviewTabProps) {
  const stats = data?.stats;

  const hourlyData = useMemo(() => {
    if (!stats) return [];
    return stats.hourly.map((plays, hour) => ({
      hour: `${String(hour).padStart(2, '0')}h`,
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

  if (loading && !stats) {
    return (
      <div className="grid grid-cols-1 gap-4 md:grid-cols-5">
        {Array.from({ length: 5 }).map((_, i) => (
          <Card key={i} className="h-28 animate-pulse bg-zinc-900/40" />
        ))}
      </div>
    );
  }

  if (!stats || stats.total_plays === 0) {
    return (
      <EmptyState
        title="No listening activity recorded yet"
        description="Start polling Spotify Friend Activity to populate your local SQLite database with plays, skips, repeats, and listening sessions."
        command="bside watch"
      />
    );
  }

  const judgedPlays = stats.skipped_plays + stats.completed_plays;
  const skipRate = judgedPlays > 0 ? (stats.skipped_plays / judgedPlays) * 100 : null;
  const maxTopTrack = stats.top_tracks[0]?.[2] ?? 1;
  const maxTopArtist = stats.top_artists[0]?.[1] ?? 1;
  const maxContext = stats.context_breakdown[0]?.[1] ?? 1;
  const maxDecade = stats.release_decades[0]?.[1] ?? 1;

  return (
    <div className="space-y-5">
      {/* 5 KPI Cards */}
      <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-5">
        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-medium text-zinc-400">Total Plays</CardTitle>
            <Activity className="h-3.5 w-3.5 text-emerald-400" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold tracking-tight text-zinc-50 tabular-nums">
              {stats.total_plays.toLocaleString()}
            </div>
            <p className="mt-1 text-[11px] text-zinc-500">
              {stats.active_days} active {stats.active_days === 1 ? 'day' : 'days'} &middot;{' '}
              {stats.session_count} sessions
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-medium text-zinc-400">Listening Time</CardTitle>
            <Clock className="h-3.5 w-3.5 text-emerald-400" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold tracking-tight text-zinc-50 tabular-nums">
              {formatTotalTime(stats.total_listened_ms)}
            </div>
            <p className="mt-1 text-[11px] text-zinc-500">
              Avg session:{' '}
              {stats.avg_session_ms ? formatTotalTime(stats.avg_session_ms) : 'n/a'}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-medium text-zinc-400">Unique Catalog</CardTitle>
            <Disc3 className="h-3.5 w-3.5 text-emerald-400" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold tracking-tight text-zinc-50 tabular-nums">
              {stats.unique_tracks.toLocaleString()}
            </div>
            <p className="mt-1 text-[11px] text-zinc-500">
              tracks across {stats.unique_artists.toLocaleString()} artists
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-medium text-zinc-400">Skip Rate</CardTitle>
            <SkipForward className="h-3.5 w-3.5 text-emerald-400" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold tracking-tight text-zinc-50 tabular-nums">
              {skipRate != null ? `${skipRate.toFixed(1)}%` : '—'}
            </div>
            <p className="mt-1 text-[11px] text-zinc-500">
              {stats.skipped_plays} skipped / {stats.completed_plays} completed
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-medium text-zinc-400">
              Popularity & Streak
            </CardTitle>
            <Flame className="h-3.5 w-3.5 text-emerald-400" />
          </CardHeader>
          <CardContent>
            <div className="flex items-baseline gap-2">
              <span className="text-2xl font-semibold tracking-tight text-zinc-50 tabular-nums">
                {stats.avg_popularity != null ? Math.round(stats.avg_popularity) : '—'}
              </span>
              <span className="text-xs text-zinc-500">/ 100</span>
              <Badge variant="default" className="ml-auto">
                {stats.current_streak_days}d streak
              </Badge>
            </div>
            <p className="mt-1 text-[11px] text-zinc-500">
              Longest streak: {stats.longest_streak_days}d
            </p>
          </CardContent>
        </Card>
      </div>

      {/* 7x24 Activity Heatmap */}
      <Card>
        <CardHeader className="flex-row flex-wrap items-center justify-between gap-2 pb-3">
          <div>
            <CardTitle>Activity Heatmap</CardTitle>
            <CardDescription>
              Local listening cadence across days of the week and hours of the day
            </CardDescription>
          </div>
          <div className="flex items-center gap-1.5 text-[11px] text-zinc-400">
            <span>Less</span>
            <span className="h-3 w-3 rounded-[3px] border border-zinc-800 bg-zinc-900/90" />
            <span className="h-3 w-3 rounded-[3px] border border-emerald-900/80 bg-emerald-950/90" />
            <span className="h-3 w-3 rounded-[3px] border border-emerald-700/70 bg-emerald-800/75" />
            <span className="h-3 w-3 rounded-[3px] border border-emerald-500/70 bg-emerald-600/85" />
            <span className="h-3 w-3 rounded-[3px] border border-emerald-300/80 bg-emerald-400" />
            <span>More</span>
          </div>
        </CardHeader>
        <CardContent>
          <div className="overflow-x-auto">
            <div className="min-w-[640px]">
              <div className="mb-1.5 grid grid-cols-[44px_repeat(24,minmax(0,1fr))] gap-1 text-[10px] text-zinc-500">
                <div />
                {HOURS.map((h) => (
                  <div key={h} className="text-center font-mono">
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
                    <span className="text-[11px] font-medium text-zinc-400">{day}</span>
                    {HOURS.map((h) => {
                      const count = stats.heatmap_grid[dIdx]?.[h] ?? 0;
                      return (
                        <Tooltip key={h}>
                          <TooltipTrigger asChild>
                            <div
                              className={`h-5 rounded-[4px] border transition-transform hover:scale-110 ${heatCellClass(count, heatmapMax)}`}
                            />
                          </TooltipTrigger>
                          <TooltipContent>
                            <div className="font-medium text-zinc-100">
                              {day} &middot; {String(h).padStart(2, '0')}:00–
                              {String((h + 1) % 24).padStart(2, '0')}:00
                            </div>
                            <div className="text-emerald-400">
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
                <CartesianGrid strokeDasharray="3 3" stroke="#27272a" vertical={false} />
                <XAxis
                  dataKey="hour"
                  tick={{ fill: '#71717a', fontSize: 10 }}
                  axisLine={false}
                  tickLine={false}
                  interval={2}
                />
                <YAxis
                  tick={{ fill: '#71717a', fontSize: 10 }}
                  axisLine={false}
                  tickLine={false}
                  allowDecimals={false}
                />
                <RechartsTooltip
                  cursor={{ fill: 'rgba(255,255,255,0.04)' }}
                  contentStyle={{
                    backgroundColor: '#18181b',
                    border: '1px solid #27272a',
                    borderRadius: '8px',
                    fontSize: '12px',
                    color: '#f4f4f5',
                  }}
                />
                <Bar dataKey="plays" fill="#10b981" radius={[3, 3, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Plays by Weekday</CardTitle>
            <CardDescription>Monday through Sunday volume</CardDescription>
          </CardHeader>
          <CardContent className="h-56">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={weekdayData} margin={{ top: 4, right: 8, left: -24, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="#27272a" vertical={false} />
                <XAxis
                  dataKey="day"
                  tick={{ fill: '#71717a', fontSize: 11 }}
                  axisLine={false}
                  tickLine={false}
                />
                <YAxis
                  tick={{ fill: '#71717a', fontSize: 10 }}
                  axisLine={false}
                  tickLine={false}
                  allowDecimals={false}
                />
                <RechartsTooltip
                  cursor={{ fill: 'rgba(255,255,255,0.04)' }}
                  contentStyle={{
                    backgroundColor: '#18181b',
                    border: '1px solid #27272a',
                    borderRadius: '8px',
                    fontSize: '12px',
                    color: '#f4f4f5',
                  }}
                />
                <Bar dataKey="plays" fill="#10b981" radius={[4, 4, 0, 0]} maxBarSize={42} />
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
              <p className="text-xs text-zinc-500">No tracks yet.</p>
            ) : (
              stats.top_tracks.map(([artist, track, plays], idx) => (
                <div key={`${artist}-${track}-${idx}`} className="space-y-1">
                  <div className="flex items-center justify-between gap-3 text-xs">
                    <div className="flex min-w-0 items-center gap-2.5">
                      <span className="w-4 shrink-0 font-mono text-[11px] text-zinc-500">
                        {idx + 1}
                      </span>
                      <div className="min-w-0">
                        <div className="truncate font-medium text-zinc-100">{track}</div>
                        <div className="truncate text-[11px] text-zinc-400">{artist}</div>
                      </div>
                    </div>
                    <span className="shrink-0 font-mono text-[11px] text-zinc-400">
                      {plays}
                    </span>
                  </div>
                  <div className="h-1 w-full overflow-hidden rounded-full bg-zinc-800/80">
                    <div
                      className="h-full rounded-full bg-emerald-500/85"
                      style={{ width: `${Math.max(4, (plays / maxTopTrack) * 100)}%` }}
                    />
                  </div>
                </div>
              ))
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
              <p className="text-xs text-zinc-500">No artists yet.</p>
            ) : (
              stats.top_artists.map(([artist, plays], idx) => (
                <div key={`${artist}-${idx}`} className="space-y-1">
                  <div className="flex items-center justify-between gap-3 text-xs">
                    <div className="flex min-w-0 items-center gap-2.5">
                      <span className="w-4 shrink-0 font-mono text-[11px] text-zinc-500">
                        {idx + 1}
                      </span>
                      <span className="truncate font-medium text-zinc-100">{artist}</span>
                    </div>
                    <span className="shrink-0 font-mono text-[11px] text-zinc-400">
                      {plays}
                    </span>
                  </div>
                  <div className="h-1 w-full overflow-hidden rounded-full bg-zinc-800/80">
                    <div
                      className="h-full rounded-full bg-emerald-500/85"
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
            <CardDescription>Where listens originated (playlists, albums, Liked Songs)</CardDescription>
          </CardHeader>
          <CardContent className="space-y-2.5">
            {stats.context_breakdown.map(([ctx, plays]) => (
              <div key={ctx} className="space-y-1">
                <div className="flex items-center justify-between text-xs">
                  <span className="capitalize text-zinc-200">{formatContextLabel(ctx)}</span>
                  <span className="font-mono text-[11px] text-zinc-400">
                    {plays} ({Math.round((plays / stats.total_plays) * 100)}%)
                  </span>
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800/80">
                  <div
                    className="h-full rounded-full bg-emerald-500/75"
                    style={{ width: `${Math.max(3, (plays / maxContext) * 100)}%` }}
                  />
                </div>
              </div>
            ))}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>Release Decades</CardTitle>
                <CardDescription>Era distribution of played tracks</CardDescription>
              </div>
              {stats.most_obscure_track && (
                <Badge variant="outline" className="gap-1">
                  <Sparkles className="h-3 w-3 text-emerald-400" />
                  Deep cut: {stats.most_obscure_track[2]}
                </Badge>
              )}
            </div>
          </CardHeader>
          <CardContent className="space-y-2.5">
            {stats.release_decades.length === 0 ? (
              <p className="text-xs text-zinc-500">No release metadata available yet.</p>
            ) : (
              stats.release_decades.map(([decade, plays]) => (
                <div key={decade} className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-mono text-zinc-200">{decade}</span>
                    <span className="font-mono text-[11px] text-zinc-400">{plays}</span>
                  </div>
                  <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800/80">
                    <div
                      className="h-full rounded-full bg-emerald-500/75"
                      style={{ width: `${Math.max(3, (plays / maxDecade) * 100)}%` }}
                    />
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
