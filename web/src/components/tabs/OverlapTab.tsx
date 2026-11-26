import { useEffect, useMemo, useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { Award, ChevronDown, ChevronUp, ExternalLink, GitCompare, Users } from 'lucide-react';
import { EASE_OUT_EXPO, SPRING_SMOOTH, StaggerItem } from '@/components/ui/motion';
import { EmptyState } from '@/components/EmptyState';
import { InfoTooltip } from '@/components/InfoTooltip';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { cn, formatRelativeTime, spotifyUriToUrl } from '@/lib/utils';
import {
  buildUsersParam,
  type AvatarMap,
  type FriendPairOverlap,
  type OverlapReport,
} from '@/types/api';

export interface OverlapTabProps {
  selectedUsers: string[];
  days: number | null;
  refreshTick: number;
  avatarMap: AvatarMap;
  onFilterByUser?: (userUri: string) => void;
}

function findUserUri(avatarMap: AvatarMap, name: string): string | null {
  if (!name) return null;
  if (avatarMap[name]?.user_uri) return avatarMap[name].user_uri;
  const lower = name.toLowerCase();
  for (const entry of Object.values(avatarMap)) {
    if (entry.name.toLowerCase() === lower) {
      return entry.user_uri;
    }
  }
  return null;
}

function ListenerAvatarStack({
  names,
  avatarMap,
  onFilterByUser,
  maxVisible = 6,
}: {
  names: string[];
  avatarMap: AvatarMap;
  onFilterByUser?: (userUri: string) => void;
  maxVisible?: number;
}) {
  const visible = names.slice(0, maxVisible);
  const overflow = names.length - visible.length;

  return (
    <div className="flex items-center">
      <div className="flex items-center -space-x-1.5">
        {visible.map((name) => {
          const uri = findUserUri(avatarMap, name);
          const imageUrl = lookupAvatar(avatarMap, uri, name);
          const clickable = Boolean(uri && onFilterByUser);

          if (clickable && uri) {
            return (
              <button
                key={name}
                type="button"
                onClick={() => onFilterByUser?.(uri)}
                title={`Filter by ${name}`}
                className="tactile-press relative rounded-full ring-2 ring-[var(--bg-surface)] transition-transform hover:z-10 hover:scale-110 cursor-pointer"
              >
                <UserAvatar name={name} imageUrl={imageUrl} size="sm" />
              </button>
            );
          }

          return (
            <span
              key={name}
              title={name}
              className="relative rounded-full ring-2 ring-[var(--bg-surface)]"
            >
              <UserAvatar name={name} imageUrl={imageUrl} size="sm" />
            </span>
          );
        })}
      </div>
      {overflow > 0 && (
        <span className="ml-2 text-[11px] text-[var(--ink-muted)] tabular-nums">
          +{overflow}
        </span>
      )}
    </div>
  );
}

function PairCard({
  pair,
  avatarMap,
  onFilterByUser,
  index = 0,
  muted = false,
}: {
  pair: FriendPairOverlap;
  avatarMap: AvatarMap;
  onFilterByUser?: (userUri: string) => void;
  index?: number;
  muted?: boolean;
}) {
  const scorePct = Math.round(pair.compatibility_score * 100);
  const artistPct = Math.round(pair.artist_jaccard * 100);
  const trackPct = Math.round(pair.track_jaccard * 100);

  const uriA = findUserUri(avatarMap, pair.friend_a);
  const uriB = findUserUri(avatarMap, pair.friend_b);
  const avatarA = lookupAvatar(avatarMap, uriA, pair.friend_a);
  const avatarB = lookupAvatar(avatarMap, uriB, pair.friend_b);

  return (
    <StaggerItem index={index}>
      <div
        className={cn(
          'rounded-[12px] border border-[var(--border-subtle)] p-4 transition-colors',
          muted
            ? 'bg-[var(--bg-inset)]/50 opacity-70'
            : 'bg-[var(--bg-surface)] hover:border-[var(--border-strong)]',
        )}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            {/* Overlapping pair avatars */}
            <div className="flex shrink-0 items-center -space-x-2">
              <button
                type="button"
                disabled={!uriA || !onFilterByUser}
                onClick={() => uriA && onFilterByUser?.(uriA)}
                title={uriA ? `Filter by ${pair.friend_a}` : pair.friend_a}
                className="tactile-press relative rounded-full ring-2 ring-[var(--bg-surface)] transition-transform enabled:cursor-pointer enabled:hover:z-10 enabled:hover:scale-105"
              >
                <UserAvatar name={pair.friend_a} imageUrl={avatarA} size="md" />
              </button>
              <button
                type="button"
                disabled={!uriB || !onFilterByUser}
                onClick={() => uriB && onFilterByUser?.(uriB)}
                title={uriB ? `Filter by ${pair.friend_b}` : pair.friend_b}
                className="tactile-press relative rounded-full ring-2 ring-[var(--bg-surface)] transition-transform enabled:cursor-pointer enabled:hover:z-10 enabled:hover:scale-105"
              >
                <UserAvatar name={pair.friend_b} imageUrl={avatarB} size="md" />
              </button>
            </div>

            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[13px] font-medium text-[var(--ink-primary)]">
                <button
                  type="button"
                  disabled={!uriA || !onFilterByUser}
                  onClick={() => uriA && onFilterByUser?.(uriA)}
                  className="truncate transition-colors enabled:cursor-pointer enabled:hover:text-[var(--accent-ember)]"
                >
                  {pair.friend_a}
                </button>
                <span className="text-[var(--ink-muted)]">&amp;</span>
                <button
                  type="button"
                  disabled={!uriB || !onFilterByUser}
                  onClick={() => uriB && onFilterByUser?.(uriB)}
                  className="truncate transition-colors enabled:cursor-pointer enabled:hover:text-[var(--accent-ember)]"
                >
                  {pair.friend_b}
                </button>
              </div>
              <div className="mt-0.5 text-xs text-[var(--ink-muted)] tabular-nums">
                {pair.shared_artists} shared {pair.shared_artists === 1 ? 'artist' : 'artists'} ·{' '}
                {pair.shared_tracks} shared {pair.shared_tracks === 1 ? 'track' : 'tracks'}
              </div>
            </div>
          </div>

          <div className="shrink-0 text-right">
            <div
              className={cn(
                'text-2xl font-semibold leading-none tracking-[-0.02em] tabular-nums',
                muted ? 'text-[var(--ink-muted)]' : 'text-[var(--ink-primary)]',
              )}
            >
              {scorePct}%
            </div>
            <div className="mt-1 flex items-center justify-end gap-1 text-[11px] text-[var(--ink-muted)]">
              <span>Match</span>
              <InfoTooltip
                iconOnly
                content="Weighted Jaccard similarity combining shared artists and shared tracks between two listeners."
              />
            </div>
          </div>
        </div>

        <div className="mt-3.5 h-[5px] w-full overflow-hidden rounded-full bg-white/[0.06]">
          <motion.div
            initial={{ width: 0 }}
            animate={{ width: `${muted ? 0 : Math.max(3, scorePct)}%` }}
            transition={{
              duration: 0.5,
              delay: Math.min(index * 0.035, 0.22),
              ease: EASE_OUT_EXPO,
            }}
            className={cn(
              'h-full rounded-full',
              muted ? 'bg-[var(--ink-muted)]/40' : 'bg-[var(--accent-ember)]',
            )}
          />
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-[var(--ink-secondary)] tabular-nums">
          <span className="inline-flex items-center gap-1">
            <span>Artists</span>
            <InfoTooltip
              iconOnly
              content="Number of unique artists listened to by both people in the selected timeframe."
            />
            <span className="font-medium text-[var(--ink-primary)]">{artistPct}%</span>
            <span className="text-[var(--ink-muted)]">
              ({pair.shared_artists}/{pair.total_union_artists})
            </span>
          </span>
          <span className="text-[var(--ink-muted)]">·</span>
          <span className="inline-flex items-center gap-1">
            <span>Tracks</span>
            <InfoTooltip
              iconOnly
              content="Number of unique tracks listened to by both people in the selected timeframe."
            />
            <span className="font-medium text-[var(--ink-primary)]">{trackPct}%</span>
            <span className="text-[var(--ink-muted)]">
              ({pair.shared_tracks}/{pair.total_union_tracks})
            </span>
          </span>
        </div>
      </div>
    </StaggerItem>
  );
}

export function OverlapTab({
  selectedUsers,
  days,
  refreshTick,
  avatarMap,
  onFilterByUser,
}: OverlapTabProps) {
  const [report, setReport] = useState<OverlapReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [showZeroPairs, setShowZeroPairs] = useState(false);

  const usersParam = useMemo(() => buildUsersParam(selectedUsers), [selectedUsers]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    const params = new URLSearchParams();
    if (usersParam) {
      params.set('users', usersParam);
      params.set('friends', usersParam);
    }
    if (days != null) params.set('days', String(days));

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
  }, [usersParam, days, refreshTick]);

  const { activePairs, zeroPairs } = useMemo(() => {
    if (!report?.friend_pairs) return { activePairs: [], zeroPairs: [] };
    const active: FriendPairOverlap[] = [];
    const zero: FriendPairOverlap[] = [];
    for (const pair of report.friend_pairs) {
      if (pair.shared_artists > 0 || pair.shared_tracks > 0 || pair.compatibility_score > 0) {
        active.push(pair);
      } else {
        zero.push(pair);
      }
    }
    active.sort((a, b) => b.compatibility_score - a.compatibility_score);
    return { activePairs: active, zeroPairs: zero };
  }, [report]);

  if (loading && !report) {
    return (
      <div className="space-y-6">
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          <div className="bside-card h-56 bg-[var(--bg-surface)]/70 lg:col-span-2" />
          <div className="bside-card h-56 bg-[var(--bg-surface)]/70" />
        </div>
        <div className="bside-card h-64 bg-[var(--bg-surface)]/60" />
      </div>
    );
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
    <div className="space-y-7">
      {/* Pair Compatibility & Trendsetter Leaderboard */}
      <section className="grid grid-cols-1 gap-5 lg:grid-cols-3">
        {/* Pair Compatibility */}
        <div className="space-y-4 lg:col-span-2">
          <div className="flex items-center justify-between">
            <div>
              <div className="flex items-center gap-2">
                <GitCompare className="h-4 w-4 text-[var(--accent-ember)]" />
                <h2 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
                  Friend compatibility
                </h2>
                <InfoTooltip
                  iconOnly
                  content="Weighted Jaccard similarity combining shared artists and shared tracks between two listeners."
                />
              </div>
              <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
                Ranked taste overlap across shared artists and tracks
              </p>
            </div>
            {activePairs.length > 0 && (
              <span className="text-xs text-[var(--ink-muted)] tabular-nums">
                {activePairs.length} {activePairs.length === 1 ? 'pair' : 'pairs'}
              </span>
            )}
          </div>

          {activePairs.length === 0 ? (
            <EmptyState
              title="No overlapping tracks or artists in this timeframe yet"
              description="Try widening the timeframe or selecting more friends to discover shared music."
            />
          ) : (
            <div className="grid grid-cols-1 gap-3.5 md:grid-cols-2">
              {activePairs.map((pair, idx) => (
                <PairCard
                  key={`${pair.friend_a}-${pair.friend_b}`}
                  pair={pair}
                  avatarMap={avatarMap}
                  onFilterByUser={onFilterByUser}
                  index={idx}
                />
              ))}
            </div>
          )}

          {zeroPairs.length > 0 && (
            <div>
              <button
                type="button"
                onClick={() => setShowZeroPairs((v) => !v)}
                className="tactile-press inline-flex items-center gap-1.5 rounded-[8px] border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-3 py-1.5 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--ink-primary)] cursor-pointer"
              >
                {showZeroPairs ? (
                  <>
                    <ChevronUp className="h-3.5 w-3.5" />
                    Hide {zeroPairs.length} zero-overlap{' '}
                    {zeroPairs.length === 1 ? 'pair' : 'pairs'}
                  </>
                ) : (
                  <>
                    <ChevronDown className="h-3.5 w-3.5" />
                    Show {zeroPairs.length} zero-overlap{' '}
                    {zeroPairs.length === 1 ? 'pair' : 'pairs'}
                  </>
                )}
              </button>

              <AnimatePresence initial={false}>
                {showZeroPairs && (
                  <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: 'auto' }}
                    exit={{ opacity: 0, height: 0 }}
                    transition={SPRING_SMOOTH}
                    className="overflow-hidden"
                  >
                    <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
                      {zeroPairs.map((pair, idx) => (
                        <PairCard
                          key={`${pair.friend_a}-${pair.friend_b}`}
                          pair={pair}
                          avatarMap={avatarMap}
                          onFilterByUser={onFilterByUser}
                          index={idx}
                          muted
                        />
                      ))}
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          )}
        </div>

        {/* Trendsetter Leaderboard */}
        <div className="bside-card flex flex-col self-start">
          <div className="border-b border-[var(--border-subtle)] px-5 py-4">
            <div className="flex items-center gap-2">
              <Award className="h-4 w-4 text-[var(--accent-ember)]" />
              <h2 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
                Trendsetters
              </h2>
              <InfoTooltip
                iconOnly
                content="Who listened to shared tracks first before other friends played them."
              />
            </div>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              First to spin shared tracks
            </p>
          </div>

          <div className="divide-y divide-[var(--border-subtle)] px-2 py-1">
            {report.trendsetter_leaderboard.length === 0 ? (
              <p className="px-3 py-8 text-center text-xs text-[var(--ink-muted)]">
                No shared tracks detected yet.
              </p>
            ) : (
              report.trendsetter_leaderboard.map(([name, count], idx) => {
                const uri = findUserUri(avatarMap, name);
                const img = lookupAvatar(avatarMap, uri, name);
                const pct = Math.max(5, (count / maxTrendsetter) * 100);

                return (
                  <StaggerItem key={name} index={idx}>
                    <div className="rounded-[8px] px-3 py-2.5 transition-colors hover:bg-[var(--bg-hover)]">
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex min-w-0 items-center gap-2.5">
                          <span className="w-5 shrink-0 font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                            {String(idx + 1).padStart(2, '0')}
                          </span>
                          <button
                            type="button"
                            disabled={!uri || !onFilterByUser}
                            onClick={() => uri && onFilterByUser?.(uri)}
                            className="tactile-press inline-flex min-w-0 items-center gap-2 text-[13px] font-medium text-[var(--ink-primary)] transition-colors enabled:cursor-pointer enabled:hover:text-[var(--accent-ember)]"
                          >
                            <UserAvatar name={name} imageUrl={img} size="sm" />
                            <span className="truncate">{name}</span>
                          </button>
                        </div>
                        <span className="shrink-0 text-xs font-medium text-[var(--ink-secondary)] tabular-nums">
                          {count} {count === 1 ? 'first' : 'firsts'}
                        </span>
                      </div>

                      <div className="mt-2 ml-7 h-[3px] overflow-hidden rounded-full bg-white/[0.06]">
                        <motion.div
                          initial={{ width: 0 }}
                          animate={{ width: `${pct}%` }}
                          transition={{
                            duration: 0.45,
                            delay: Math.min(idx * 0.035, 0.2),
                            ease: EASE_OUT_EXPO,
                          }}
                          className="h-full rounded-full bg-[var(--accent-ember)]/80"
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

      {/* Shared Tracks */}
      <section className="bside-card">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-subtle)] px-5 py-4">
          <div>
            <h2 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
              Shared tracks
            </h2>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              Tracks played by multiple friends, highlighting who discovered them first
            </p>
          </div>
          {report.shared_tracks.length > 0 && (
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {report.shared_tracks.length} tracks
            </span>
          )}
        </div>

        {report.shared_tracks.length === 0 ? (
          <p className="py-10 text-center text-xs text-[var(--ink-muted)]">
            No shared tracks yet.
          </p>
        ) : (
          <div className="divide-y divide-[var(--border-subtle)] px-2 py-1">
            {report.shared_tracks.slice(0, 50).map((t, idx) => {
              const url = spotifyUriToUrl(t.track_uri);
              const firstUri = findUserUri(avatarMap, t.first_player_name);
              const firstAvatar = lookupAvatar(avatarMap, firstUri, t.first_player_name);

              return (
                <StaggerItem key={t.track_uri} index={idx}>
                  <div className="group flex flex-col gap-2.5 rounded-[8px] px-3 py-3 transition-colors hover:bg-[var(--bg-hover)] sm:flex-row sm:items-center sm:justify-between sm:gap-4">
                    {/* Track & artist */}
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        {url ? (
                          <a
                            href={url}
                            target="_blank"
                            rel="noreferrer"
                            className="truncate text-[13px] font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-ember)]"
                          >
                            {t.track_name}
                          </a>
                        ) : (
                          <span className="truncate text-[13px] font-medium text-[var(--ink-primary)]">
                            {t.track_name}
                          </span>
                        )}
                      </div>
                      <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-[var(--ink-secondary)]">
                        <span className="truncate">{t.artist_name}</span>
                        <span className="text-[var(--ink-muted)]">·</span>
                        <span className="inline-flex items-center gap-1.5 text-[var(--ink-muted)]">
                          <span>First by</span>
                          <button
                            type="button"
                            disabled={!firstUri || !onFilterByUser}
                            onClick={() => firstUri && onFilterByUser?.(firstUri)}
                            className="tactile-press inline-flex items-center gap-1 font-medium text-[var(--accent-ember)] transition-opacity enabled:cursor-pointer enabled:hover:opacity-85"
                          >
                            <UserAvatar
                              name={t.first_player_name}
                              imageUrl={firstAvatar}
                              size="xs"
                            />
                            <span>{t.first_player_name}</span>
                          </button>
                          <span className="font-mono text-[11px] text-[var(--ink-muted)]">
                            {formatRelativeTime(t.first_played_ms)}
                          </span>
                        </span>
                      </div>
                    </div>

                    {/* Listener avatar stack + plays + external link */}
                    <div className="flex shrink-0 items-center justify-between gap-4 sm:justify-end">
                      <ListenerAvatarStack
                        names={t.friends_who_played}
                        avatarMap={avatarMap}
                        onFilterByUser={onFilterByUser}
                      />

                      <div className="flex items-center gap-2">
                        <span className="min-w-[54px] text-right text-xs font-medium text-[var(--ink-primary)] tabular-nums">
                          {t.total_plays.toLocaleString()}{' '}
                          <span className="font-normal text-[var(--ink-muted)]">
                            {t.total_plays === 1 ? 'play' : 'plays'}
                          </span>
                        </span>

                        {url && (
                          <a
                            href={url}
                            target="_blank"
                            rel="noreferrer"
                            title="Open in Spotify"
                            aria-label={`Open ${t.track_name} in Spotify`}
                            className="tactile-press inline-flex h-7 w-7 items-center justify-center rounded-[7px] text-[var(--ink-muted)] transition-colors hover:bg-white/[0.06] hover:text-[var(--ink-primary)]"
                          >
                            <ExternalLink className="h-3.5 w-3.5" />
                          </a>
                        )}
                      </div>
                    </div>
                  </div>
                </StaggerItem>
              );
            })}
          </div>
        )}
      </section>

      {/* Shared Artists */}
      <section className="bside-card">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-subtle)] px-5 py-4">
          <div>
            <div className="flex items-center gap-2">
              <Users className="h-4 w-4 text-[var(--ink-muted)]" />
              <h2 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
                Shared artists
              </h2>
            </div>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              Artists listened to by two or more friends
            </p>
          </div>
          {report.shared_artists.length > 0 && (
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {report.shared_artists.length} artists
            </span>
          )}
        </div>

        {report.shared_artists.length === 0 ? (
          <p className="py-10 text-center text-xs text-[var(--ink-muted)]">
            No shared artists yet.
          </p>
        ) : (
          <div className="divide-y divide-[var(--border-subtle)] px-2 py-1">
            {report.shared_artists.slice(0, 30).map(([artist, friends, plays], idx) => (
              <StaggerItem key={artist} index={idx}>
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-[8px] px-3 py-2.5 transition-colors hover:bg-[var(--bg-hover)]">
                  <div className="flex min-w-0 items-center gap-3">
                    <span className="w-5 shrink-0 font-mono text-[11px] text-[var(--ink-muted)] tabular-nums">
                      {String(idx + 1).padStart(2, '0')}
                    </span>
                    <div className="min-w-0">
                      <div className="truncate text-[13px] font-medium text-[var(--ink-primary)]">
                        {artist}
                      </div>
                      <div className="truncate text-xs text-[var(--ink-muted)]">
                        {friends.join(' · ')}
                      </div>
                    </div>
                  </div>

                  <div className="flex shrink-0 items-center gap-4">
                    <ListenerAvatarStack
                      names={friends}
                      avatarMap={avatarMap}
                      onFilterByUser={onFilterByUser}
                    />
                    <span className="min-w-[60px] text-right text-xs font-medium text-[var(--ink-secondary)] tabular-nums">
                      {plays.toLocaleString()} {plays === 1 ? 'play' : 'plays'}
                    </span>
                  </div>
                </div>
              </StaggerItem>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
