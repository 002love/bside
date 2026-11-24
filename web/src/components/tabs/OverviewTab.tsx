import { useMemo, type ReactNode } from 'react';
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

const WEEKDAYS = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
const HOURS = Array.from({ length: 24 }, (_, i) => i);

interface OverviewTabProps {
  data: OverviewResponse | null;
  loading: boolean;
  liveStrip?: ReactNode;
}

function heatCellClass(count: number, max: number): string {
  if (count <= 0 || max <= 0) return 'bg-[#141412] border-[#242420]';
  const ratio = count / max;
  if (ratio < 0.25) return 'bg-[#263420] border-[#33462a]';
  if (ratio < 0.5) return 'bg-[#3d5432] border-[#4b673d]';
  if (ratio < 0.75) return 'bg-[#557445] border-[#6a8f57]';
  return 'bg-[#89b471] border-[#a3ca8d]';
}

export function OverviewTab({ data, loading, liveStrip }: OverviewTabProps) {
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

  if (loading && !stats) {
    return (
      <div className="space-y-5">
        {liveStrip}
        <div className="grid grid-cols-1 gap-3.5 md:grid-cols-5">
          {Array.from({ length: 5 }).map((_, i) => (
            <Card key={i} className="h-28 animate-pulse bg-[#181816]" />
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

  return (
    <div className="space-y-6">
      {/* Compact Collapsible On Air Strip */}
      {liveStrip}

      {/* 01 // 5 KPI Cards */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          01 // KEY TELEMETRY
        </div>
        <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-5">
          <Card className="shadow-edge">
            <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
                TOTAL PLAYS
              </span>
              <Activity className="h-3.5 w-3.5 text-[#89b471]" />
            </CardHeader>
            <CardContent>
              <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[#efeee8] tabular-nums">
                {stats.total_plays.toLocaleString()}
              </div>
              <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                {stats.active_days} {stats.active_days === 1 ? 'DAY' : 'DAYS'} &middot;{' '}
                {stats.session_count} SESSIONS
              </p>
            </CardContent>
          </Card>

          <Card className="shadow-edge">
            <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
                LISTENING TIME
              </span>
              <Clock className="h-3.5 w-3.5 text-[#89b471]" />
            </CardHeader>
            <CardContent>
              <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[#efeee8] tabular-nums">
                {formatTotalTime(stats.total_listened_ms).toUpperCase()}
              </div>
              <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                AVG SESSION:{' '}
                {stats.avg_session_ms ? formatTotalTime(stats.avg_session_ms).toUpperCase() : 'N/A'}
              </p>
            </CardContent>
          </Card>

          <Card className="shadow-edge">
            <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
                UNIQUE CATALOG
              </span>
              <Disc3 className="h-3.5 w-3.5 text-[#89b471]" />
            </CardHeader>
            <CardContent>
              <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[#efeee8] tabular-nums">
                {stats.unique_tracks.toLocaleString()}
              </div>
              <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                TRACKS / {stats.unique_artists.toLocaleString()} ARTISTS
              </p>
            </CardContent>
          </Card>

          <Card className="shadow-edge">
            <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
                SKIP RATE
              </span>
              <SkipForward
                className={
                  skipRate != null && skipRate >= 35
                    ? 'h-3.5 w-3.5 text-[#c48a47]'
                    : 'h-3.5 w-3.5 text-[#89b471]'
                }
              />
            </CardHeader>
            <CardContent>
              <div className="font-display text-4xl font-extrabold leading-none tracking-tight text-[#efeee8] tabular-nums">
                {skipRate != null ? `${skipRate.toFixed(1)}%` : '—'}
              </div>
              <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                {stats.skipped_plays} SKIP / {stats.completed_plays} FULL
              </p>
            </CardContent>
          </Card>

          <Card className="shadow-edge">
            <CardHeader className="flex-row items-center justify-between space-y-0 pb-2">
              <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
                POPULARITY & STREAK
              </span>
              <Flame className="h-3.5 w-3.5 text-[#89b471]" />
            </CardHeader>
            <CardContent>
              <div className="flex items-baseline gap-1.5">
                <span className="font-display text-4xl font-extrabold leading-none tracking-tight text-[#efeee8] tabular-nums">
                  {stats.avg_popularity != null ? Math.round(stats.avg_popularity) : '—'}
                </span>
                <span className="font-mono text-[10px] text-[#8d8c84]">/100</span>
                <Badge variant="default" className="ml-auto">
                  {stats.current_streak_days}D STREAK
                </Badge>
              </div>
              <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                BEST STREAK: {stats.longest_streak_days}D
              </p>
            </CardContent>
          </Card>
        </div>
      </section>

      {/* 02 // 7x24 Activity Matrix */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          02 // ACTIVITY MATRIX
        </div>
        <Card>
          <CardHeader className="flex-row flex-wrap items-center justify-between gap-2 pb-3">
            <div>
              <CardTitle>7&times;24 Listening Heatmap</CardTitle>
              <CardDescription>
                Local listening cadence across days of the week and hours of the day
              </CardDescription>
            </div>
            <div className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
              <span>LESS</span>
              <span className="h-3 w-3 rounded-none border border-[#242420] bg-[#141412]" />
              <span className="h-3 w-3 rounded-none border border-[#33462a] bg-[#263420]" />
              <span className="h-3 w-3 rounded-none border border-[#4b673d] bg-[#3d5432]" />
              <span className="h-3 w-3 rounded-none border border-[#6a8f57] bg-[#557445]" />
              <span className="h-3 w-3 rounded-none border border-[#a3ca8d] bg-[#89b471]" />
              <span>MORE</span>
            </div>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto">
              <div className="min-w-[640px]">
                <div className="mb-1.5 grid grid-cols-[44px_repeat(24,minmax(0,1fr))] gap-1 font-mono text-[10px] text-[#8d8c84]">
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
                      <span className="font-mono text-[10px] font-semibold text-[#8d8c84]">{day}</span>
                      {HOURS.map((h) => {
                        const count = stats.heatmap_grid[dIdx]?.[h] ?? 0;
                        return (
                          <Tooltip key={h}>
                            <TooltipTrigger asChild>
                              <div
                                className={`h-5 rounded-none border transition-colors hover:border-[#efeee8] ${heatCellClass(count, heatmapMax)}`}
                              />
                            </TooltipTrigger>
                            <TooltipContent>
                              <div className="font-semibold text-[#efeee8]">
                                {day} &middot; {String(h).padStart(2, '0')}:00–
                                {String((h + 1) % 24).padStart(2, '0')}:00
                              </div>
                              <div className="text-[#89b471]">
                                {count} {count === 1 ? 'PLAY' : 'PLAYS'}
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
      </section>

      {/* 03 // Hourly & Weekday Charts */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          03 // TEMPORAL DISTRIBUTION
        </div>
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle>Plays by Hour</CardTitle>
              <CardDescription>Distribution across 24 hours (local time)</CardDescription>
            </CardHeader>
            <CardContent className="h-56">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={hourlyData} margin={{ top: 4, right: 8, left: -24, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="2 2" stroke="#2b2b27" vertical={false} />
                  <XAxis
                    dataKey="hour"
                    tick={{ fill: '#8d8c84', fontSize: 10, fontFamily: 'IBM Plex Mono, monospace' }}
                    axisLine={{ stroke: '#2b2b27' }}
                    tickLine={false}
                    interval={2}
                  />
                  <YAxis
                    tick={{ fill: '#8d8c84', fontSize: 10, fontFamily: 'IBM Plex Mono, monospace' }}
                    axisLine={false}
                    tickLine={false}
                    allowDecimals={false}
                  />
                  <RechartsTooltip
                    cursor={{ fill: 'rgba(239,238,232,0.05)' }}
                    contentStyle={{
                      backgroundColor: '#181816',
                      border: '1px solid #45453e',
                      borderRadius: '0px',
                      fontSize: '11px',
                      fontFamily: 'IBM Plex Mono, monospace',
                      color: '#efeee8',
                    }}
                  />
                  <Bar dataKey="plays" fill="#89b471" radius={[0, 0, 0, 0]} />
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
                  <CartesianGrid strokeDasharray="2 2" stroke="#2b2b27" vertical={false} />
                  <XAxis
                    dataKey="day"
                    tick={{ fill: '#8d8c84', fontSize: 10, fontFamily: 'IBM Plex Mono, monospace' }}
                    axisLine={{ stroke: '#2b2b27' }}
                    tickLine={false}
                  />
                  <YAxis
                    tick={{ fill: '#8d8c84', fontSize: 10, fontFamily: 'IBM Plex Mono, monospace' }}
                    axisLine={false}
                    tickLine={false}
                    allowDecimals={false}
                  />
                  <RechartsTooltip
                    cursor={{ fill: 'rgba(239,238,232,0.05)' }}
                    contentStyle={{
                      backgroundColor: '#181816',
                      border: '1px solid #45453e',
                      borderRadius: '0px',
                      fontSize: '11px',
                      fontFamily: 'IBM Plex Mono, monospace',
                      color: '#efeee8',
                    }}
                  />
                  <Bar dataKey="plays" fill="#557445" radius={[0, 0, 0, 0]} maxBarSize={42} />
                </BarChart>
              </ResponsiveContainer>
            </CardContent>
          </Card>
        </div>
      </section>

      {/* 04 // Top Tracks & Top Artists */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          04 // ROTATION CHARTS
        </div>
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle>Top Tracks</CardTitle>
              <CardDescription>Most played songs in the selected timeframe</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {stats.top_tracks.length === 0 ? (
                <p className="font-mono text-xs text-[#8d8c84]">No tracks yet.</p>
              ) : (
                stats.top_tracks.map(([artist, track, plays], idx) => (
                  <div key={`${artist}-${track}-${idx}`} className="space-y-1.5">
                    <div className="flex items-center justify-between gap-3 text-xs">
                      <div className="flex min-w-0 items-center gap-2.5">
                        <span className="w-5 shrink-0 font-mono text-[10px] text-[#8d8c84]">
                          {String(idx + 1).padStart(2, '0')}
                        </span>
                        <div className="min-w-0">
                          <div className="truncate font-medium text-[#efeee8]">{track}</div>
                          <div className="truncate font-mono text-[10px] text-[#8d8c84]">{artist}</div>
                        </div>
                      </div>
                      <span className="shrink-0 font-mono text-[11px] font-semibold text-[#89b471] tabular-nums">
                        {plays}
                      </span>
                    </div>
                    <div className="h-1 w-full bg-[#2b2b27]">
                      <div
                        className="h-full bg-[#89b471]"
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
                <p className="font-mono text-xs text-[#8d8c84]">No artists yet.</p>
              ) : (
                stats.top_artists.map(([artist, plays], idx) => (
                  <div key={`${artist}-${idx}`} className="space-y-1.5">
                    <div className="flex items-center justify-between gap-3 text-xs">
                      <div className="flex min-w-0 items-center gap-2.5">
                        <span className="w-5 shrink-0 font-mono text-[10px] text-[#8d8c84]">
                          {String(idx + 1).padStart(2, '0')}
                        </span>
                        <span className="truncate font-medium text-[#efeee8]">{artist}</span>
                      </div>
                      <span className="shrink-0 font-mono text-[11px] font-semibold text-[#89b471] tabular-nums">
                        {plays}
                      </span>
                    </div>
                    <div className="h-1 w-full bg-[#2b2b27]">
                      <div
                        className="h-full bg-[#557445]"
                        style={{ width: `${Math.max(4, (plays / maxTopArtist) * 100)}%` }}
                      />
                    </div>
                  </div>
                ))
              )}
            </CardContent>
          </Card>
        </div>
      </section>

      {/* 05 // Context Breakdown & Release Decades */}
      <section className="space-y-2.5">
        <div className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
          05 // CONTEXT & ERA PROFILE
        </div>
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle>Context Breakdown</CardTitle>
              <CardDescription>Where listens originated (playlists, albums, Liked Songs)</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {stats.context_breakdown.map(([ctx, plays]) => (
                <div key={ctx} className="space-y-1.5">
                  <div className="flex items-center justify-between font-mono text-xs">
                    <span className="uppercase text-[#efeee8]">{formatContextLabel(ctx)}</span>
                    <span className="text-[11px] text-[#8d8c84] tabular-nums">
                      {plays} ({Math.round((plays / stats.total_plays) * 100)}%)
                    </span>
                  </div>
                  <div className="h-1.5 w-full bg-[#2b2b27]">
                    <div
                      className="h-full bg-[#557445]"
                      style={{ width: `${Math.max(3, (plays / maxContext) * 100)}%` }}
                    />
                  </div>
                </div>
              ))}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <CardTitle>Release Decades</CardTitle>
                  <CardDescription>Era distribution of played tracks</CardDescription>
                </div>
                {stats.most_obscure_track && (
                  <Badge variant="default" className="gap-1">
                    <Sparkles className="h-3 w-3" />
                    DEEP CUT: POP {stats.most_obscure_track[2]}
                  </Badge>
                )}
              </div>
            </CardHeader>
            <CardContent className="space-y-3">
              {stats.release_decades.length === 0 ? (
                <p className="font-mono text-xs text-[#8d8c84]">No release metadata available yet.</p>
              ) : (
                stats.release_decades.map(([decade, plays]) => (
                  <div key={decade} className="space-y-1.5">
                    <div className="flex items-center justify-between font-mono text-xs">
                      <span className="text-[#efeee8]">{decade}</span>
                      <span className="text-[11px] text-[#8d8c84] tabular-nums">{plays}</span>
                    </div>
                    <div className="h-1.5 w-full bg-[#2b2b27]">
                      <div
                        className="h-full bg-[#89b471]"
                        style={{ width: `${Math.max(3, (plays / maxDecade) * 100)}%` }}
                      />
                    </div>
                  </div>
                ))
              )}
            </CardContent>
          </Card>
        </div>
      </section>
    </div>
  );
}
