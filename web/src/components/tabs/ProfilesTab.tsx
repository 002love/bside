import { useEffect, useState } from 'react';
import { ExternalLink, History, ListMusic } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { EmptyState } from '@/components/EmptyState';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { formatRelativeTime, spotifyUriToUrl } from '@/lib/utils';
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

function eventBadgeVariant(
  eventType: string,
): 'default' | 'warning' | 'destructive' | 'info' | 'secondary' {
  switch (eventType) {
    case 'playlist_added':
      return 'default';
    case 'playlist_removed':
      return 'destructive';
    case 'rename':
      return 'warning';
    case 'followers':
      return 'info';
    default:
      return 'secondary';
  }
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
    <div className="space-y-6 animate-fade-up">
      {/* Synced Profiles */}
      {profiles.length > 0 && (
        <section className="space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="font-mono text-[11px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
              Synced Profiles ({profiles.length})
            </h2>
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {profiles.map((profile) => {
              const profileUrl = spotifyUriToUrl(profile.user_uri);
              const avatarUrl =
                profile.image_url ??
                lookupAvatar(avatarMap, profile.user_uri, profile.user_name);

              return (
                <Card key={profile.user_uri}>
                  <CardHeader className="flex-row items-center gap-3.5 border-b border-[var(--border-subtle)] pb-3.5">
                    <button
                      type="button"
                      onClick={() => onFilterByUser?.(profile.user_uri)}
                      className="shrink-0 cursor-pointer"
                      title={`Filter analytics by ${profile.user_name}`}
                    >
                      <UserAvatar
                        name={profile.user_name}
                        imageUrl={avatarUrl}
                        size="lg"
                        isSelf={profile.is_self}
                      />
                    </button>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => onFilterByUser?.(profile.user_uri)}
                          className="truncate text-left transition-colors hover:text-[var(--accent-primary)] cursor-pointer"
                          title={`Filter analytics by ${profile.user_name}`}
                        >
                          <CardTitle className="truncate">{profile.user_name}</CardTitle>
                        </button>
                        {profile.is_self && (
                          <Badge variant="default" className="px-1.5 py-0 text-[9px]">
                            YOU
                          </Badge>
                        )}
                        {profileUrl && (
                          <a
                            href={profileUrl}
                            target="_blank"
                            rel="noreferrer"
                            className="text-[var(--ink-muted)] transition-colors hover:text-[var(--accent-primary)]"
                            title="Open Spotify profile"
                          >
                            <ExternalLink className="h-3.5 w-3.5" />
                          </a>
                        )}
                      </div>
                      <CardDescription className="mt-0.5 font-mono text-[10px] uppercase tracking-wider">
                        {profile.followers_count != null
                          ? `${profile.followers_count.toLocaleString()} followers`
                          : 'Followers hidden'}
                        {profile.following_count != null
                          ? ` · ${profile.following_count.toLocaleString()} following`
                          : ''}
                        {' · '}Synced {formatRelativeTime(profile.updated_at_ms)}
                      </CardDescription>
                    </div>
                  </CardHeader>

                  <CardContent className="space-y-4 pt-4">
                    <div>
                      <div className="mb-2 flex items-center gap-1.5 font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
                        <ListMusic className="h-3 w-3 text-[var(--accent-primary)]" />
                        Public Playlists ({profile.public_playlists.length})
                      </div>
                      {profile.public_playlists.length === 0 ? (
                        <p className="text-xs text-[var(--ink-muted)]">No public playlists listed.</p>
                      ) : (
                        <div className="space-y-1.5">
                          {profile.public_playlists.slice(0, 8).map((pl) => {
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
                                className="group flex items-center justify-between gap-2.5 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2.5 py-1.5 text-xs transition-all hover:border-[var(--border-accent)] hover:bg-[var(--bg-hover)] cursor-pointer"
                              >
                                <div className="flex min-w-0 items-center gap-2.5">
                                  <PlaylistCover
                                    imageUrl={pl.image_url}
                                    name={pl.name}
                                    contextType="playlist"
                                    size="sm"
                                  />
                                  <span className="truncate font-medium text-[var(--ink-primary)] transition-colors group-hover:text-[var(--accent-primary)]">
                                    {pl.name}
                                  </span>
                                </div>
                                <div className="flex shrink-0 items-center gap-2">
                                  {pl.followers_count != null && (
                                    <span className="font-mono text-[10px] text-[var(--ink-muted)] tabular-nums">
                                      {pl.followers_count.toLocaleString()}
                                    </span>
                                  )}
                                  {plUrl && (
                                    <a
                                      href={plUrl}
                                      target="_blank"
                                      rel="noreferrer"
                                      onClick={(e) => e.stopPropagation()}
                                      className="inline-flex h-6 w-6 items-center justify-center rounded-[var(--radius-control)] text-[var(--ink-muted)] transition-colors hover:bg-[var(--bg-panel)] hover:text-[var(--accent-primary)]"
                                      title="Open in Spotify"
                                      aria-label={`Open ${pl.name} in Spotify`}
                                    >
                                      <ExternalLink className="h-3 w-3" />
                                    </a>
                                  )}
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>

                    {profile.recently_played_artists.length > 0 && (
                      <>
                        <Separator />
                        <div>
                          <div className="mb-2 font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--ink-muted)]">
                            Recently Played Artists
                          </div>
                          <div className="flex flex-wrap gap-1.5">
                            {profile.recently_played_artists.slice(0, 12).map((artist) => {
                              const artistUrl = spotifyUriToUrl(artist.uri);
                              return artistUrl ? (
                                <a
                                  key={artist.uri || artist.name}
                                  href={artistUrl}
                                  target="_blank"
                                  rel="noreferrer"
                                >
                                  <Badge
                                    variant="secondary"
                                    className="hover:border-[var(--border-accent)] hover:text-[var(--accent-primary)]"
                                  >
                                    {artist.name}
                                  </Badge>
                                </a>
                              ) : (
                                <Badge key={artist.name} variant="secondary">
                                  {artist.name}
                                </Badge>
                              );
                            })}
                          </div>
                        </div>
                      </>
                    )}
                  </CardContent>
                </Card>
              );
            })}
          </div>
        </section>
      )}

      {/* Chronological Changelog */}
      <section className="space-y-3">
        <Card>
          <CardHeader className="border-b border-[var(--border-subtle)]">
            <div className="flex items-center gap-2">
              <History className="h-4 w-4 text-[var(--accent-primary)]" />
              <CardTitle>Profile Changelog</CardTitle>
            </div>
            <CardDescription>
              Renames, follower changes, public playlist updates, and new artists
            </CardDescription>
          </CardHeader>
          <CardContent className="pt-2">
            {events.length === 0 ? (
              <p className="py-6 text-center text-xs text-[var(--ink-muted)]">
                No profile changes detected in this timeframe.
              </p>
            ) : (
              <div className="divide-y divide-[var(--border-subtle)]">
                {events.map((ev) => {
                  const avatarUrl = lookupAvatar(avatarMap, ev.user_uri, ev.user_name);
                  return (
                    <div
                      key={ev.id}
                      className="flex flex-wrap items-center justify-between gap-2 py-2.5 text-xs"
                    >
                      <div className="flex min-w-0 items-center gap-2.5">
                        <Badge variant={eventBadgeVariant(ev.event_type)} className="shrink-0">
                          {ev.event_type.replace(/_/g, ' ')}
                        </Badge>
                        <button
                          type="button"
                          onClick={() => onFilterByUser?.(ev.user_uri)}
                          className="inline-flex shrink-0 items-center gap-1.5 font-semibold text-[var(--ink-primary)] transition-colors hover:text-[var(--accent-primary)] cursor-pointer"
                          title={`Filter analytics by ${ev.user_name}`}
                        >
                          <UserAvatar
                            name={ev.user_name}
                            imageUrl={avatarUrl}
                            size="xs"
                          />
                          <span>{ev.user_name}</span>
                        </button>
                        <span className="truncate text-[var(--ink-secondary)]">{ev.summary}</span>
                      </div>
                      <span className="shrink-0 font-mono text-[10px] text-[var(--ink-muted)] tabular-nums">
                        {ev.detected_at_local}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>
      </section>
    </div>
  );
}
