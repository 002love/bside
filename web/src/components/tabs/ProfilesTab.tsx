import { useEffect, useMemo, useState, type CSSProperties } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { ExternalLink, Filter, History, ListMusic, Sparkles } from 'lucide-react';
import {
  AmbientArtGlow,
  EASE_OUT_EXPO,
  SPRING_SNAPPY,
  StaggerItem,
} from '@/components/ui/motion';
import { EmptyState } from '@/components/EmptyState';
import { PlaylistCover } from '@/components/PlaylistCover';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { useCoverAccent } from '@/lib/extractColor';
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

interface FriendPortraitButtonProps {
  profile: StoredProfile;
  avatarUrl: string | null;
  active: boolean;
  onSelect: () => void;
}

function FriendPortraitButton({
  profile,
  avatarUrl,
  active,
  onSelect,
}: FriendPortraitButtonProps) {
  const palette = useCoverAccent(avatarUrl, profile.user_uri, profile.color);

  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        'tactile-press group relative flex w-[76px] shrink-0 flex-col items-center gap-2 rounded-[10px] px-1.5 pt-1.5 pb-2.5 text-center transition-colors cursor-pointer',
        active ? 'bg-white/[0.04]' : 'hover:bg-white/[0.035]',
      )}
      aria-pressed={active}
      title={profile.user_name}
    >
      <div className="relative">
        <UserAvatar
          name={profile.user_name}
          imageUrl={avatarUrl}
          size="lg"
          isSelf={profile.is_self}
          ringColor={active ? palette.accent : `rgba(${palette.rgb}, 0.55)`}
          className={cn(
            'transition-transform duration-200',
            active ? 'scale-[1.03]' : 'opacity-85 group-hover:opacity-100 group-hover:scale-[1.03]',
          )}
        />
      </div>

      <span
        className={cn(
          'w-full truncate text-[12px] font-medium leading-tight transition-colors',
          active ? 'text-white' : 'text-[var(--ink-secondary)] group-hover:text-white',
        )}
      >
        {profile.user_name}
      </span>

      {active && (
        <motion.span
          layoutId="active-friend-tab"
          transition={SPRING_SNAPPY}
          style={{ backgroundColor: palette.accent }}
          className="absolute inset-x-3 bottom-0 h-[2.5px] rounded-full"
        />
      )}
    </button>
  );
}

interface EventsTimelineListProps {
  events: ProfileEvent[];
  avatarMap: AvatarMap;
  onFilterByUser?: (userUri: string) => void;
  emptyMessage: string;
}

function EventsTimelineList({
  events,
  avatarMap,
  onFilterByUser,
  emptyMessage,
}: EventsTimelineListProps) {
  if (events.length === 0) {
    return (
      <p className="py-8 text-sm text-[var(--ink-muted)]">
        {emptyMessage}
      </p>
    );
  }

  return (
    <ol className="relative ml-2 border-l border-white/[0.08] space-y-5">
      {events.map((ev, idx) => {
        const avatarUrl = lookupAvatar(avatarMap, ev.user_uri, ev.user_name);
        return (
          <li key={ev.id} className="relative pl-6">
            <span
              aria-hidden="true"
              className={cn(
                'absolute -left-[4.5px] top-[7px] h-2 w-2 rounded-full ring-[3px] ring-[#09090b]',
                eventNodeColor(ev.event_type),
              )}
            />

            <StaggerItem index={idx}>
              <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[13px]">
                  <button
                    type="button"
                    onClick={() => onFilterByUser?.(ev.user_uri)}
                    className="tactile-press inline-flex shrink-0 items-center gap-1.5 font-medium text-white transition-colors hover:text-[var(--accent-ember)] cursor-pointer"
                    title={`Filter stats by ${ev.user_name}`}
                  >
                    <UserAvatar
                      name={ev.user_name}
                      imageUrl={avatarUrl}
                      size="xs"
                    />
                    <span>{ev.user_name}</span>
                  </button>

                  <span className="text-xs font-medium text-[var(--ink-muted)]">
                    {formatEventLabel(ev.event_type)}
                  </span>

                  <span className="text-white/20" aria-hidden="true">
                    ·
                  </span>

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
  );
}

interface FriendSpotlightStageProps {
  profile: StoredProfile;
  avatarMap: AvatarMap;
  events: ProfileEvent[];
  onSelectPlaylist?: (contextUri: string) => void;
  onFilterByUser?: (userUri: string) => void;
}

function FriendSpotlightStage({
  profile,
  avatarMap,
  events,
  onSelectPlaylist,
  onFilterByUser,
}: FriendSpotlightStageProps) {
  const avatarUrl =
    profile.image_url ?? lookupAvatar(avatarMap, profile.user_uri, profile.user_name);
  const palette = useCoverAccent(avatarUrl, profile.user_uri, profile.color);
  const profileUrl = spotifyUriToUrl(profile.user_uri);

  const statParts = [
    profile.followers_count != null
      ? `${profile.followers_count.toLocaleString()} ${profile.followers_count === 1 ? 'follower' : 'followers'}`
      : 'Followers hidden',
    profile.following_count != null
      ? `${profile.following_count.toLocaleString()} following`
      : null,
    `Synced ${formatRelativeTime(profile.updated_at_ms)}`,
  ].filter(Boolean);

  const artistPillVars = {
    '--friend-soft': palette.soft,
    '--friend-border': `rgba(${palette.rgb}, 0.38)`,
  } as CSSProperties;

  return (
    <motion.div
      key={profile.user_uri}
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -4 }}
      transition={{ duration: 0.2, ease: EASE_OUT_EXPO }}
      className="w-full"
    >
      {/* Edge-to-Edge Artist-Page Hero */}
      <section className="relative w-full overflow-hidden border-b border-white/[0.07] px-6 pt-8 pb-8 lg:px-9">
        <AmbientArtGlow palette={palette} intensity="vivid" />

        <div className="relative z-10 flex flex-col gap-6 sm:flex-row sm:items-end sm:justify-between">
          <div className="flex flex-col gap-5 sm:flex-row sm:items-center">
            <div className="shrink-0 rounded-full shadow-[var(--shadow-art)]">
              <UserAvatar
                name={profile.user_name}
                imageUrl={avatarUrl}
                size="2xl"
                isSelf={profile.is_self}
                ringColor={palette.accent}
              />
            </div>

            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2.5">
                <span className="text-xs font-medium text-white/75">
                  {profile.is_self ? 'Your profile' : 'Friend profile'}
                </span>
                {profile.public_playlists.length > 0 && (
                  <>
                    <span className="text-white/30" aria-hidden="true">
                      ·
                    </span>
                    <span className="text-xs text-white/70 tabular-nums">
                      {profile.public_playlists.length}{' '}
                      {profile.public_playlists.length === 1
                        ? 'public playlist'
                        : 'public playlists'}
                    </span>
                  </>
                )}
              </div>

              <h1 className="mt-1.5 truncate text-3xl font-bold tracking-[-0.03em] text-white sm:text-4xl">
                {profile.user_name}
              </h1>

              <p className="mt-2 text-[13px] text-white/75 tabular-nums">
                {statParts.join(' · ')}
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2.5">
            {onFilterByUser && (
              <button
                type="button"
                onClick={() => onFilterByUser(profile.user_uri)}
                className="tactile-press inline-flex items-center gap-2 rounded-full bg-white px-4 py-2 text-xs font-semibold text-[#050507] transition-colors hover:bg-[#e6e6eb] cursor-pointer"
              >
                <Filter className="h-3.5 w-3.5" />
                <span>Filter stats</span>
              </button>
            )}

            {profileUrl && (
              <a
                href={profileUrl}
                target="_blank"
                rel="noreferrer"
                className="tactile-press inline-flex items-center gap-2 rounded-full border border-white/[0.15] bg-white/[0.08] px-4 py-2 text-xs font-medium text-white backdrop-blur-sm transition-colors hover:bg-white/[0.14]"
              >
                <span>Open in Spotify</span>
                <ExternalLink className="h-3.5 w-3.5" />
              </a>
            )}
          </div>
        </div>
      </section>

      {/* Below Hero on Deep #09090b (Cardless) */}
      <div className="w-full space-y-10 px-6 py-7 lg:px-9">
        {/* Public Playlists Shelf */}
        <section className="space-y-4">
          <div className="flex items-baseline justify-between gap-3">
            <div className="flex items-center gap-2">
              <ListMusic className="h-4 w-4 text-[var(--ink-muted)]" />
              <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
                Public playlists
              </h2>
            </div>
            <span className="text-xs text-[var(--ink-muted)] tabular-nums">
              {profile.public_playlists.length}{' '}
              {profile.public_playlists.length === 1 ? 'playlist' : 'playlists'}
            </span>
          </div>

          {profile.public_playlists.length === 0 ? (
            <p className="border-t border-white/[0.07] pt-5 text-sm text-[var(--ink-muted)]">
              No public playlists listed for {profile.user_name}.
            </p>
          ) : (
            <div className="grid grid-cols-2 gap-5 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
              {profile.public_playlists.map((pl, idx) => {
                const plUrl = spotifyUriToUrl(pl.uri);
                return (
                  <StaggerItem key={pl.uri} index={idx}>
                    <div
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
                      className="tactile-press group flex flex-col gap-2.5 rounded-[10px] p-1.5 -m-1.5 transition-colors hover:bg-white/[0.055] cursor-pointer"
                    >
                      <div className="relative">
                        <PlaylistCover
                          imageUrl={pl.image_url}
                          name={pl.name}
                          contextType="playlist"
                          size="fluid"
                          hoverZoom
                          className="shadow-[var(--shadow-art)]"
                        />
                        {plUrl && (
                          <a
                            href={plUrl}
                            target="_blank"
                            rel="noreferrer"
                            onClick={(e) => e.stopPropagation()}
                            className="absolute top-2 right-2 inline-flex h-7 w-7 items-center justify-center rounded-full bg-black/70 text-white/85 opacity-0 backdrop-blur-sm transition-all hover:bg-black/90 hover:text-white group-hover:opacity-100"
                            title="Open in Spotify"
                            aria-label={`Open ${pl.name} in Spotify`}
                          >
                            <ExternalLink className="h-3.5 w-3.5" />
                          </a>
                        )}
                      </div>

                      <div className="min-w-0 px-0.5">
                        <div className="truncate text-[13px] font-medium text-white transition-colors group-hover:text-[var(--accent-ember)]">
                          {pl.name}
                        </div>
                        {pl.followers_count != null && (
                          <div className="mt-0.5 text-xs text-[var(--ink-muted)] tabular-nums">
                            {pl.followers_count.toLocaleString()}{' '}
                            {pl.followers_count === 1 ? 'follower' : 'followers'}
                          </div>
                        )}
                      </div>
                    </div>
                  </StaggerItem>
                );
              })}
            </div>
          )}
        </section>

        {/* Heavy Rotation / Recently Played Artists */}
        {profile.recently_played_artists.length > 0 && (
          <section className="space-y-4 border-t border-white/[0.07] pt-8">
            <div className="flex items-baseline justify-between gap-3">
              <div className="flex items-center gap-2">
                <Sparkles className="h-4 w-4 text-[var(--ink-muted)]" />
                <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
                  Recently played artists
                </h2>
              </div>
              <span className="text-xs text-[var(--ink-muted)] tabular-nums">
                {profile.recently_played_artists.length}{' '}
                {profile.recently_played_artists.length === 1 ? 'artist' : 'artists'}
              </span>
            </div>

            <div className="flex flex-wrap gap-2" style={artistPillVars}>
              {profile.recently_played_artists.map((artist) => {
                const artistUrl = spotifyUriToUrl(artist.uri);
                const pillClass =
                  'inline-flex items-center gap-1.5 rounded-full border border-white/[0.09] bg-white/[0.03] px-3.5 py-1.5 text-[13px] font-medium text-[var(--ink-secondary)] transition-colors';
                return artistUrl ? (
                  <a
                    key={artist.uri || artist.name}
                    href={artistUrl}
                    target="_blank"
                    rel="noreferrer"
                    className={cn(
                      pillClass,
                      'tactile-press hover:border-[var(--friend-border)] hover:bg-[var(--friend-soft)] hover:text-white',
                    )}
                  >
                    <span>{artist.name}</span>
                  </a>
                ) : (
                  <span key={artist.name} className={pillClass}>
                    {artist.name}
                  </span>
                );
              })}
            </div>
          </section>
        )}

        {/* Profile Changes Timeline */}
        <section className="space-y-4 border-t border-white/[0.07] pt-8">
          <div className="flex items-baseline justify-between gap-3">
            <div className="flex items-center gap-2">
              <History className="h-4 w-4 text-[var(--ink-muted)]" />
              <h2 className="text-[16px] font-semibold tracking-[-0.02em] text-white">
                Profile changes
              </h2>
            </div>
            {events.length > 0 && (
              <span className="text-xs text-[var(--ink-muted)] tabular-nums">
                {events.length} {events.length === 1 ? 'event' : 'events'}
              </span>
            )}
          </div>

          <EventsTimelineList
            events={events}
            avatarMap={avatarMap}
            onFilterByUser={onFilterByUser}
            emptyMessage={`No profile changes detected for ${profile.user_name} in this timeframe.`}
          />
        </section>
      </div>
    </motion.div>
  );
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
  const [activeUri, setActiveUri] = useState<string | null>(null);
  const [showAllActivity, setShowAllActivity] = useState(false);

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

  // Keep active friend synchronized with loaded profiles and single-user filter
  useEffect(() => {
    if (profiles.length === 0) {
      setActiveUri(null);
      return;
    }

    if (selectedUsers.length === 1) {
      const target = selectedUsers[0].toLowerCase();
      const matched = profiles.find(
        (p) =>
          p.user_uri.toLowerCase() === target ||
          p.user_name.toLowerCase() === target,
      );
      if (matched) {
        setActiveUri(matched.user_uri);
        setShowAllActivity(false);
        return;
      }
    }

    setActiveUri((prev) => {
      if (prev && profiles.some((p) => p.user_uri === prev)) {
        return prev;
      }
      return profiles[0].user_uri;
    });
  }, [profiles, selectedUsers]);

  const activeProfile = useMemo(
    () => profiles.find((p) => p.user_uri === activeUri) ?? profiles[0] ?? null,
    [profiles, activeUri],
  );

  const activeProfileEvents = useMemo(() => {
    if (!activeProfile) return [];
    const targetUri = activeProfile.user_uri.toLowerCase();
    return events.filter((ev) => ev.user_uri.toLowerCase() === targetUri);
  }, [events, activeProfile]);

  if (loading && profiles.length === 0 && events.length === 0) {
    return (
      <div className="w-full">
        <div className="flex items-center gap-4 border-b border-white/[0.07] bg-[#060608]/70 px-6 py-4 lg:px-8">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="flex w-[76px] flex-col items-center gap-2">
              <div className="h-11 w-11 rounded-full bg-white/[0.06]" />
              <div className="h-2.5 w-12 rounded bg-white/[0.05]" />
            </div>
          ))}
        </div>
        <div className="border-b border-white/[0.07] px-6 py-10 lg:px-9">
          <div className="flex items-center gap-6">
            <div className="h-28 w-28 rounded-full bg-white/[0.06]" />
            <div className="space-y-3">
              <div className="h-3 w-24 rounded bg-white/[0.05]" />
              <div className="h-8 w-52 rounded bg-white/[0.07]" />
              <div className="h-3 w-44 rounded bg-white/[0.05]" />
            </div>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-5 px-6 py-8 sm:grid-cols-4 lg:grid-cols-6 lg:px-9">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="space-y-2.5">
              <div className="aspect-square w-full rounded-[10px] bg-white/[0.05]" />
              <div className="h-3 w-3/4 rounded bg-white/[0.05]" />
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (!loading && profiles.length === 0 && events.length === 0) {
    return (
      <div className="w-full px-6 py-10 lg:px-8">
        <EmptyState
          title="No friend profiles synced yet"
          description="Sync friend profiles to track public playlists, recently played artists, follower counts, and profile changelogs."
          command="bside profiles --refresh"
        />
      </div>
    );
  }

  return (
    <div className="w-full">
      {/* Top Interactive Friend Portrait Strip */}
      {profiles.length > 0 && (
        <div className="w-full border-b border-white/[0.07] bg-[#060608]/70 px-6 py-4 lg:px-8">
          <div className="flex items-center justify-between gap-4">
            <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto pb-0.5">
              {profiles.map((profile) => {
                const avatarUrl =
                  profile.image_url ??
                  lookupAvatar(avatarMap, profile.user_uri, profile.user_name);
                const isSelected = !showAllActivity && activeProfile?.user_uri === profile.user_uri;

                return (
                  <FriendPortraitButton
                    key={profile.user_uri}
                    profile={profile}
                    avatarUrl={avatarUrl}
                    active={isSelected}
                    onSelect={() => {
                      setActiveUri(profile.user_uri);
                      setShowAllActivity(false);
                    }}
                  />
                );
              })}
            </div>

            <div className="shrink-0 pl-2">
              <button
                type="button"
                onClick={() => setShowAllActivity((prev) => !prev)}
                aria-pressed={showAllActivity}
                className={cn(
                  'tactile-press relative inline-flex items-center gap-2 rounded-full border px-3.5 py-2 text-xs font-medium transition-colors cursor-pointer',
                  showAllActivity
                    ? 'border-white/25 bg-white text-[#050507]'
                    : 'border-white/[0.1] bg-white/[0.04] text-[var(--ink-secondary)] hover:border-white/[0.18] hover:text-white',
                )}
              >
                <History className="h-3.5 w-3.5" />
                <span>All activity</span>
                <span
                  className={cn(
                    'tabular-nums',
                    showAllActivity ? 'text-[#050507]/70' : 'text-[var(--ink-muted)]',
                  )}
                >
                  ({events.length})
                </span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Full-Bleed Friend Spotlight Stage or All-Circle Activity Changelog */}
      <AnimatePresence mode="wait">
        {showAllActivity || !activeProfile ? (
          <motion.div
            key="all-activity"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2, ease: EASE_OUT_EXPO }}
            className="w-full"
          >
            <div className="flex flex-wrap items-baseline justify-between gap-3 border-b border-white/[0.07] px-6 pt-7 pb-6 lg:px-9">
              <div>
                <h1 className="text-2xl font-bold tracking-[-0.03em] text-white">
                  Profile activity
                </h1>
                <p className="mt-1 text-xs text-[var(--ink-secondary)]">
                  Renames, follower changes, public playlist updates, and new artists across your circle
                </p>
              </div>
              <span className="text-xs text-[var(--ink-muted)] tabular-nums">
                {events.length} {events.length === 1 ? 'event' : 'events'}
              </span>
            </div>

            <div className="px-6 py-7 lg:px-9">
              <EventsTimelineList
                events={events}
                avatarMap={avatarMap}
                onFilterByUser={onFilterByUser}
                emptyMessage="No profile changes detected in this timeframe."
              />
            </div>
          </motion.div>
        ) : (
          <FriendSpotlightStage
            key={activeProfile.user_uri}
            profile={activeProfile}
            avatarMap={avatarMap}
            events={activeProfileEvents}
            onSelectPlaylist={onSelectPlaylist}
            onFilterByUser={onFilterByUser}
          />
        )}
      </AnimatePresence>
    </div>
  );
}
