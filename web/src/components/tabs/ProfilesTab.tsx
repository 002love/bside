import { useEffect, useState } from 'react';
import { ExternalLink, History, ListMusic, User } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { EmptyState } from '@/components/EmptyState';
import { formatRelativeTime, spotifyUriToUrl } from '@/lib/utils';
import type { ProfileEvent, StoredProfile, TimeframeKey } from '@/types/api';

interface ProfilesTabProps {
  friend: string;
  timeframe: TimeframeKey;
  refreshTick: number;
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

export function ProfilesTab({ friend, timeframe, refreshTick }: ProfilesTabProps) {
  const [profiles, setProfiles] = useState<StoredProfile[]>([]);
  const [events, setEvents] = useState<ProfileEvent[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    setLoading(true);

    const profileParams = new URLSearchParams();
    if (friend) profileParams.set('friend', friend);

    const changeParams = new URLSearchParams();
    if (friend) changeParams.set('friend', friend);
    if (timeframe !== 'all') changeParams.set('days', timeframe);
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
  }, [friend, timeframe, refreshTick]);

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
    <div className="space-y-5">
      {/* Friend Profile Cards */}
      {profiles.length > 0 && (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {profiles.map((profile) => {
            const profileUrl = spotifyUriToUrl(profile.user_uri);
            return (
              <Card key={profile.user_uri}>
                <CardHeader className="flex-row items-center gap-3.5 pb-3">
                  {profile.image_url ? (
                    <img
                      src={profile.image_url}
                      alt={profile.user_name}
                      className="h-11 w-11 shrink-0 rounded-full border border-zinc-800 object-cover"
                    />
                  ) : (
                    <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full border border-zinc-800 bg-zinc-900 text-zinc-400">
                      <User className="h-5 w-5" />
                    </div>
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <CardTitle className="truncate text-sm">{profile.user_name}</CardTitle>
                      {profileUrl && (
                        <a
                          href={profileUrl}
                          target="_blank"
                          rel="noreferrer"
                          className="text-zinc-500 transition-colors hover:text-emerald-400"
                          title="Open Spotify profile"
                        >
                          <ExternalLink className="h-3.5 w-3.5" />
                        </a>
                      )}
                    </div>
                    <CardDescription className="mt-0.5">
                      {profile.followers_count != null
                        ? `${profile.followers_count.toLocaleString()} followers`
                        : 'Followers hidden'}
                      {profile.following_count != null
                        ? ` · ${profile.following_count.toLocaleString()} following`
                        : ''}
                      {' · '}Updated {formatRelativeTime(profile.updated_at_ms)}
                    </CardDescription>
                  </div>
                </CardHeader>

                <CardContent className="space-y-4">
                  <div>
                    <div className="mb-2 flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wider text-zinc-400">
                      <ListMusic className="h-3 w-3 text-emerald-400" />
                      Public Playlists ({profile.public_playlists.length})
                    </div>
                    {profile.public_playlists.length === 0 ? (
                      <p className="text-xs text-zinc-500">No public playlists listed.</p>
                    ) : (
                      <div className="space-y-1.5">
                        {profile.public_playlists.slice(0, 8).map((pl) => {
                          const plUrl = spotifyUriToUrl(pl.uri);
                          return (
                            <div
                              key={pl.uri}
                              className="flex items-center justify-between gap-2 rounded-lg border border-zinc-800/70 bg-zinc-950/60 px-2.5 py-1.5 text-xs"
                            >
                              <span className="truncate text-zinc-200">{pl.name}</span>
                              <div className="flex shrink-0 items-center gap-2">
                                {pl.followers_count != null && (
                                  <span className="font-mono text-[10px] text-zinc-500">
                                    {pl.followers_count}
                                  </span>
                                )}
                                {plUrl && (
                                  <a
                                    href={plUrl}
                                    target="_blank"
                                    rel="noreferrer"
                                    className="text-zinc-400 transition-colors hover:text-emerald-400"
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
                        <div className="mb-2 text-[11px] font-medium uppercase tracking-wider text-zinc-400">
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
                                  className="hover:border-emerald-500/40 hover:text-emerald-300"
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
      )}

      {/* Chronological Changelog */}
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <History className="h-4 w-4 text-emerald-400" />
            <CardTitle>Profile Changelog</CardTitle>
          </div>
          <CardDescription>
            Detected renames, follower deltas, public playlist additions/removals, and new artists
          </CardDescription>
        </CardHeader>
        <CardContent>
          {events.length === 0 ? (
            <p className="py-6 text-center text-xs text-zinc-500">
              No profile changes detected in this timeframe.
            </p>
          ) : (
            <div className="divide-y divide-zinc-800/70">
              {events.map((ev) => (
                <div
                  key={ev.id}
                  className="flex flex-wrap items-center justify-between gap-2 py-2.5 text-xs"
                >
                  <div className="flex min-w-0 items-center gap-2.5">
                    <Badge variant={eventBadgeVariant(ev.event_type)} className="shrink-0">
                      {ev.event_type.replace(/_/g, ' ')}
                    </Badge>
                    <span className="font-medium text-zinc-100">{ev.user_name}</span>
                    <span className="truncate text-zinc-400">{ev.summary}</span>
                  </div>
                  <span className="shrink-0 font-mono text-[11px] text-zinc-500">
                    {ev.detected_at_local}
                  </span>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
