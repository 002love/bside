import { useEffect, useState } from 'react';
import { motion } from 'motion/react';
import { ExternalLink, Filter, History, ListMusic } from 'lucide-react';
import { AmbientArtGlow, EASE_OUT_EXPO, StaggerItem } from '@/components/ui/motion';
import { EmptyState } from '@/components/EmptyState';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { cn, formatRelativeTime, spotifyUriToUrl } from '@/lib/utils';
import {
  buildUsersParam,
  type AvatarMap,
  type ProfileEvent,
  type StoredProfile,
} from '@/types/api';

export interface ProfilesTabProps {
  selectedUsers: string[];
  days: number | null;
  refreshTick: number;
  avatarMap: AvatarMap;
  onSelectPlaylist?: (contextUri: string) => void;
  onFilterByUser?: (userUri: string) => void;
}

function eventNodeColor(eventType: string): string {
  switch (eventType) {
    case 'playlist_added':
      return 'bg-[var(--accent-ember)]';
    case 'playlist_removed':
      return 'bg-[var(--danger)]';
    case 'rename':
      return 'bg-[var(--warning)]';
    case 'followers':
      return 'bg-[var(--accent-blue)]';
    default:
      return 'bg-[var(--ink-muted)]';
  }
}

function formatEventLabel(eventType: string): string {
  const cleaned = eventType.replace(/_/g, ' ').trim();
  if (!cleaned) return 'Update';
  return cleaned.charAt(0).toUpperCase() + cleaned.slice(1);
}

export function ProfilesTab({
  selectedUsers,
  days,
  refreshTick,
  avatarMap,
  onSelectPlaylist,
  onFilterByUser,
}: ProfilesTabProps) {
  const [profiles, setProfiles] = useState<StoredProfile[]>([]);
  const [events, setEvents] = useState<ProfileEvent[]>([]);
  const [loading, setLoading] = useState(true);

  const usersParam = buildUsersParam(selectedUsers);

  useEffect(() => {
    let active = true;
    setLoading(true);

    const profileParams = new URLSearchParams();
    if (usersParam) profileParams.set('users', usersParam);

    const changeParams = new URLSearchParams();
    if (usersParam) changeParams.set('users', usersParam);
    if (days != null) changeParams.set('days', String(days));
    changeParams.set('limit', '100');

    Promise.all([
      fetch(`/api/profiles?${profileParams.toString()}`).then((r) => r.json()),
      fetch(`/api/changes?${changeParams.toString()}`).then((r) => r.json()),
    ])
      .then(([pData, cData]) => {
        if (!active) return;
        setProfiles(Array.isArray(pData) ? pData : []);
        setEvents(Array.isArray(cData) ? cData : []);
      })
      .catch(() => {
        if (!active) return;
        setProfiles([]);
        setEvents([]);
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [usersParam, days, refreshTick]);

  if (loading && profiles.length === 0 && events.length === 0) {
    return (
      <div className="space-y-6">
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="bside-card h-64 bg-[var(--bg-surface)]/70 p-5" />
          ))}
        </div>
      </div>
    );
  }

  if (!loading && profiles.length === 0 && events.length === 0) {
    return (
      <EmptyState
        title="No friend profiles synced yet"
        description="Sync friend profiles to track public playlists, recently played artists, follower counts, and profile changelogs."
        command="bside profiles --refresh"
      />
    );
  }

  return (
    <div className="space-y-8">
      {/* Friend Directory */}
      {profiles.length > 0 && (
        <section className="space-y-4">
          <div className="flex items-baseline justify-between gap-2">
            <div>
              <h2 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
                Friend directory
              </h2>
              <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
                Synced public playlists and recently played artists
              </p>
            </div>
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {profiles.length} {profiles.length === 1 ? 'profile' : 'profiles'}
            </span>
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {profiles.map((profile, idx) => {
              const profileUrl = spotifyUriToUrl(profile.user_uri);
              const avatarUrl =
                profile.image_url ??
                lookupAvatar(avatarMap, profile.user_uri, profile.user_name);
              const ambientImage =
                profile.public_playlists.find((pl) => pl.image_url)?.image_url ?? avatarUrl;

              const followerMeta = [
                profile.followers_count != null
                  ? `${profile.followers_count.toLocaleString()} followers`
                  : 'Followers hidden',
                profile.following_count != null
                  ? `${profile.following_count.toLocaleString()} following`
                  : null,
                `Synced ${formatRelativeTime(profile.updated_at_ms)}`,
              ]
                .filter(Boolean)
                .join(' · ');

              return (
                <motion.article
                  key={profile.user_uri}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{
                    duration: 0.24,
                    delay: Math.min(idx * 0.03, 0.18),
                    ease: EASE_OUT_EXPO,
                  }}
                  className="bside-card relative flex flex-col overflow-hidden"
                >
                  {/* Subtle top ambient tint */}
                  {ambientImage ? (
                    <AmbientArtGlow
                      imageUrl={ambientImage}
                      intensity="subtle"
                      className="h-36 bottom-auto"
                    />
                  ) : (
                    <div
                      aria-hidden="true"
                      className="pointer-events-none absolute inset-x-0 top-0 h-28 bg-gradient-to-b from-white/[0.03] to-transparent"
                    />
                  )}

                  {/* Profile Header */}
                  <div className="relative z-10 flex items-start justify-between gap-3 border-b border-[var(--border-subtle)] p-5">
                    <div className="flex min-w-0 items-center gap-3.5">
                      <button
                        type="button"
                        onClick={() => onFilterByUser?.(profile.user_uri)}
                        className="tactile-press shrink-0 rounded-full cursor-pointer"
                        title={`Filter stats by ${profile.user_name}`}
                      >
                        <UserAvatar
                          name={profile.user_name}
                          imageUrl={avatarUrl}
                          size="lg"
                          isSelf={profile.is_self}
                        />
                      </button>

                      <div className="min-w-0">
                        <div className="flex items-center gap-1.5">
                          <button
                            type="button"
                            onClick={() => onFilterByUser?.(profile.user_uri)}
                            className="truncate text-left text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-ember)] cursor-pointer"
                            title={`Filter stats by ${profile.user_name}`}
                          >
                            {profile.user_name}
                          </button>
                          {profile.is_self && (
                            <span className="text-xs text-[var(--ink-muted)]">· You</span>
                          )}
                          {profileUrl && (
                            <a
                              href={profileUrl}
                              target="_blank"
                              rel="noreferrer"
                              className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-[var(--ink-muted)] transition-colors hover:bg-white/[0.06] hover:text-[var(--ink-primary)]"
                              title="Open Spotify profile"
                              aria-label={`Open ${profile.user_name} on Spotify`}
                            >
                              <ExternalLink className="h-3.5 w-3.5" />
                            </a>
                          )}
                        </div>
                        <p className="mt-0.5 truncate text-xs text-[var(--ink-secondary)] tabular-nums">
                          {followerMeta}
                        </p>
                      </div>
                    </div>

                    {onFilterByUser && (
                      <button
                        type="button"
                        onClick={() => onFilterByUser(profile.user_uri)}
                        className="tactile-press inline-flex shrink-0 items-center gap-1.5 rounded-[8px] border border-[var(--border-subtle)] bg-[var(--bg-inset)]/80 px-2.5 py-1.5 text-xs font-medium text-[var(--ink-secondary)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--ink-primary)] cursor-pointer"
                      >
                        <Filter className="h-3 w-3" />
                        <span>Filter stats</span>
                      </button>
                    )}
                  </div>

                  {/* Playlists Horizontal Shelf */}
                  <div className="relative z-10 flex flex-1 flex-col justify-between gap-5 p-5">
                    <div>
                      <div className="mb-3 flex items-center justify-between text-xs">
                        <span className="inline-flex items-center gap-1.5 font-medium text-[var(--ink-secondary)]">
                          <ListMusic className="h-3.5 w-3.5 text-[var(--ink-muted)]" />
                          Public playlists
                        </span>
                        <span className="text-[var(--ink-muted)] tabular-nums">
                          {profile.public_playlists.length}
                        </span>
                      </div>

                      {profile.public_playlists.length === 0 ? (
                        <p className="text-xs text-[var(--ink-muted)]">
                          No public playlists listed.
                        </p>
                      ) : (
                        <div className="flex gap-3 overflow-x-auto pb-1.5">
                          {profile.public_playlists.slice(0, 12).map((pl) => {
                            const plUrl = spotifyUriToUrl(pl.uri);
                            return (
                              <div
                                key={pl.uri}
                                onClick={() => onSelectPlaylist?.(pl.uri)}
                                role="button"
                                tabIndex={0}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter' || e.key === ' ') {
                                    e.preventDefault();
                                    onSelectPlaylist?.(pl.uri);
                                  }
                                }}
                                title="Open playlist in Playlists tab"
                                className="tactile-press group flex w-[132px] shrink-0 flex-col gap-2 rounded-[10px] p-2 transition-colors hover:bg-[var(--bg-hover)] cursor-pointer"
                              >
                                <div className="relative flex items-start justify-between">
                                  <PlaylistCover
                                    imageUrl={pl.image_url}
                                    name={pl.name}
                                    contextType="playlist"
                                    size="md"
                                    hoverZoom
                                  />
                                  {plUrl && (
                                    <a
                                      href={plUrl}
                                      target="_blank"
                                      rel="noreferrer"
                                      onClick={(e) => e.stopPropagation()}
                                      className="inline-flex h-6 w-6 items-center justify-center rounded-md text-[var(--ink-muted)] opacity-0 transition-all hover:bg-white/[0.08] hover:text-[var(--ink-primary)] group-hover:opacity-100"
                                      title="Open in Spotify"
                                      aria-label={`Open ${pl.name} in Spotify`}
                                    >
                                      <ExternalLink className="h-3 w-3" />
                                    </a>
                                  )}
                                </div>

                                <div className="min-w-0">
                                  <div className="truncate text-xs font-medium text-[var(--ink-primary)] transition-colors group-hover:text-[var(--accent-ember)]">
                                    {pl.name}
                                  </div>
                                  {pl.followers_count != null && (
                                    <div className="mt-0.5 text-[11px] text-[var(--ink-muted)] tabular-nums">
                                      {pl.followers_count.toLocaleString()}{' '}
                                      {pl.followers_count === 1 ? 'follower' : 'followers'}
                                    </div>
                                  )}
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>

                    {/* Recently Played Artists */}
                    {profile.recently_played_artists.length > 0 && (
                      <div className="border-t border-[var(--border-subtle)] pt-4">
                        <div className="mb-2 text-xs font-medium text-[var(--ink-secondary)]">
                          Recently played artists
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {profile.recently_played_artists.slice(0, 12).map((artist) => {
                            const artistUrl = spotifyUriToUrl(artist.uri);
                            const chipClass =
                              'inline-flex items-center rounded-[6px] bg-white/[0.04] px-2 py-0.5 text-xs text-[var(--ink-secondary)] transition-colors';
                            return artistUrl ? (
                              <a
                                key={artist.uri || artist.name}
                                href={artistUrl}
                                target="_blank"
                                rel="noreferrer"
                                className={cn(
                                  chipClass,
                                  'tactile-press hover:bg-white/[0.09] hover:text-[var(--ink-primary)]',
                                )}
                              >
                                {artist.name}
                              </a>
                            ) : (
                              <span key={artist.name} className={chipClass}>
                                {artist.name}
                              </span>
                            );
                          })}
                        </div>
                      </div>
                    )}
                  </div>
                </motion.article>
              );
            })}
          </div>
        </section>
      )}

      {/* Profile Activity Timeline */}
      <section className="bside-card">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-subtle)] px-5 py-4">
          <div>
            <div className="flex items-center gap-2">
              <History className="h-4 w-4 text-[var(--ink-muted)]" />
              <h2 className="text-[15px] font-semibold tracking-[-0.015em] text-[var(--ink-primary)]">
                Profile activity
              </h2>
            </div>
            <p className="mt-0.5 text-xs text-[var(--ink-secondary)]">
              Renames, follower changes, public playlist updates, and new artists
            </p>
          </div>
          {events.length > 0 && (
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {events.length} {events.length === 1 ? 'event' : 'events'}
            </span>
          )}
        </div>

        <div className="p-5">
          {events.length === 0 ? (
            <p className="py-6 text-center text-xs text-[var(--ink-muted)]">
              No profile changes detected in this timeframe.
            </p>
          ) : (
            <ol className="relative ml-2 border-l border-[var(--border-subtle)] space-y-4">
              {events.map((ev, idx) => {
                const avatarUrl = lookupAvatar(avatarMap, ev.user_uri, ev.user_name);
                return (
                  <li key={ev.id} className="relative pl-6">
                    {/* Crisp static timeline node (no pulsing) */}
                    <span
                      aria-hidden="true"
                      className={cn(
                        'absolute -left-[4.5px] top-[7px] h-2 w-2 rounded-full ring-[3px] ring-[var(--bg-surface)]',
                        eventNodeColor(ev.event_type),
                      )}
                    />

                    <StaggerItem index={idx}>
                      <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
                        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[13px]">
                          <button
                            type="button"
                            onClick={() => onFilterByUser?.(ev.user_uri)}
                            className="tactile-press inline-flex shrink-0 items-center gap-1.5 font-medium text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-ember)] cursor-pointer"
                            title={`Filter stats by ${ev.user_name}`}
                          >
                            <UserAvatar
                              name={ev.user_name}
                              imageUrl={avatarUrl}
                              size="xs"
                            />
                            <span>{ev.user_name}</span>
                          </button>

                          <span className="text-xs text-[var(--ink-muted)]">
                            {formatEventLabel(ev.event_type)}
                          </span>

                          <span className="text-[var(--ink-muted)]">·</span>

                          <span className="truncate text-xs text-[var(--ink-secondary)]">
                            {ev.summary}
                          </span>
                        </div>

                        <time
                          dateTime={ev.detected_at_local}
                          title={ev.detected_at_local}
                          className="shrink-0 font-mono text-[11px] text-[var(--ink-muted)] tabular-nums"
                        >
                          {formatRelativeTime(ev.detected_at_ms)}
                        </time>
                      </div>
                    </StaggerItem>
                  </li>
                );
              })}
            </ol>
          )}
        </div>
      </section>
    </div>
  );
}
