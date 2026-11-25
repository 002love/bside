import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  BarChart3,
  Clock,
  Disc3,
  GitCompare,
  ListMusic,
  Palette,
  Radio,
  RefreshCw,
  Users,
  X,
} from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { TooltipProvider } from '@/components/ui/tooltip';
import { AccountBadge } from '@/components/AccountBadge';
import { FriendMultiSelect, type FriendOption } from '@/components/FriendMultiSelect';
import { InfoTooltip } from '@/components/InfoTooltip';
import { LiveNowSection } from '@/components/LiveNowSection';
import { PixelDot } from '@/components/PixelEq';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { OverviewTab } from '@/components/tabs/OverviewTab';
import { HistoryTab } from '@/components/tabs/HistoryTab';
import { PlaylistsTab } from '@/components/tabs/PlaylistsTab';
import { OverlapTab } from '@/components/tabs/OverlapTab';
import { ProfilesTab } from '@/components/tabs/ProfilesTab';
import { useLiveFeed } from '@/hooks/useLiveFeed';
import { useTheme } from '@/hooks/useTheme';
import { cn } from '@/lib/utils';
import type {
  AccountProfile,
  AvatarMap,
  FriendSummary,
  TimeframeKey,
} from '@/types/api';

const TIMEFRAMES: { key: TimeframeKey; label: string }[] = [
  { key: '7', label: '7D' },
  { key: '30', label: '30D' },
  { key: '90', label: '90D' },
  { key: 'all', label: 'ALL' },
];

export function App() {
  const [activeTab, setActiveTab] = useState('overview');
  const [selectedUsers, setSelectedUsers] = useState<string[]>([]);
  const [timeframe, setTimeframe] = useState<TimeframeKey>('30');
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [refreshTick, setRefreshTick] = useState(0);
  const [focusedContextUri, setFocusedContextUri] = useState<string | null>(null);

  const [friends, setFriends] = useState<FriendSummary[]>([]);
  const [loadingFriends, setLoadingFriends] = useState(false);

  const { theme, toggleTheme } = useTheme();
  const live = useLiveFeed();

  const days: number | null = timeframe === 'all' ? null : Number(timeframe);

  const triggerRefresh = useCallback(() => {
    setRefreshTick((t) => t + 1);
    void live.pollNow();
  }, [live.pollNow]);

  const handleSelectPlaylist = useCallback((contextUri: string) => {
    setFocusedContextUri(contextUri);
    setActiveTab('playlists');
  }, []);

  const handleFilterByUser = useCallback((userUri: string) => {
    setSelectedUsers((prev) => {
      if (prev.length === 1 && prev[0] === userUri) {
        return [];
      }
      return [userUri];
    });
  }, []);

  useEffect(() => {
    if (!autoRefresh) return;
    const id = window.setInterval(() => {
      setRefreshTick((t) => t + 1);
    }, 30_000);
    return () => window.clearInterval(id);
  }, [autoRefresh]);

  useEffect(() => {
    let active = true;
    setLoadingFriends(true);

    fetch('/api/friends')
      .then((res) => res.json())
      .then((data: unknown) => {
        if (!active) return;
        if (Array.isArray(data)) {
          setFriends(data as FriendSummary[]);
        }
      })
      .catch(() => {
        // Keep previous friend list if request fails
      })
      .finally(() => {
        if (active) setLoadingFriends(false);
      });

    return () => {
      active = false;
    };
  }, [refreshTick]);

  // Derive connected account profile from live snapshot or self friend summary
  const accountProfile = useMemo<AccountProfile | null>(() => {
    if (live.snapshot?.account_profile) {
      return live.snapshot.account_profile;
    }
    const selfFriend = friends.find((f) => f.is_self);
    if (selfFriend) {
      const username =
        live.snapshot?.account ||
        selfFriend.user_uri.split(':').pop() ||
        selfFriend.user_name;
      return {
        user_uri: selfFriend.user_uri,
        username,
        display_name: selfFriend.user_name,
        image_url: selfFriend.image_url,
      };
    }
    if (live.snapshot?.account_uri || live.snapshot?.account) {
      const username =
        live.snapshot.account ||
        live.snapshot.account_uri?.split(':').pop() ||
        'me';
      return {
        user_uri: live.snapshot.account_uri || `spotify:user:${username}`,
        username,
        display_name: live.snapshot.account || username,
        image_url: null,
      };
    }
    return null;
  }, [live.snapshot, friends]);

  const selfLiveItem = useMemo(() => {
    if (!live.snapshot?.friends) return null;
    return (
      live.snapshot.friends.find(
        (f) => f.is_self || (accountProfile && f.user_uri === accountProfile.user_uri),
      ) ?? null
    );
  }, [live.snapshot, accountProfile]);

  const isFilteredToSelf = Boolean(
    accountProfile &&
      selectedUsers.length === 1 &&
      selectedUsers[0] === accountProfile.user_uri,
  );

  const handleFilterSelf = useCallback(() => {
    if (!accountProfile) return;
    setSelectedUsers((prev) =>
      prev.length === 1 && prev[0] === accountProfile.user_uri
        ? []
        : [accountProfile.user_uri],
    );
  }, [accountProfile]);

  // Build deduplicated multi-select options combining /api/friends + live + accountProfile
  const friendOptions = useMemo<FriendOption[]>(() => {
    const map = new Map<string, FriendOption>();

    for (const f of friends) {
      map.set(f.user_uri, {
        user_uri: f.user_uri,
        user_name: f.user_name,
        image_url: f.image_url,
        play_count: f.total_plays,
        is_self: Boolean(
          f.is_self || (accountProfile && f.user_uri === accountProfile.user_uri),
        ),
        is_live: f.is_live,
      });
    }

    if (live.snapshot) {
      for (const lf of live.snapshot.friends) {
        const existing = map.get(lf.user_uri);
        map.set(lf.user_uri, {
          user_uri: lf.user_uri,
          user_name: existing?.user_name || lf.user_name,
          image_url: lf.user_image_url ?? existing?.image_url ?? null,
          play_count: existing?.play_count,
          is_self: Boolean(
            lf.is_self ||
              existing?.is_self ||
              (accountProfile && lf.user_uri === accountProfile.user_uri),
          ),
          is_live: lf.playing,
        });
      }
    }

    if (accountProfile && !map.has(accountProfile.user_uri)) {
      map.set(accountProfile.user_uri, {
        user_uri: accountProfile.user_uri,
        user_name: accountProfile.display_name || accountProfile.username,
        image_url: accountProfile.image_url,
        is_self: true,
        is_live: Boolean(selfLiveItem?.playing),
      });
    }

    return Array.from(map.values()).sort((a, b) => {
      if (Boolean(a.is_self) !== Boolean(b.is_self)) return a.is_self ? -1 : 1;
      if (Boolean(a.is_live) !== Boolean(b.is_live)) return a.is_live ? -1 : 1;
      const playDiff = (b.play_count ?? 0) - (a.play_count ?? 0);
      if (playDiff !== 0) return playDiff;
      return a.user_name.localeCompare(b.user_name);
    });
  }, [friends, live.snapshot, accountProfile, selfLiveItem]);

  // Build avatarMap keyed by BOTH user_uri and user_name.toLowerCase()
  const avatarMap = useMemo<AvatarMap>(() => {
    const map: AvatarMap = {};
    const register = (
      userUri: string,
      name: string,
      imageUrl: string | null | undefined,
      isSelf?: boolean,
    ) => {
      const existing = map[userUri];
      const entry = {
        user_uri: userUri,
        name: name || existing?.name || userUri,
        imageUrl: imageUrl ?? existing?.imageUrl ?? null,
        isSelf: Boolean(isSelf || existing?.isSelf),
      };
      map[userUri] = entry;
      if (entry.name) {
        map[entry.name.toLowerCase()] = entry;
      }
    };

    for (const f of friends) {
      register(f.user_uri, f.user_name, f.image_url, f.is_self);
    }
    if (live.snapshot) {
      for (const lf of live.snapshot.friends) {
        register(lf.user_uri, lf.user_name, lf.user_image_url, lf.is_self);
      }
    }
    if (accountProfile) {
      register(
        accountProfile.user_uri,
        accountProfile.display_name || accountProfile.username,
        accountProfile.image_url,
        true,
      );
    }
    return map;
  }, [friends, live.snapshot, accountProfile]);

  const selectedFriendObjects = useMemo(() => {
    const byUri = new Map(friendOptions.map((f) => [f.user_uri, f]));
    return selectedUsers.map((uri) => {
      const found = byUri.get(uri);
      return (
        found ?? {
          user_uri: uri,
          user_name: avatarMap[uri]?.name ?? uri.split(':').pop() ?? uri,
          image_url: lookupAvatar(avatarMap, uri),
          is_self: false,
          is_live: false,
        }
      );
    });
  }, [selectedUsers, friendOptions, avatarMap]);

  const liveSpinningCount = live.snapshot
    ? live.snapshot.friends.filter((f) => f.playing).length
    : friends.filter((f) => f.is_live).length;

  const liveConnected = live.snapshot?.connected ?? false;

  return (
    <TooltipProvider delayDuration={200}>
      <div className="flex min-h-screen flex-col bg-[var(--bg-canvas)] bg-pixel-grid text-[var(--ink-primary)]">
        {/* Sticky Top Header */}
        <header className="sticky top-0 z-30 border-b border-[var(--border-subtle)] bg-[var(--bg-canvas)]/95 backdrop-blur-md">
          <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-6">
            {/* Left: Brand Wordmark + Live Status Pill */}
            <div className="flex items-center gap-3.5">
              <button
                type="button"
                onClick={() => setActiveTab('overview')}
                className="flex items-baseline gap-0.5 font-display text-3xl font-extrabold leading-none tracking-tight text-[var(--ink-primary)] transition-opacity hover:opacity-90 cursor-pointer"
              >
                <span>bside</span>
                <span
                  className="inline-block h-[7px] w-[7px] rounded-[var(--radius-pill)] bg-[var(--accent-primary)]"
                  aria-hidden="true"
                />
              </button>

              <div className="hidden h-5 w-px bg-[var(--border-subtle)] sm:block" />

              <button
                type="button"
                onClick={() => setActiveTab('live')}
                className="inline-flex items-center gap-2 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2.5 py-1 font-mono text-[10px] font-semibold uppercase tracking-widest transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--bg-hover)] cursor-pointer"
                title="Open Live Now deck"
              >
                <PixelDot
                  active={liveSpinningCount > 0}
                  tone={
                    liveSpinningCount > 0
                      ? 'sage'
                      : liveConnected
                        ? 'muted'
                        : 'amber'
                  }
                />
                {liveSpinningCount > 0 ? (
                  <span className="text-[var(--accent-primary)]">
                    LIVE &middot; {liveSpinningCount} ON AIR
                  </span>
                ) : liveConnected ? (
                  <span className="text-[var(--ink-muted)]">LIVE &middot; IDLE</span>
                ) : (
                  <span className="text-[var(--ink-muted)]">
                    {live.snapshot?.status ? live.snapshot.status.toUpperCase() : 'ARCHIVE'}
                  </span>
                )}
              </button>
            </div>

            {/* Right: Filters, Controls, Theme Toggle & Own Account Badge */}
            <div className="flex flex-wrap items-center gap-2">
              {/* Multi-Friend Selector */}
              <FriendMultiSelect
                friends={friendOptions}
                selectedUsers={selectedUsers}
                onChange={setSelectedUsers}
              />

              {/* Timeframe Segmented Control */}
              <InfoTooltip
                content="Scopes stats, history, playlists, overlap, and profile changes to this time window."
                side="bottom"
              >
                <div className="inline-flex items-center overflow-hidden rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)]">
                  {TIMEFRAMES.map((tf) => (
                    <button
                      key={tf.key}
                      type="button"
                      onClick={() => setTimeframe(tf.key)}
                      className={cn(
                        'border-r border-[var(--border-subtle)] px-2.5 py-1.5 font-mono text-[10px] font-semibold uppercase tracking-wider transition-colors last:border-r-0 cursor-pointer',
                        timeframe === tf.key
                          ? 'bg-[var(--accent-primary)] text-[var(--ink-on-accent)]'
                          : 'text-[var(--ink-muted)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
                      )}
                    >
                      {tf.label}
                    </button>
                  ))}
                </div>
              </InfoTooltip>

              {/* Auto-Refresh 30s Toggle */}
              <InfoTooltip
                content="Auto-refresh database analytics every 30 seconds. Live Now updates continuously via SSE stream."
                side="bottom"
              >
                <div className="flex h-8 items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] px-2.5">
                  <label
                    htmlFor="auto-refresh-toggle"
                    className="font-mono text-[10px] font-semibold uppercase tracking-wider text-[var(--ink-muted)] cursor-pointer select-none"
                  >
                    AUTO 30s
                  </label>
                  <Switch
                    id="auto-refresh-toggle"
                    checked={autoRefresh}
                    onCheckedChange={setAutoRefresh}
                  />
                </div>
              </InfoTooltip>

              {/* Refresh Button */}
              <InfoTooltip
                content="Poll Spotify now and refresh all charts & history."
                side="bottom"
              >
                <Button variant="secondary" size="md" onClick={triggerRefresh}>
                  <RefreshCw
                    className={cn(
                      'h-3 w-3',
                      (loadingFriends || live.polling) && 'animate-spin',
                    )}
                  />
                  Refresh
                </Button>
              </InfoTooltip>

              {/* Dual Theme Toggle */}
              <InfoTooltip
                content="Switch between Spotify Dark (layered) and Classic B-Side themes"
                side="bottom"
              >
                <Button
                  variant="secondary"
                  size="md"
                  onClick={toggleTheme}
                  aria-label="Toggle theme"
                >
                  {theme === 'spotify' ? (
                    <>
                      <Disc3 className="h-3.5 w-3.5 text-[var(--accent-primary)]" />
                      <span className="hidden sm:inline">Spotify</span>
                    </>
                  ) : (
                    <>
                      <Palette className="h-3.5 w-3.5 text-[var(--accent-primary)]" />
                      <span className="hidden sm:inline">Classic</span>
                    </>
                  )}
                </Button>
              </InfoTooltip>

              {/* Top-Right Connected Own Account Badge */}
              <AccountBadge
                profile={accountProfile}
                liveItem={selfLiveItem}
                isFilteredToSelf={isFilteredToSelf}
                onFilterSelf={handleFilterSelf}
              />
            </div>
          </div>
        </header>

        {/* Main Workspace */}
        <main className="mx-auto w-full max-w-7xl flex-1 px-4 py-6 sm:px-6">
          {/* Active Filter Summary Bar */}
          {selectedUsers.length > 0 && (
            <div className="mb-4 flex flex-wrap items-center justify-between gap-2 rounded-[var(--radius-control)] border border-[var(--border-accent)] bg-[var(--accent-soft)] px-3 py-2 animate-fade-up">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-[10px] font-semibold uppercase tracking-widest text-[var(--accent-primary)]">
                  Filtered to
                </span>
                {selectedFriendObjects.map((friend) => (
                  <span
                    key={friend.user_uri}
                    className="inline-flex items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] py-0.5 pr-1.5 pl-1 text-xs font-medium text-[var(--ink-primary)]"
                  >
                    <UserAvatar
                      name={friend.user_name}
                      imageUrl={friend.image_url}
                      size="xs"
                      isLive={friend.is_live}
                      isSelf={friend.is_self}
                    />
                    <span className="max-w-[140px] truncate">{friend.user_name}</span>
                    <button
                      type="button"
                      onClick={() =>
                        setSelectedUsers((prev) =>
                          prev.filter((u) => u !== friend.user_uri),
                        )
                      }
                      className="inline-flex h-4 w-4 items-center justify-center rounded-full text-[var(--ink-muted)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)] cursor-pointer"
                      aria-label={`Remove ${friend.user_name} from filter`}
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </span>
                ))}
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setSelectedUsers([])}
                className="h-6 px-2 text-[10px]"
              >
                <X className="h-3 w-3" />
                Reset filter
              </Button>
            </div>
          )}

          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <div className="overflow-x-auto pb-1">
              <TabsList>
                <TabsTrigger value="live">
                  <Radio className="h-3.5 w-3.5" />
                  <span>LIVE</span>
                  {liveSpinningCount > 0 && (
                    <span className="ml-0.5 rounded-[var(--radius-pill)] border border-current px-1.5 py-0 text-[9px] leading-tight">
                      {liveSpinningCount}
                    </span>
                  )}
                </TabsTrigger>
                <TabsTrigger value="overview">
                  <BarChart3 className="h-3.5 w-3.5" />
                  OVERVIEW
                </TabsTrigger>
                <TabsTrigger value="history">
                  <Clock className="h-3.5 w-3.5" />
                  HISTORY
                </TabsTrigger>
                <TabsTrigger value="playlists">
                  <ListMusic className="h-3.5 w-3.5" />
                  PLAYLISTS
                </TabsTrigger>
                <TabsTrigger value="overlap">
                  <GitCompare className="h-3.5 w-3.5" />
                  OVERLAP
                </TabsTrigger>
                <TabsTrigger value="profiles">
                  <Users className="h-3.5 w-3.5" />
                  PROFILES
                </TabsTrigger>
              </TabsList>
            </div>

            <TabsContent value="live">
              <LiveNowSection
                live={live}
                mode="full"
                selectedUsers={selectedUsers}
                avatarMap={avatarMap}
                onSelectPlaylist={handleSelectPlaylist}
                onFilterByUser={handleFilterByUser}
              />
            </TabsContent>

            <TabsContent value="overview">
              <OverviewTab
                selectedUsers={selectedUsers}
                days={days}
                refreshTick={refreshTick}
                avatarMap={avatarMap}
                onSelectPlaylist={handleSelectPlaylist}
                onFilterByUser={handleFilterByUser}
                liveStrip={
                  <LiveNowSection
                    live={live}
                    mode="compact"
                    selectedUsers={selectedUsers}
                    avatarMap={avatarMap}
                    onOpenFull={() => setActiveTab('live')}
                    onSelectPlaylist={handleSelectPlaylist}
                    onFilterByUser={handleFilterByUser}
                  />
                }
              />
            </TabsContent>

            <TabsContent value="history">
              <HistoryTab
                selectedUsers={selectedUsers}
                days={days}
                refreshTick={refreshTick}
                avatarMap={avatarMap}
                onSelectPlaylist={handleSelectPlaylist}
                onFilterByUser={handleFilterByUser}
              />
            </TabsContent>

            <TabsContent value="playlists">
              <PlaylistsTab
                selectedUsers={selectedUsers}
                days={days}
                refreshTick={refreshTick}
                liveSnapshot={live.snapshot}
                avatarMap={avatarMap}
                focusedContextUri={focusedContextUri}
                onClearFocusedContext={() => setFocusedContextUri(null)}
                onFilterByUser={handleFilterByUser}
              />
            </TabsContent>

            <TabsContent value="overlap">
              <OverlapTab
                selectedUsers={selectedUsers}
                days={days}
                refreshTick={refreshTick}
                avatarMap={avatarMap}
                onFilterByUser={handleFilterByUser}
              />
            </TabsContent>

            <TabsContent value="profiles">
              <ProfilesTab
                selectedUsers={selectedUsers}
                days={days}
                refreshTick={refreshTick}
                avatarMap={avatarMap}
                onSelectPlaylist={handleSelectPlaylist}
                onFilterByUser={handleFilterByUser}
              />
            </TabsContent>
          </Tabs>
        </main>

        {/* Footer */}
        <footer className="mt-10 border-t border-[var(--border-subtle)] bg-[var(--bg-inset)]">
          <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-4 py-4 sm:px-6">
            <div className="flex items-baseline gap-0.5 font-display text-xl font-extrabold tracking-tight text-[var(--ink-primary)]">
              <span>bside</span>
              <span
                className="inline-block h-[5px] w-[5px] rounded-[var(--radius-pill)] bg-[var(--accent-primary)]"
                aria-hidden="true"
              />
            </div>
            <a
              href="https://github.com/002love/bside"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 font-mono text-[11px] font-medium text-[var(--ink-muted)] transition-colors hover:text-[var(--ink-primary)]"
            >
              <svg
                viewBox="0 0 24 24"
                fill="currentColor"
                className="h-3.5 w-3.5"
                aria-hidden="true"
              >
                <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z" />
              </svg>
              <span>002love/bside</span>
            </a>
          </div>
        </footer>
      </div>
    </TooltipProvider>
  );
}

export default App;
