import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import {
  Activity,
  Clock,
  Disc3,
  ExternalLink,
  Flame,
  Sparkles,
  Users,
} from 'lucide-react';
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip as RechartsTooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { EASE_OUT_EXPO, SPRING_SNAPPY, StaggerItem } from '@/components/ui/motion';
import { CoverArt } from '@/components/CoverArt';
import { EmptyState } from '@/components/EmptyState';
import { InfoTooltip } from '@/components/InfoTooltip';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { cn, formatContextLabel, formatRelativeTime, formatTotalTime } from '@/lib/utils';
import { buildUsersParam, type AvatarMap, type OverviewResponse } from '@/types/api';

const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
const HOURS = Array.from({ length: 24 }, (_, i) => i);

type RhythmMode = 'charts' | 'heatmap';

export interface OverviewTabProps {
  selectedUsers: string[];
  days: number | null;
  refreshTick: number;
  avatarMap: AvatarMap;
  liveStrip?: ReactNode;
  onSelectPlaylist?: (contextUri: string) => void;
  onFilterByUser?: (userUri: string) => void;
}

function heatCellColor(count: number, max: number): string {
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
    <div className="bside-popover px-3 py-2 text-xs">
      <div className="text-[11px] font-medium text-[var(--ink-secondary)]">{label}</div>
      <div className="mt-0.5 text-xs font-semibold text-white tabular-nums">
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
  const [rhythmMode, setRhythmMode] = useState<RhythmMode>('charts');

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
      hour: `${String(hour).padStart(2, '0')}:00`,
      shortHour: `${String(hour).padStart(2, '0')}`,
      plays,
    }));
  }, [stats]);

  const weekdayData = useMemo(() => {
    if (!stats) return [];
    return stats.weekday.map((plays, idx) => ({
      day: WEEKDAYS[idx] ?? `Day ${idx + 1}`,
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

  const timeframeLabel = days != null ? `Last ${days} days` : 'All recorded time';
  const scopeLabel =
    selectedUsers.length === 0
      ? 'Everyone in your circle'
      : selectedUsers.length === 1
        ? '1 listener selected'
        : `${selectedUsers.length} listeners selected`;

  if (loading && !stats) {
    return (
      <div className="w-full">
        <div className="px-6 pt-6 pb-5 lg:px-8">
          <div className="h-7 w-32 rounded bg-white/[0.07]" />
          <div className="mt-2 h-3.5 w-48 rounded bg-white/[0.04]" />
        </div>
        {liveStrip}
        <div className="px-6 lg:px-8">
          <div className="grid grid-cols-2 border-y border-white/[0.08] divide-y divide-white/[0.07] lg:grid-cols-4 lg:divide-y-0 lg:divide-x">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="px-5 py-6 first:pl-0">
                <div className="h-3 w-20 rounded bg-white/[0.05]" />
                <div className="mt-4 h-8 w-24 rounded bg-white/[0.07]" />
                <div className="mt-3 h-2.5 w-32 rounded bg-white/[0.04]" />
              </div>
            ))}
          </div>
        </div>
        <div className="px-6 py-8 lg:px-8">
          <div className="h-[240px] w-full rounded bg-white/[0.03]" />
        </div>
      </div>
    );
  }

  if (!stats || stats.total_plays === 0) {
    return (
      <div className="w-full">
        <div className="px-6 pt-6 pb-5 lg:px-8">
          <h1 className="text-2xl font-bold tracking-[-0.03em] text-white">Overview</h1>
          <p className="mt-1 text-xs text-[var(--ink-secondary)]">
            {timeframeLabel} · {scopeLabel}
          </p>
        </div>
        {liveStrip}
        <div className="px-6 py-8 lg:px-8">
          <EmptyState
            title="No listening activity recorded yet"
            description="Start polling Spotify Friend Activity to populate your local SQLite database with plays, skips, repeats, and listening sessions."
            command="bside watch"
          />
        </div>
      </div>
    );
  }

  const judgedPlays = stats.skipped_plays + stats.completed_plays;
  const completionRate = judgedPlays > 0 ? (stats.completed_plays / judgedPlays) * 100 : null;
  const maxTopTrack = stats.top_tracks[0]?.[2] ?? 1;
  const maxTopArtist = stats.top_artists[0]?.[1] ?? 1;
  const maxContext = stats.context_breakdown[0]?.[1] ?? 1;
  const maxDecade = stats.release_decades[0]?.[1] ?? 1;

  const deepCut =
    stats.most_obscure_track ??
    (stats as unknown as { deep_cut_track?: [string, string, number] | null }).deep_cut_track ??
    null;

  const friendsSummary = data?.friends_summary ?? [];
  const recentListens = data?.recent_listens ?? [];

  const kpiItems = [
    {
      label: 'Total plays',
      value: stats.total_plays.toLocaleString(),
      subtext: `${stats.active_days} active ${stats.active_days === 1 ? 'day' : 'days'} · ${stats.session_count.toLocaleString()} sessions`,
      tooltip:
        'Total deduplicated track listens recorded in the selected time window and listener filter.',
      icon: Activity,
    },
    {
      label: 'Listening time',
      value: formatTotalTime(stats.total_listened_ms),
      subtext: stats.avg_session_ms
        ? `Avg session ${formatTotalTime(stats.avg_session_ms)}`
        : 'Average session unavailable',
      tooltip: 'Estimated total playback time based on actual listened duration per play.',
      icon: Clock,
    },
    {
      label: 'Unique tracks',
      value: stats.unique_tracks.toLocaleString(),
      subtext:
        stats.avg_popularity != null
          ? `${stats.unique_artists.toLocaleString()} artists · Avg pop ${Math.round(stats.avg_popularity)}/100`
          : `${stats.unique_artists.toLocaleString()} unique artists`,
      tooltip:
        'Count of distinct tracks and artists heard across the selected listens, alongside average Spotify popularity (0–100).',
      icon: Disc3,
    },
    {
      label: completionRate != null ? 'Completion & streak' : 'Listening streak',
      value:
        completionRate != null
          ? `${completionRate.toFixed(0)}%`
          : `${stats.current_streak_days}d`,
      subtext:
        completionRate != null
          ? `${stats.current_streak_days}d streak · Best ${stats.longest_streak_days}d · ${stats.skipped_plays} skipped`
          : `Best streak ${stats.longest_streak_days}d · ${stats.completed_plays} full plays`,
      tooltip:
        'Share of judged plays listened to completion (excluding mid-track pauses), paired with consecutive active days.',
      icon: Flame,
    },
  ];

  return (
    <div className="w-full">
      {/* Subtle Top Header Band */}
      <header className="flex flex-wrap items-baseline justify-between gap-2 px-6 pt-6 pb-5 lg:px-8">
        <div>
          <h1 className="text-2xl font-bold tracking-[-0.03em] text-white">Overview</h1>
          <p className="mt-1 text-xs text-[var(--ink-secondary)]">
            {timeframeLabel} · {scopeLabel}
          </p>
        </div>
        {stats.last_played_ms != null && (
          <span className="font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
            Last play {formatRelativeTime(stats.last_played_ms)}
          </span>
        )}
      </header>

      {/* Live Strip (when present) */}
      {liveStrip}

      {/* Hairline KPI Strip — Zero Card Boxes */}
      <section className="px-6 lg:px-8">
        <div className="grid grid-cols-2 border-y border-white/[0.08] divide-y divide-white/[0.07] lg:grid-cols-4 lg:divide-y-0 lg:divide-x">
          {kpiItems.map((item, index) => {
            const Icon = item.icon;
            return (
              <StaggerItem
                key={item.label}
                index={index}
                className={cn(
                  'flex flex-col justify-between py-6',
                  index % 2 === 0 ? 'pr-5' : 'pl-5',
                  'lg:px-6 lg:first:pl-0 lg:last:pr-0',
                )}
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-1.5">
                    <span className="text-[13px] font-medium text-[var(--ink-secondary)]">
                      {item.label}
                    </span>
                    <InfoTooltip iconOnly content={item.tooltip} />
                  </div>
                  <Icon className="h-3.5 w-3.5 text-[var(--ink-muted)]" />
                </div>

                <div className="mt-3.5 text-[32px] font-semibold leading-none tracking-[-0.03em] text-white tabular-nums">
                  {item.value}
                </div>

                <p className="mt-2.5 truncate text-xs text-[var(--ink-muted)] tabular-nums">
                  {item.subtext}
                </p>
              </StaggerItem>
            );
          })}
        </div>
      </section>

      {/* Listening Rhythm & Heatmap — Painted directly on #09090b */}
      <section className="border-b border-white/[0.07] px-6 py-8 lg:px-8">
        <div className="flex flex-wrap items-center justify-between gap-3 pb-6">
          <div>
            <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
              Listening rhythm
            </h2>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              {rhythmMode === 'charts'
                ? 'Hourly cadence and weekday volume in local time'
                : '7×24 listening density across weekdays and hours'}
            </p>
          </div>

          <div
            role="tablist"
            aria-label="Listening rhythm view"
            className="inline-flex items-center rounded-full border border-white/[0.08] bg-white/[0.03] p-1"
          >
            {(
              [
                { id: 'charts', label: 'Activity' },
                { id: 'heatmap', label: '7×24 Heatmap' },
              ] as const
            ).map((tab) => {
              const active = rhythmMode === tab.id;
              return (
                <button
                  key={tab.id}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => setRhythmMode(tab.id)}
                  className={cn(
                    'tactile-press relative rounded-full px-3.5 py-1.5 text-xs font-medium transition-colors cursor-pointer',
                    active
                      ? 'text-white'
                      : 'text-[var(--ink-muted)] hover:text-[var(--ink-secondary)]',
                  )}
                >
                  {active && (
                    <motion.span
                      layoutId="overview-chart-tab"
                      transition={SPRING_SNAPPY}
                      className="absolute inset-0 rounded-full bg-white/[0.11] ring-1 ring-inset ring-white/[0.1]"
                    />
                  )}
                  <span className="relative z-10">{tab.label}</span>
                </button>
              );
            })}
          </div>
        </div>

        <AnimatePresence mode="wait" initial={false}>
          {rhythmMode === 'charts' ? (
            <motion.div
              key="charts"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.18, ease: EASE_OUT_EXPO }}
              className="grid grid-cols-1 gap-8 lg:grid-cols-2"
            >
              {/* Hourly Area Chart */}
              <div>
                <div className="mb-3 flex items-center justify-between">
                  <span className="text-[13px] font-medium text-white">
                    Plays by hour
                  </span>
                  <span className="flex items-center gap-1.5 text-[11px] text-[var(--ink-muted)]">
                    <span className="h-2 w-2 rounded-full bg-[var(--chart-1)]" />
                    24-hour distribution
                  </span>
                </div>
                <div className="h-56 w-full">
                  <ResponsiveContainer width="100%" height="100%">
                    <AreaChart
                      data={hourlyData}
                      margin={{ top: 6, right: 8, left: -24, bottom: 0 }}
                    >
                      <defs>
                        <linearGradient id="bsideHourlyFill" x1="0" y1="0" x2="0" y2="1">
                          <stop offset="0%" stopColor="var(--chart-1)" stopOpacity={0.36} />
                          <stop offset="100%" stopColor="var(--chart-1)" stopOpacity={0.01} />
                        </linearGradient>
                      </defs>
                      <CartesianGrid
                        stroke="var(--chart-grid)"
                        strokeDasharray="2 4"
                        vertical={false}
                      />
                      <XAxis
                        dataKey="shortHour"
                        tick={{ fill: 'var(--chart-axis)', fontSize: 11 }}
                        axisLine={{ stroke: 'var(--chart-grid)' }}
                        tickLine={false}
                        interval={2}
                      />
                      <YAxis
                        tick={{ fill: 'var(--chart-axis)', fontSize: 11 }}
                        axisLine={false}
                        tickLine={false}
                        allowDecimals={false}
                      />
                      <RechartsTooltip
                        cursor={{ stroke: 'rgba(255,255,255,0.16)', strokeWidth: 1 }}
                        content={<ChartTooltip />}
                        labelFormatter={(val) => `${val}:00`}
                      />
                      <Area
                        type="monotone"
                        dataKey="plays"
                        stroke="var(--chart-1)"
                        strokeWidth={2}
                        fill="url(#bsideHourlyFill)"
                        activeDot={{
                          r: 4,
                          fill: 'var(--chart-1)',
                          stroke: '#09090b',
                          strokeWidth: 2,
                        }}
                      />
                    </AreaChart>
                  </ResponsiveContainer>
                </div>
              </div>

              {/* Weekday Bar Chart */}
              <div className="lg:border-l lg:border-white/[0.07] lg:pl-8">
                <div className="mb-3 flex items-center justify-between">
                  <span className="text-[13px] font-medium text-white">
                    Plays by weekday
                  </span>
                  <span className="flex items-center gap-1.5 text-[11px] text-[var(--ink-muted)]">
                    <span className="h-2 w-2 rounded-full bg-[var(--chart-2)]" />
                    Monday to Sunday
                  </span>
                </div>
                <div className="h-56 w-full">
                  <ResponsiveContainer width="100%" height="100%">
                    <BarChart
                      data={weekdayData}
                      margin={{ top: 6, right: 8, left: -24, bottom: 0 }}
                    >
                      <CartesianGrid
                        stroke="var(--chart-grid)"
                        strokeDasharray="2 4"
                        vertical={false}
                      />
                      <XAxis
                        dataKey="day"
                        tick={{ fill: 'var(--chart-axis)', fontSize: 11 }}
                        axisLine={{ stroke: 'var(--chart-grid)' }}
                        tickLine={false}
                      />
                      <YAxis
                        tick={{ fill: 'var(--chart-axis)', fontSize: 11 }}
                        axisLine={false}
                        tickLine={false}
                        allowDecimals={false}
                      />
                      <RechartsTooltip
                        cursor={{ fill: 'rgba(255,255,255,0.04)' }}
                        content={<ChartTooltip />}
                      />
                      <Bar
                        dataKey="plays"
                        fill="var(--chart-2)"
                        radius={[5, 5, 0, 0]}
                        maxBarSize={38}
                      />
                    </BarChart>
                  </ResponsiveContainer>
                </div>
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="heatmap"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.18, ease: EASE_OUT_EXPO }}
            >
              <div className="mb-3 flex items-center justify-end gap-1.5 text-[11px] text-[var(--ink-muted)]">
                <span>Less</span>
                {[
                  'var(--heat-0)',
                  'var(--heat-1)',
                  'var(--heat-2)',
                  'var(--heat-3)',
                  'var(--heat-4)',
                ].map((color, idx) => (
                  <span
                    key={idx}
                    className="h-3 w-3 rounded-[3px] ring-1 ring-inset ring-white/[0.06]"
                    style={{ backgroundColor: color }}
                  />
                ))}
                <span>More</span>
              </div>

              <div className="overflow-x-auto">
                <div className="min-w-[620px]">
                  <div className="mb-2 grid grid-cols-[44px_repeat(24,minmax(0,1fr))] gap-1.5 font-mono text-[10px] text-[var(--ink-muted)]">
                    <div />
                    {HOURS.map((h) => (
                      <div key={h} className="text-center">
                        {h % 3 === 0 ? String(h).padStart(2, '0') : ''}
                      </div>
                    ))}
                  </div>

                  <div className="space-y-1.5">
                    {WEEKDAYS.map((day, dIdx) => (
                      <div
                        key={day}
                        className="grid grid-cols-[44px_repeat(24,minmax(0,1fr))] items-center gap-1.5"
                      >
                        <span className="text-[11px] font-medium text-[var(--ink-secondary)]">
                          {day}
                        </span>
                        {HOURS.map((h) => {
                          const count = stats.heatmap_grid[dIdx]?.[h] ?? 0;
                          return (
                            <Tooltip key={h}>
                              <TooltipTrigger asChild>
                                <div
                                  className="h-6 rounded-[4px] ring-1 ring-inset ring-white/[0.05] transition-transform duration-150 ease-out hover:z-10 hover:scale-[1.14] hover:ring-white/30"
                                  style={{
                                    backgroundColor: heatCellColor(count, heatmapMax),
                                  }}
                                />
                              </TooltipTrigger>
                              <TooltipContent>
                                <div className="text-xs font-medium text-white">
                                  {day} ·{' '}
                                  <span className="font-mono">
                                    {String(h).padStart(2, '0')}:00–
                                    {String((h + 1) % 24).padStart(2, '0')}:00
                                  </span>
                                </div>
                                <div className="mt-0.5 text-[11px] text-[var(--accent-ember)] tabular-nums">
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
            </motion.div>
          )}
        </AnimatePresence>
      </section>

      {/* Top Tracks & Top Artists — 2-col Full-Width Borderless Ranked Lists */}
      <section className="grid grid-cols-1 gap-10 border-b border-white/[0.07] px-6 py-8 lg:grid-cols-2 lg:px-8">
        {/* Top Tracks */}
        <div className="flex flex-col">
          <div className="pb-4">
            <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
              Top tracks
            </h2>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              Most played songs in the selected timeframe
            </p>
          </div>

          <div className="divide-y divide-white/[0.06] border-t border-white/[0.07]">
            {stats.top_tracks.length === 0 ? (
              <p className="py-8 text-xs text-[var(--ink-muted)]">No tracks yet.</p>
            ) : (
              stats.top_tracks.map(([artist, track, plays], idx) => {
                const match = trackCoverLookup.get(
                  `${artist.toLowerCase()}||${track.toLowerCase()}`,
                );
                const pct = Math.max(4, (plays / maxTopTrack) * 100);

                return (
                  <StaggerItem key={`${artist}-${track}-${idx}`} index={idx}>
                    <div className="group -mx-2.5 rounded-[8px] px-2.5 py-2.5 transition-colors hover:bg-white/[0.055]">
                      <div className="flex items-center justify-between gap-3">
                        <div className="flex min-w-0 items-center gap-3">
                          <span className="w-5 shrink-0 font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                            {String(idx + 1).padStart(2, '0')}
                          </span>
                          <CoverArt
                            src={match?.coverUrl}
                            alt={track}
                            size="sm"
                            hoverZoom
                          />
                          <div className="min-w-0">
                            {match?.spotifyUrl ? (
                              <a
                                href={match.spotifyUrl}
                                target="_blank"
                                rel="noreferrer"
                                className="block truncate text-[13px] font-medium text-white transition-colors hover:text-[var(--accent-ember)]"
                              >
                                {track}
                              </a>
                            ) : (
                              <div className="truncate text-[13px] font-medium text-white">
                                {track}
                              </div>
                            )}
                            <div className="truncate text-xs text-[var(--ink-secondary)]">
                              {artist}
                            </div>
                          </div>
                        </div>

                        <span className="shrink-0 text-xs font-medium text-[var(--ink-secondary)] tabular-nums">
                          {plays.toLocaleString()}
                        </span>
                      </div>

                      <div className="mt-2 ml-8 h-[3px] overflow-hidden rounded-full bg-white/[0.05]">
                        <motion.div
                          initial={{ width: 0 }}
                          animate={{ width: `${pct}%` }}
                          transition={{
                            duration: 0.45,
                            delay: Math.min(idx * 0.03, 0.2),
                            ease: EASE_OUT_EXPO,
                          }}
                          className="h-full rounded-full bg-[var(--accent-ember)]/85"
                        />
                      </div>
                    </div>
                  </StaggerItem>
                );
              })
            )}
          </div>
        </div>

        {/* Top Artists */}
        <div className="flex flex-col">
          <div className="pb-4">
            <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
              Top artists
            </h2>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              Highest rotation artists across listens
            </p>
          </div>

          <div className="divide-y divide-white/[0.06] border-t border-white/[0.07]">
            {stats.top_artists.length === 0 ? (
              <p className="py-8 text-xs text-[var(--ink-muted)]">No artists yet.</p>
            ) : (
              stats.top_artists.map(([artist, plays], idx) => {
                const pct = Math.max(4, (plays / maxTopArtist) * 100);
                return (
                  <StaggerItem key={`${artist}-${idx}`} index={idx}>
                    <div className="group -mx-2.5 rounded-[8px] px-2.5 py-2.5 transition-colors hover:bg-white/[0.055]">
                      <div className="flex items-center justify-between gap-3">
                        <div className="flex min-w-0 items-center gap-3">
                          <span className="w-5 shrink-0 font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                            {String(idx + 1).padStart(2, '0')}
                          </span>
                          <span className="truncate text-[13px] font-medium text-white">
                            {artist}
                          </span>
                        </div>
                        <span className="shrink-0 text-xs font-medium text-[var(--ink-secondary)] tabular-nums">
                          {plays.toLocaleString()}
                        </span>
                      </div>

                      <div className="mt-2 ml-8 h-[3px] overflow-hidden rounded-full bg-white/[0.05]">
                        <motion.div
                          initial={{ width: 0 }}
                          animate={{ width: `${pct}%` }}
                          transition={{
                            duration: 0.45,
                            delay: Math.min(idx * 0.03, 0.2),
                            ease: EASE_OUT_EXPO,
                          }}
                          className="h-full rounded-full bg-[var(--accent-ember)]/75"
                        />
                      </div>
                    </div>
                  </StaggerItem>
                );
              })
            )}
          </div>
        </div>
      </section>

      {/* Listening Sources & Release Decades */}
      <section className="grid grid-cols-1 gap-10 border-b border-white/[0.07] px-6 py-8 lg:grid-cols-2 lg:px-8">
        <div>
          <div className="pb-4">
            <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
              Listening sources
            </h2>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              Playlists, albums, and Liked Songs
            </p>
          </div>

          <div className="space-y-4 border-t border-white/[0.07] pt-5">
            {stats.context_breakdown.map(([ctx, plays], idx) => {
              const share = Math.round((plays / stats.total_plays) * 100);
              const pct = Math.max(3, (plays / maxContext) * 100);
              return (
                <div key={ctx} className="space-y-1.5">
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-medium text-white">
                      {formatContextLabel(ctx)}
                    </span>
                    <span className="text-[var(--ink-secondary)] tabular-nums">
                      {plays.toLocaleString()} · {share}%
                    </span>
                  </div>
                  <div className="h-[4px] overflow-hidden rounded-full bg-white/[0.06]">
                    <motion.div
                      initial={{ width: 0 }}
                      animate={{ width: `${pct}%` }}
                      transition={{
                        duration: 0.45,
                        delay: Math.min(idx * 0.04, 0.2),
                        ease: EASE_OUT_EXPO,
                      }}
                      className="h-full rounded-full bg-[var(--accent-blue)]/85"
                    />
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <div>
          <div className="pb-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
                  Release decades
                </h2>
                <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
                  Era distribution of played tracks
                </p>
              </div>
              {deepCut && (
                <InfoTooltip
                  content="Least popular track listened to in this period (lowest Spotify 0–100 score)"
                  side="top"
                >
                  <span className="inline-flex items-center gap-1.5 text-xs text-[var(--accent-ember)]">
                    <Sparkles className="h-3.5 w-3.5" />
                    <span>Deep cut · Pop {deepCut[2]}</span>
                  </span>
                </InfoTooltip>
              )}
            </div>

            {deepCut && (
              <div className="mt-3 flex items-center gap-2 border-l-2 border-[var(--accent-ember)]/70 pl-3 text-xs">
                <span className="truncate font-medium text-white">{deepCut[1]}</span>
                <span className="text-[var(--ink-muted)]">·</span>
                <span className="truncate text-[var(--ink-secondary)]">{deepCut[0]}</span>
              </div>
            )}
          </div>

          <div className="space-y-4 border-t border-white/[0.07] pt-5">
            {stats.release_decades.length === 0 ? (
              <p className="text-xs text-[var(--ink-muted)]">
                No release metadata available yet.
              </p>
            ) : (
              stats.release_decades.map(([decade, plays], idx) => {
                const pct = Math.max(3, (plays / maxDecade) * 100);
                return (
                  <div key={decade} className="space-y-1.5">
                    <div className="flex items-center justify-between text-xs">
                      <span className="font-medium text-white tabular-nums">{decade}</span>
                      <span className="text-[var(--ink-secondary)] tabular-nums">
                        {plays.toLocaleString()}
                      </span>
                    </div>
                    <div className="h-[4px] overflow-hidden rounded-full bg-white/[0.06]">
                      <motion.div
                        initial={{ width: 0 }}
                        animate={{ width: `${pct}%` }}
                        transition={{
                          duration: 0.45,
                          delay: Math.min(idx * 0.04, 0.2),
                          ease: EASE_OUT_EXPO,
                        }}
                        className="h-full rounded-full bg-[var(--accent-ember)]/85"
                      />
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>
      </section>

      {/* Listeners & Recent Listens */}
      {(friendsSummary.length > 0 || recentListens.length > 0) && (
        <section className="grid grid-cols-1 gap-10 px-6 py-8 lg:grid-cols-2 lg:px-8">
          {friendsSummary.length > 0 && (
            <div className="flex flex-col">
              <div className="flex items-center justify-between pb-4">
                <div>
                  <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
                    Listeners
                  </h2>
                  <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
                    Select a listener to filter stats
                  </p>
                </div>
                <Users className="h-4 w-4 text-[var(--ink-muted)]" />
              </div>

              <div className="divide-y divide-white/[0.06] border-t border-white/[0.07]">
                {friendsSummary.slice(0, 8).map((friend, idx) => {
                  const isActive = selectedSet.has(friend.user_uri.toLowerCase());
                  const avatarUrl =
                    friend.image_url ??
                    lookupAvatar(avatarMap, friend.user_uri, friend.user_name);

                  return (
                    <StaggerItem key={friend.user_uri} index={idx}>
                      <button
                        type="button"
                        onClick={() => onFilterByUser?.(friend.user_uri)}
                        className={cn(
                          'tactile-press -mx-2.5 flex w-[calc(100%+1.25rem)] items-center justify-between gap-3 rounded-[8px] px-2.5 py-2.5 text-left transition-colors cursor-pointer',
                          isActive
                            ? 'bg-[var(--accent-ember-soft)]'
                            : 'hover:bg-white/[0.055]',
                        )}
                      >
                        <div className="flex min-w-0 items-center gap-3">
                          <UserAvatar
                            name={friend.user_name}
                            imageUrl={avatarUrl}
                            size="sm"
                            isLive={friend.is_live}
                            isSelf={friend.is_self}
                          />
                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span
                                className={cn(
                                  'truncate text-[13px] font-medium',
                                  isActive ? 'text-[var(--accent-ember)]' : 'text-white',
                                )}
                              >
                                {friend.user_name}
                              </span>
                              {friend.is_self && (
                                <span className="text-[11px] text-[var(--ink-muted)]">· You</span>
                              )}
                            </div>
                            <div className="truncate text-xs text-[var(--ink-secondary)]">
                              {friend.latest_track_name} · {friend.latest_artist_name}
                            </div>
                          </div>
                        </div>

                        <div className="shrink-0 text-right">
                          <div className="text-xs font-medium text-white tabular-nums">
                            {friend.total_plays.toLocaleString()}
                          </div>
                          <div className="font-mono text-[11px] text-[var(--ink-muted)]">
                            {formatRelativeTime(friend.last_active_ms)}
                          </div>
                        </div>
                      </button>
                    </StaggerItem>
                  );
                })}
              </div>
            </div>
          )}

          {recentListens.length > 0 && (
            <div className="flex flex-col">
              <div className="pb-4">
                <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
                  Recent listens
                </h2>
                <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
                  Latest recorded plays in this scope
                </p>
              </div>

              <div className="divide-y divide-white/[0.06] border-t border-white/[0.07]">
                {recentListens.slice(0, 8).map((listen, idx) => {
                  const avatarUrl =
                    listen.user_image_url ??
                    lookupAvatar(avatarMap, listen.user_uri, listen.user_name);
                  const contextLabel =
                    listen.context_name || formatContextLabel(listen.context_type);
                  const canOpenPlaylist = Boolean(listen.context_uri && onSelectPlaylist);

                  return (
                    <StaggerItem key={listen.id} index={idx}>
                      <div className="group -mx-2.5 flex items-center justify-between gap-3 rounded-[8px] px-2.5 py-2.5 transition-colors hover:bg-white/[0.055]">
                        <div className="flex min-w-0 items-center gap-3">
                          <CoverArt
                            src={listen.cover_url}
                            alt={listen.track_name}
                            size="sm"
                            hoverZoom
                          />
                          <div className="min-w-0">
                            <a
                              href={listen.spotify_url}
                              target="_blank"
                              rel="noreferrer"
                              className="inline-flex max-w-full items-center gap-1 text-[13px] font-medium text-white transition-colors hover:text-[var(--accent-ember)]"
                            >
                              <span className="truncate">{listen.track_name}</span>
                              <ExternalLink className="h-3 w-3 shrink-0 opacity-0 transition-opacity group-hover:opacity-60" />
                            </a>
                            <div className="truncate text-xs text-[var(--ink-secondary)]">
                              {listen.all_artists || listen.artist_name}
                            </div>
                            <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-[var(--ink-muted)]">
                              <button
                                type="button"
                                onClick={() => onFilterByUser?.(listen.user_uri)}
                                className="tactile-press inline-flex items-center gap-1.5 text-[var(--ink-secondary)] transition-colors hover:text-white cursor-pointer"
                              >
                                <UserAvatar
                                  name={listen.user_name}
                                  imageUrl={avatarUrl}
                                  size="xs"
                                />
                                <span className="max-w-[110px] truncate">{listen.user_name}</span>
                              </button>
                              {canOpenPlaylist && listen.context_uri && (
                                <>
                                  <span aria-hidden="true">·</span>
                                  <button
                                    type="button"
                                    onClick={() => onSelectPlaylist?.(listen.context_uri!)}
                                    className="tactile-press inline-flex max-w-[150px] items-center gap-1.5 text-[var(--ink-muted)] transition-colors hover:text-[var(--accent-ember)] cursor-pointer"
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
                                </>
                              )}
                            </div>
                          </div>
                        </div>

                        <span className="shrink-0 font-mono text-[11px] text-[var(--ink-muted)]">
                          {formatRelativeTime(listen.played_at_ms)}
                        </span>
                      </div>
                    </StaggerItem>
                  );
                })}
              </div>
            </div>
          )}
        </section>
      )}
    </div>
  );
}
