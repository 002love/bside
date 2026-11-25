import { useEffect, useMemo, useState } from 'react';
import { Award, ChevronDown, ChevronUp, ExternalLink, GitCompare, Users } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { EmptyState } from '@/components/EmptyState';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { InfoTooltip } from '@/components/InfoTooltip';
import { formatRelativeTime, spotifyUriToUrl } from '@/lib/utils';
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

interface UserPillProps {
  name: string;
  avatarMap: AvatarMap;
  onFilterByUser?: (userUri: string) => void;
  size?: 'xs' | 'sm';
  accent?: boolean;
}

function UserPill({
  name,
  avatarMap,
  onFilterByUser,
  size = 'xs',
  accent = false,
}: UserPillProps) {
  const uri = findUserUri(avatarMap, name);
  const imageUrl = lookupAvatar(avatarMap, uri, name);
  const clickable = Boolean(uri && onFilterByUser);

  const inner = (
    <>
      <UserAvatar name={name} imageUrl={imageUrl} size={size} />
      <span className="truncate font-medium">{name}</span>
    </>
  );

  const baseClasses = accent
    ? 'inline-flex max-w-[160px] items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--border-accent)] bg-[var(--accent-soft)] px-2 py-0.5 text-[11px] text-[var(--accent-primary)] transition-colors'
    : 'inline-flex max-w-[160px] items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2 py-0.5 text-[11px] text-[var(--ink-secondary)] transition-colors';

  if (clickable && uri) {
    return (
      <button
        type="button"
        onClick={() => onFilterByUser?.(uri)}
        className={`${baseClasses} hover:border-[var(--border-strong)] hover:text-[var(--ink-primary)] cursor-pointer`}
        title={`Filter by ${name}`}
      >
        {inner}
      </button>
    );
  }

  return <span className={baseClasses}>{inner}</span>;
}

function PairCard({
  pair,
  avatarMap,
  onFilterByUser,
  muted = false,
}: {
  pair: FriendPairOverlap;
  avatarMap: AvatarMap;
  onFilterByUser?: (userUri: string) => void;
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
    <div
      className={
        muted
          ? 'rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)]/50 p-3 opacity-75'
          : 'bside-card-interactive rounded-[var(--radius-control)] bg-[var(--bg-inset)] p-3.5'
      }
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <button
            type="button"
            disabled={!uriA || !onFilterByUser}
            onClick={() => uriA && onFilterByUser?.(uriA)}
            className="inline-flex min-w-0 items-center gap-1.5 text-xs font-semibold text-[var(--ink-primary)] transition-colors enabled:cursor-pointer enabled:hover:text-[var(--accent-primary)]"
          >
            <UserAvatar name={pair.friend_a} imageUrl={avatarA} size="sm" />
            <span className="truncate">{pair.friend_a}</span>
          </button>
          <span className="font-mono text-xs text-[var(--ink-muted)]">&times;</span>
          <button
            type="button"
            disabled={!uriB || !onFilterByUser}
            onClick={() => uriB && onFilterByUser?.(uriB)}
            className="inline-flex min-w-0 items-center gap-1.5 text-xs font-semibold text-[var(--ink-primary)] transition-colors enabled:cursor-pointer enabled:hover:text-[var(--accent-primary)]"
          >
            <UserAvatar name={pair.friend_b} imageUrl={avatarB} size="sm" />
            <span className="truncate">{pair.friend_b}</span>
          </button>
        </div>

        <div className="flex shrink-0 items-baseline gap-1 font-display text-2xl font-extrabold leading-none text-[var(--accent-primary)] tabular-nums">
          <span className={muted ? 'text-[var(--ink-muted)]' : ''}>{scorePct}%</span>
          <span className="inline-flex items-center gap-0.5 font-mono text-[9px] font-semibold uppercase tracking-wider text-[var(--ink-muted)]">
            Match
            <InfoTooltip
              iconOnly
              content="Weighted Jaccard similarity combining shared artists and shared tracks between two listeners."
            />
          </span>
        </div>
      </div>

      <div className="mt-2.5 h-1.5 w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--border-subtle)]">
        <div
          className={muted ? 'h-full bg-[var(--ink-muted)]/40' : 'h-full bg-[var(--accent-primary)]'}
          style={{ width: `${muted ? 0 : Math.max(3, scorePct)}%` }}
        />
      </div>

      <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1 font-mono text-[10px] uppercase tracking-wider text-[var(--ink-muted)]">
        <span className="inline-flex items-center gap-1">
          Artists
          <InfoTooltip
            iconOnly
            content="Number of unique artists listened to by both people in the selected timeframe."
          />
          :{' '}
          <strong className="text-[var(--ink-primary)]">{artistPct}%</strong> ({pair.shared_artists}/
          {pair.total_union_artists})
        </span>
        <span className="inline-flex items-center gap-1">
          Tracks
          <InfoTooltip
            iconOnly
            content="Number of unique tracks listened to by both people in the selected timeframe."
          />
          :{' '}
          <strong className="text-[var(--ink-primary)]">{trackPct}%</strong> ({pair.shared_tracks}/
          {pair.total_union_tracks})
        </span>
      </div>
    </div>
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
    return <Card className="h-48 animate-pulse bg-[var(--bg-panel)]" />;
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
    <div className="space-y-6 animate-fade-up">
      {/* Pairwise Compatibility & Trendsetter */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <CardHeader>
            <div className="flex items-center gap-2">
              <GitCompare className="h-4 w-4 text-[var(--accent-primary)]" />
              <CardTitle>Friend Compatibility</CardTitle>
              <InfoTooltip
                iconOnly
                content="Weighted Jaccard similarity combining shared artists and shared tracks between two listeners."
              />
            </div>
            <CardDescription>
              Ranked taste overlap across shared artists and tracks
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {activePairs.length === 0 ? (
              <EmptyState
                title="No overlapping tracks or artists in this timeframe yet"
                description="Try widening the timeframe or selecting more friends to discover shared music."
              />
            ) : (
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                {activePairs.map((pair) => (
                  <PairCard
                    key={`${pair.friend_a}-${pair.friend_b}`}
                    pair={pair}
                    avatarMap={avatarMap}
                    onFilterByUser={onFilterByUser}
                  />
                ))}
              </div>
            )}

            {zeroPairs.length > 0 && (
              <div className="pt-1">
                <button
                  type="button"
                  onClick={() => setShowZeroPairs((v) => !v)}
                  className="inline-flex items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-3 py-1 font-mono text-[10px] font-semibold uppercase tracking-wider text-[var(--ink-muted)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--ink-primary)] cursor-pointer"
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

                {showZeroPairs && (
                  <div className="mt-3 grid grid-cols-1 gap-2.5 md:grid-cols-2">
                    {zeroPairs.map((pair) => (
                      <PairCard
                        key={`${pair.friend_a}-${pair.friend_b}`}
                        pair={pair}
                        avatarMap={avatarMap}
                        onFilterByUser={onFilterByUser}
                        muted
                      />
                    ))}
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Trendsetter Leaderboard */}
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <Award className="h-4 w-4 text-[var(--accent-primary)]" />
              <CardTitle>Trendsetters</CardTitle>
              <InfoTooltip
                iconOnly
                content="Who listened to shared tracks first before other friends played them."
              />
            </div>
            <CardDescription>First to spin shared tracks</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {report.trendsetter_leaderboard.length === 0 ? (
              <p className="font-mono text-xs text-[var(--ink-muted)]">
                No shared tracks detected yet.
              </p>
            ) : (
              report.trendsetter_leaderboard.map(([name, count], idx) => {
                const uri = findUserUri(avatarMap, name);
                const img = lookupAvatar(avatarMap, uri, name);
                return (
                  <div key={name} className="space-y-1.5">
                    <div className="flex items-center justify-between gap-2 font-mono text-xs">
                      <div className="flex min-w-0 items-center gap-2">
                        <span className="w-5 shrink-0 text-[10px] text-[var(--ink-muted)]">
                          {String(idx + 1).padStart(2, '0')}
                        </span>
                        <button
                          type="button"
                          disabled={!uri || !onFilterByUser}
                          onClick={() => uri && onFilterByUser?.(uri)}
                          className="inline-flex min-w-0 items-center gap-1.5 font-semibold text-[var(--ink-primary)] transition-colors enabled:cursor-pointer enabled:hover:text-[var(--accent-primary)]"
                        >
                          <UserAvatar name={name} imageUrl={img} size="sm" />
                          <span className="truncate">{name}</span>
                        </button>
                      </div>
                      <span className="shrink-0 text-[11px] font-semibold text-[var(--accent-primary)] tabular-nums">
                        {count} {count === 1 ? 'first' : 'firsts'}
                      </span>
                    </div>
                    <div className="h-1.5 w-full overflow-hidden rounded-[var(--radius-pill)] bg-[var(--border-subtle)]">
                      <div
                        className="h-full bg-[var(--accent-muted)]"
                        style={{ width: `${Math.max(5, (count / maxTrendsetter) * 100)}%` }}
                      />
                    </div>
                  </div>
                );
              })
            )}
          </CardContent>
        </Card>
      </div>

      {/* Shared Tracks */}
      <Card>
        <CardHeader className="border-b border-[var(--border-subtle)]">
          <CardTitle>Shared Tracks</CardTitle>
          <CardDescription>
            Tracks played by multiple friends, highlighting who discovered them first
          </CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {report.shared_tracks.length === 0 ? (
            <p className="py-8 text-center font-mono text-xs text-[var(--ink-muted)]">
              No shared tracks yet.
            </p>
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
                        {url ? (
                          <a
                            href={url}
                            target="_blank"
                            rel="noreferrer"
                            className="font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)] hover:underline"
                          >
                            {t.track_name}
                          </a>
                        ) : (
                          <div className="font-medium text-[var(--ink-primary)]">
                            {t.track_name}
                          </div>
                        )}
                        <div className="text-[11px] text-[var(--ink-muted)]">{t.artist_name}</div>
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap items-center gap-2">
                          <UserPill
                            name={t.first_player_name}
                            avatarMap={avatarMap}
                            onFilterByUser={onFilterByUser}
                            accent
                          />
                          <span className="font-mono text-[10px] uppercase text-[var(--ink-muted)]">
                            {formatRelativeTime(t.first_played_ms)}
                          </span>
                        </div>
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-1.5">
                          {t.friends_who_played.map((f) => (
                            <UserPill
                              key={f}
                              name={f}
                              avatarMap={avatarMap}
                              onFilterByUser={onFilterByUser}
                            />
                          ))}
                        </div>
                      </TableCell>
                      <TableCell className="text-right font-mono text-xs font-semibold text-[var(--ink-primary)] tabular-nums">
                        {t.total_plays}
                      </TableCell>
                      <TableCell className="text-right">
                        {url && (
                          <a
                            href={url}
                            target="_blank"
                            rel="noreferrer"
                            title="Open in Spotify"
                            className="inline-flex h-7 w-7 items-center justify-center rounded-[var(--radius-control)] border border-transparent text-[var(--ink-muted)] transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--bg-inset)] hover:text-[var(--accent-primary)]"
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
            <Users className="h-4 w-4 text-[var(--accent-primary)]" />
            <CardTitle>Shared Artists</CardTitle>
          </div>
          <CardDescription>Artists listened to by two or more friends</CardDescription>
        </CardHeader>
        <CardContent>
          {report.shared_artists.length === 0 ? (
            <p className="font-mono text-xs text-[var(--ink-muted)]">No shared artists yet.</p>
          ) : (
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
              {report.shared_artists.slice(0, 30).map(([artist, friends, plays]) => (
                <div
                  key={artist}
                  className="bside-card-interactive flex flex-col justify-between gap-2.5 rounded-[var(--radius-control)] bg-[var(--bg-inset)] p-3"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-mono text-xs font-semibold text-[var(--ink-primary)]">
                      {artist}
                    </span>
                    <span className="shrink-0 font-mono text-[10px] uppercase text-[var(--accent-primary)] tabular-nums">
                      {plays} plays
                    </span>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {friends.map((f) => (
                      <UserPill
                        key={f}
                        name={f}
                        avatarMap={avatarMap}
                        onFilterByUser={onFilterByUser}
                      />
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
