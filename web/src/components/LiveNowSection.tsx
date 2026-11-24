import { useState } from 'react';
import {
  ArrowRight,
  ChevronDown,
  ChevronUp,
  ExternalLink,
  Lock,
  Radio,
  RefreshCw,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { VinylSleeve } from '@/components/VinylSleeve';
import { PixelDot, PixelEq } from '@/components/PixelEq';
import { cn, formatContextLabel, formatDuration, formatRelativeTime } from '@/lib/utils';
import type { LiveFriendItem, LiveSnapshot } from '@/types/api';

interface LiveNowSectionProps {
  snapshot: LiveSnapshot | null;
  loading: boolean;
  polling: boolean;
  now: number;
  onPollNow: () => void;
  onSelectFriend: (userUri: string) => void;
  variant?: 'full' | 'compact';
  onOpenLiveTab?: () => void;
}

function formatContextTag(item: LiveFriendItem): string {
  const base =
    item.context_name ||
    (item.context_type ? formatContextLabel(item.context_type) : null);
  if (!base) return item.context_is_private ? 'PRIVATE' : 'DIRECT PLAY';
  return base.toUpperCase();
}

function getStatusBanner(snapshot: LiveSnapshot | null, loading: boolean) {
  if (loading && !snapshot) {
    return {
      tone: 'muted' as const,
      label: 'CONNECTING',
      text: 'Opening realtime stream to local Spotify Friend Activity daemon...',
    };
  }
  if (!snapshot) {
    return {
      tone: 'muted' as const,
      label: 'OFFLINE',
      text: 'Realtime live feed endpoint is unavailable. Historical SQLite analytics remain active.',
    };
  }
  if (snapshot.connected && snapshot.status === 'live') {
    return null;
  }
  switch (snapshot.status) {
    case 'connecting':
      return {
        tone: 'amber' as const,
        label: 'CONNECTING',
        text: snapshot.message || 'Connecting to Spotify via librespot...',
      };
    case 'unauthenticated':
      return {
        tone: 'amber' as const,
        label: 'UNAUTHENTICATED',
        text: snapshot.message || 'Run `bside auth` to sign in and enable live friend activity.',
      };
    case 'error':
      return {
        tone: 'rust' as const,
        label: 'STREAM ERROR',
        text: snapshot.message || 'Live feed encountered an upstream error. Retrying automatically.',
      };
    default:
      return {
        tone: 'muted' as const,
        label: snapshot.status ? snapshot.status.toUpperCase() : 'OFFLINE',
        text: snapshot.message || 'Live polling is currently paused or offline.',
      };
  }
}

export function LiveNowSection({
  snapshot,
  loading,
  polling,
  now,
  onPollNow,
  onSelectFriend,
  variant = 'full',
  onOpenLiveTab,
}: LiveNowSectionProps) {
  const [collapsed, setCollapsed] = useState(false);

  const friends = snapshot?.friends ?? [];
  const spinningFriends = friends.filter((f) => f.playing);
  const recentFriends = friends.filter((f) => !f.playing);
  const banner = getStatusBanner(snapshot, loading);
  const lastSyncLabel = snapshot?.updated_at_ms
    ? formatRelativeTime(snapshot.updated_at_ms).toUpperCase()
    : 'AWAITING SYNC';

  if (variant === 'compact') {
    return (
      <section className="border border-[#2b2b27] bg-[#181816]">
        {/* Compact Strip Header */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[#2b2b27] bg-[#141412] px-4 py-2.5">
          <div className="flex items-center gap-2.5">
            <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
              00 // ON AIR
            </span>
            <span className="h-3 w-px bg-[#2b2b27]" />
            <span className="inline-flex items-center gap-2 font-mono text-[11px] font-semibold uppercase tracking-wider text-[#efeee8]">
              <PixelDot
                active={spinningFriends.length > 0}
                tone={spinningFriends.length > 0 ? 'sage' : banner ? banner.tone : 'muted'}
              />
              {spinningFriends.length > 0
                ? `${spinningFriends.length} SPINNING NOW`
                : banner
                  ? banner.label
                  : '0 SPINNING NOW'}
            </span>
            {snapshot?.updated_at_ms && (
              <span className="hidden sm:inline font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                &middot; SYNCED {lastSyncLabel}
              </span>
            )}
          </div>

          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={onPollNow}
              disabled={polling}
              className="h-6 px-2 text-[10px]"
            >
              <RefreshCw className={cn('h-3 w-3', polling && 'animate-spin')} />
              POLL
            </Button>
            {onOpenLiveTab && (
              <Button
                variant="outline"
                size="sm"
                onClick={onOpenLiveTab}
                className="h-6 px-2 text-[10px]"
              >
                LIVE DECK
                <ArrowRight className="h-3 w-3" />
              </Button>
            )}
            <button
              type="button"
              onClick={() => setCollapsed((c) => !c)}
              className="inline-flex h-6 w-6 items-center justify-center border border-[#2b2b27] bg-[#181816] text-[#8d8c84] hover:border-[#45453e] hover:text-[#efeee8] cursor-pointer"
              title={collapsed ? 'Expand spinning now strip' : 'Collapse spinning now strip'}
            >
              {collapsed ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronUp className="h-3.5 w-3.5" />}
            </button>
          </div>
        </div>

        {!collapsed && (
          <div className="p-4">
            {banner && (
              <div className="flex items-center justify-between gap-3 border border-[#2b2b27] bg-[#141412] px-3 py-2 font-mono text-[11px] text-[#8d8c84]">
                <span className="flex items-center gap-2">
                  <PixelDot active={banner.tone === 'amber'} tone={banner.tone} />
                  <strong className="text-[#efeee8]">{banner.label}:</strong> {banner.text}
                </span>
              </div>
            )}

            {!banner && spinningFriends.length === 0 && (
              <div className="flex flex-wrap items-center justify-between gap-2 font-mono text-[11px] text-[#8d8c84]">
                <span>No friends are actively spinning right now.</span>
                {recentFriends.length > 0 && onOpenLiveTab && (
                  <button
                    type="button"
                    onClick={onOpenLiveTab}
                    className="text-[#89b471] underline underline-offset-4 hover:text-[#efeee8] cursor-pointer"
                  >
                    View {recentFriends.length} recently spun &rarr;
                  </button>
                )}
              </div>
            )}

            {spinningFriends.length > 0 && (
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                {spinningFriends.map((friend) => {
                  const elapsed =
                    friend.duration_ms != null && friend.duration_ms > 0
                      ? Math.min(Math.max(0, now - friend.timestamp_ms), friend.duration_ms)
                      : null;
                  const pct =
                    elapsed != null && friend.duration_ms
                      ? Math.min(100, (elapsed / friend.duration_ms) * 100)
                      : 0;

                  return (
                    <div
                      key={friend.user_uri}
                      className="flex items-center gap-3 border border-[#557445]/60 bg-[#141412] p-3 shadow-[3px_3px_0px_#557445]"
                    >
                      <VinylSleeve
                        coverUrl={friend.cover_url}
                        title={friend.track_name}
                        spinning
                        size="sm"
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center justify-between gap-2">
                          <button
                            type="button"
                            onClick={() => onSelectFriend(friend.user_uri)}
                            className="truncate font-mono text-[10px] font-semibold uppercase tracking-wider text-[#89b471] hover:underline cursor-pointer"
                            title={`Filter analytics by ${friend.user_name}`}
                          >
                            {friend.user_name}
                          </button>
                          <PixelEq active />
                        </div>
                        <a
                          href={friend.spotify_url}
                          target="_blank"
                          rel="noreferrer"
                          className="mt-0.5 block truncate text-xs font-semibold text-[#efeee8] hover:text-[#89b471]"
                        >
                          {friend.track_name}
                        </a>
                        <div className="truncate font-mono text-[10px] text-[#8d8c84]">
                          {friend.all_artists}
                        </div>
                        {friend.duration_ms != null && friend.duration_ms > 0 && (
                          <div className="mt-2">
                            <div className="h-[2px] w-full bg-[#2b2b27]">
                              <div
                                className="h-full bg-[#89b471] transition-[width] duration-1000 ease-linear"
                                style={{ width: `${pct}%` }}
                              />
                            </div>
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </section>
    );
  }

  // FULL ON AIR DECK VARIANT
  return (
    <div className="space-y-6">
      {/* Header Bar */}
      <div className="flex flex-wrap items-end justify-between gap-4 border-b border-[#2b2b27] pb-4">
        <div>
          <div className="mb-1 flex items-center gap-2 font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84]">
            <span>01 // REALTIME TURNTABLE DECK</span>
            {snapshot?.account && (
              <>
                <span>&middot;</span>
                <span className="text-[#efeee8]">@{snapshot.account}</span>
              </>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="font-display text-4xl font-extrabold uppercase leading-none tracking-tight text-[#efeee8]">
              ON AIR<span className="text-[#89b471]">.</span>
            </h2>
            <div className="inline-flex items-center gap-2 border border-[#2b2b27] bg-[#141412] px-2.5 py-1 font-mono text-[11px] font-semibold uppercase tracking-wider">
              <PixelDot
                active={spinningFriends.length > 0}
                tone={spinningFriends.length > 0 ? 'sage' : banner ? banner.tone : 'muted'}
              />
              <span className={spinningFriends.length > 0 ? 'text-[#89b471]' : 'text-[#8d8c84]'}>
                {spinningFriends.length} SPINNING NOW
              </span>
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <div className="text-right font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
            <div>LAST SYNCED: {lastSyncLabel}</div>
            {snapshot?.poll_interval_secs ? (
              <div>INTERVAL: {snapshot.poll_interval_secs}S SSE STREAM</div>
            ) : null}
          </div>
          <Button
            variant="primary"
            size="md"
            onClick={onPollNow}
            disabled={polling}
          >
            <RefreshCw className={cn('h-3.5 w-3.5', polling && 'animate-spin')} />
            POLL NOW
          </Button>
        </div>
      </div>

      {/* Status Banner when not connected/live */}
      {banner && (
        <div className="flex flex-wrap items-center justify-between gap-3 border border-[#45453e] bg-[#181816] px-4 py-3 shadow-edge">
          <div className="flex items-center gap-3 font-mono text-xs">
            <PixelDot active={banner.tone === 'amber'} tone={banner.tone} />
            <div>
              <span className="font-semibold uppercase tracking-wider text-[#efeee8]">
                {banner.label}
              </span>
              <span className="mx-2 text-[#45453e]">/</span>
              <span className="text-[#8d8c84]">{banner.text}</span>
            </div>
          </div>
          <Button variant="outline" size="sm" onClick={onPollNow} disabled={polling}>
            RETRY
          </Button>
        </div>
      )}

      {/* Spinning Now Grid */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 font-mono text-[11px] font-semibold uppercase tracking-widest text-[#efeee8]">
            <Radio className="h-3.5 w-3.5 text-[#89b471]" />
            <span>ACTIVE TURNTABLES</span>
            <span className="text-[#8d8c84]">({spinningFriends.length})</span>
          </div>
          <span className="font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
            CLICK FRIEND NAME TO FILTER ANALYTICS
          </span>
        </div>

        {spinningFriends.length === 0 ? (
          <div className="border border-dashed border-[#2b2b27] bg-[#141412] px-6 py-10 text-center">
            <div className="mx-auto mb-3 flex justify-center">
              <PixelEq active={false} />
            </div>
            <p className="font-display text-xl font-bold uppercase tracking-tight text-[#efeee8]">
              ALL TURNTABLES IDLE
            </p>
            <p className="mt-1 font-mono text-[11px] text-[#8d8c84]">
              No friends are actively playing right now. Live vinyls will spin up automatically over SSE.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            {spinningFriends.map((friend) => {
              const hasDuration = friend.duration_ms != null && friend.duration_ms > 0;
              const elapsed = hasDuration
                ? Math.min(Math.max(0, now - friend.timestamp_ms), friend.duration_ms!)
                : null;
              const pct =
                elapsed != null && friend.duration_ms
                  ? Math.min(100, (elapsed / friend.duration_ms) * 100)
                  : 0;

              return (
                <article
                  key={friend.user_uri}
                  className="group relative flex flex-col justify-between border border-[#557445] bg-[#181816] p-4 shadow-moss"
                >
                  {/* Top metadata row: Friend + Live EQ */}
                  <div className="mb-3 flex items-center justify-between gap-2 border-b border-[#2b2b27] pb-2.5">
                    <div className="flex min-w-0 items-center gap-2">
                      <PixelDot active tone="sage" />
                      <button
                        type="button"
                        onClick={() => onSelectFriend(friend.user_uri)}
                        className="truncate font-mono text-xs font-semibold uppercase tracking-wider text-[#efeee8] hover:text-[#89b471] cursor-pointer"
                        title={`Inspect ${friend.user_name}'s analytics`}
                      >
                        {friend.user_name}
                      </button>
                    </div>
                    <div className="flex shrink-0 items-center gap-2.5">
                      <PixelEq active />
                      <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[#89b471]">
                        SPINNING
                      </span>
                    </div>
                  </div>

                  {/* Center body: Spinning Vinyl + Track Metadata */}
                  <div className="flex flex-col gap-4 sm:flex-row sm:items-center">
                    <VinylSleeve
                      coverUrl={friend.cover_url}
                      title={friend.track_name}
                      spinning
                      size="lg"
                    />

                    <div className="min-w-0 flex-1 space-y-2">
                      <div>
                        <a
                          href={friend.spotify_url}
                          target="_blank"
                          rel="noreferrer"
                          className="group/link inline-flex max-w-full items-center gap-1.5 font-display text-2xl font-bold leading-tight tracking-tight text-[#efeee8] hover:text-[#89b471]"
                        >
                          <span className="truncate">{friend.track_name}</span>
                          <ExternalLink className="h-3.5 w-3.5 shrink-0 opacity-50 group-hover/link:opacity-100" />
                        </a>
                        <div className="truncate font-mono text-xs text-[#8d8c84]">
                          {friend.all_artists}
                        </div>
                        {friend.album_name && (
                          <div className="mt-0.5 truncate font-mono text-[10px] text-[#6f6e67]">
                            {friend.album_name}
                            {friend.release_date ? ` · ${friend.release_date.slice(0, 4)}` : ''}
                          </div>
                        )}
                      </div>

                      {/* Badges: Context, Explicit, Popularity */}
                      <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                        <Badge
                          variant={friend.context_is_private ? 'warning' : 'secondary'}
                          className="max-w-[220px]"
                        >
                          {friend.context_is_private && <Lock className="h-2.5 w-2.5 shrink-0" />}
                          <span className="truncate">
                            {formatContextTag(friend)}
                            {friend.context_is_private ? ' [PRIVATE]' : ''}
                          </span>
                        </Badge>
                        {friend.explicit && <Badge variant="outline">[E]</Badge>}
                        {friend.popularity != null && (
                          <Badge variant="outline">POP {friend.popularity}</Badge>
                        )}
                      </div>
                    </div>
                  </div>

                  {/* Bottom Live Playback Progress Bar */}
                  <div className="mt-4 pt-3 border-t border-[#2b2b27]/80">
                    <div className="mb-1.5 flex items-center justify-between font-mono text-[10px] uppercase tracking-wider text-[#8d8c84]">
                      <span>PLAYBACK POSITION</span>
                      <span className="tabular-nums text-[#efeee8]">
                        {elapsed != null && friend.duration_ms
                          ? `${formatDuration(elapsed)} / ${formatDuration(friend.duration_ms)}`
                          : 'LIVE STREAM'}
                      </span>
                    </div>
                    <div className="h-[2px] w-full bg-[#2b2b27]">
                      <div
                        className="h-full bg-[#89b471] transition-[width] duration-1000 ease-linear"
                        style={{ width: hasDuration ? `${pct}%` : '100%' }}
                      />
                    </div>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </section>

      {/* Recently Spun Row */}
      {recentFriends.length > 0 && (
        <section className="space-y-3 pt-2">
          <div className="flex items-center gap-2 font-mono text-[11px] font-semibold uppercase tracking-widest text-[#8d8c84]">
            <span>02 // RECENTLY SPUN</span>
            <span>({recentFriends.length})</span>
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {recentFriends.map((friend) => (
              <article
                key={friend.user_uri}
                className="flex items-center gap-3.5 border border-[#2b2b27] bg-[#181816] p-3 transition-colors hover:border-[#45453e] hover:bg-[#20201d]"
              >
                <VinylSleeve
                  coverUrl={friend.cover_url}
                  title={friend.track_name}
                  spinning={false}
                  size="sm"
                />

                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between gap-2">
                    <button
                      type="button"
                      onClick={() => onSelectFriend(friend.user_uri)}
                      className="truncate font-mono text-[10px] font-semibold uppercase tracking-wider text-[#efeee8] hover:text-[#89b471] cursor-pointer"
                      title={`Inspect ${friend.user_name}'s analytics`}
                    >
                      {friend.user_name}
                    </button>
                    <span className="shrink-0 font-mono text-[10px] uppercase text-[#8d8c84]">
                      {formatRelativeTime(friend.timestamp_ms).toUpperCase()}
                    </span>
                  </div>

                  <a
                    href={friend.spotify_url}
                    target="_blank"
                    rel="noreferrer"
                    className="mt-0.5 block truncate text-xs font-medium text-[#efeee8] hover:text-[#89b471]"
                  >
                    {friend.track_name}
                  </a>
                  <div className="truncate font-mono text-[10px] text-[#8d8c84]">
                    {friend.all_artists}
                  </div>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
