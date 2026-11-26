import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  BarChart3,
  Clock,
  GitCompare,
  Headphones,
  ListMusic,
  Menu,
  Radio,
  RefreshCw,
  Timer,
  Users,
  X,
} from 'lucide-react';
import { AnimatePresence, MotionConfig, motion } from 'motion/react';
import { Button } from '@/components/ui/button';
import { TooltipProvider } from '@/components/ui/tooltip';
import { EASE_OUT_EXPO, PageTransition, SPRING_SNAPPY } from '@/components/ui/motion';
import { AccountBadge } from '@/components/AccountBadge';
import { FriendMultiSelect, type FriendOption } from '@/components/FriendMultiSelect';
import { InfoTooltip } from '@/components/InfoTooltip';
import { LiveNowSection } from '@/components/LiveNowSection';
import { PixelEq } from '@/components/PixelEq';
import { UserAvatar, lookupAvatar } from '@/components/UserAvatar';
import { OverviewTab } from '@/components/tabs/OverviewTab';
import { HistoryTab } from '@/components/tabs/HistoryTab';
import { PlaylistsTab } from '@/components/tabs/PlaylistsTab';
import { OverlapTab } from '@/components/tabs/OverlapTab';
import { ProfilesTab } from '@/components/tabs/ProfilesTab';
import { useLiveFeed } from '@/hooks/useLiveFeed';
import { useCoverAccent } from '@/lib/extractColor';
import { cn } from '@/lib/utils';
import type {
  AccountProfile,
  AvatarMap,
  FriendSummary,
  LiveFriendItem,
  TimeframeKey,
} from '@/types/api';

type NavKey = 'live' | 'overview' | 'history' | 'playlists' | 'overlap' | 'profiles';

const NAV_ITEMS: { key: NavKey; label: string; icon: typeof Radio }[] = [
  { key: 'live', label: 'Live', icon: Radio },
  { key: 'overview', label: 'Overview', icon: BarChart3 },
  { key: 'history', label: 'History', icon: Clock },
  { key: 'playlists', label: 'Playlists', icon: ListMusic },
  { key: 'overlap', label: 'Overlap', icon: GitCompare },
  { key: 'profiles', label: 'Friends', icon: Users },
];

const TIMEFRAMES: { key: TimeframeKey; label: string }[] = [
  { key: '7', label: '7d' },
  { key: '30', label: '30d' },
  { key: '90', label: '90d' },
  { key: 'all', label: 'All' },
];

function SidebarLiveRow({
  friend,
  avatarUrl,
  isSelected,
  onClick,
}: {
  friend: LiveFriendItem;
  avatarUrl: string | null;
  isSelected: boolean;
  onClick: () => void;
}) {
  const palette = useCoverAccent(friend.cover_url, friend.track_uri || friend.track_name);

  return (
    <button
      type="button"
      onClick={onClick}
      title={`${friend.user_name} · ${friend.track_name}`}
      className={cn(
        'tactile-press group flex w-full items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-left transition-colors cursor-pointer',
        isSelected ? 'bg-white/[0.11] text-white' : 'hover:bg-white/[0.055]',
      )}
    >
      <UserAvatar
        name={friend.user_name}
        imageUrl={avatarUrl}
        size="sm"
        isLive
        isSelf={friend.is_self}
        ringColor={palette.accent}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-1.5">
          <span className="truncate text-xs font-medium text-white">
            {friend.user_name}
          </span>
          <PixelEq active color={palette.accent} className="h-2.5 shrink-0" />
        </div>
        <div className="truncate text-[11px] text-[var(--ink-muted)] group-hover:text-[var(--ink-secondary)]">
          {friend.track_name}
        </div>
      </div>
    </button>
  );
}

export function App() {
  const [activeTab, setActiveTab] = useState<NavKey>('live');
  const [selectedUsers, setSelectedUsers] = useState<string[]>([]);
  const [timeframe, setTimeframe] = useState<TimeframeKey>('30');
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [refreshTick, setRefreshTick] = useState(0);
  const [focusedContextUri, setFocusedContextUri] = useState<string | null>(null);
  const [mobileNavOpen, setMobileNavOpen] = useState(false);

  const [friends, setFriends] = useState<FriendSummary[]>([]);
  const [loadingFriends, setLoadingFriends] = useState(false);

  const live = useLiveFeed();
  const days: number | null = timeframe === 'all' ? null : Number(timeframe);

  const triggerRefresh = useCallback(() => {
    setRefreshTick((t) => t + 1);
    void live.pollNow();
  }, [live.pollNow]);

  const handleSelectPlaylist = useCallback((contextUri: string) => {
    setFocusedContextUri(contextUri);
    setActiveTab('playlists');
    setMobileNavOpen(false);
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

  const liveFriends = useMemo(
    () => (live.snapshot?.friends ?? []).filter((f) => f.playing),
    [live.snapshot],
  );

  const liveSpinningCount = live.snapshot
    ? liveFriends.length
    : friends.filter((f) => f.is_live).length;

  const primaryLiveAccent = useCoverAccent(
    liveFriends[0]?.cover_url,
    liveFriends[0]?.track_uri || liveFriends[0]?.track_name,
  );

  const connectionSubtitle = useMemo(() => {
    if (!live.snapshot) return 'Archive';
    if (live.snapshot.connected && live.snapshot.status === 'live') {
      if (live.snapshot.transport === 'dealer_push') return 'Realtime';
      if (live.snapshot.transport === 'mercury_push') return 'Mercury';
      if (live.snapshot.transport === 'http_polling') return 'Polling';
      return 'Connected';
    }
    return live.snapshot.status === 'unauthenticated' ? 'Sign in' : 'Offline';
  }, [live.snapshot]);

  const sidebarContent = (
    <div className="flex h-full flex-col bg-[var(--bg-rail)]">
      {/* Brand Header */}
      <div className="flex h-[54px] shrink-0 items-center justify-between px-4">
        <button
          type="button"
          onClick={() => {
            setActiveTab('live');
            setMobileNavOpen(false);
          }}
          className="flex items-center gap-2 text-left transition-opacity hover:opacity-85 cursor-pointer"
        >
          <span className="font-display text-[18px] font-bold tracking-[-0.035em] text-white">
            bside
          </span>
          <span
            className="h-1.5 w-1.5 rounded-full transition-colors duration-500"
            style={{
              backgroundColor:
                liveSpinningCount > 0 ? primaryLiveAccent.accent : 'var(--accent-ember)',
            }}
            aria-hidden="true"
          />
        </button>

        <span className="text-[11px] font-medium text-[var(--ink-muted)]">
          {connectionSubtitle}
        </span>
      </div>

      {/* Primary Navigation */}
      <nav aria-label="Primary" className="px-2.5 pt-1">
        <div className="space-y-0.5">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = activeTab === item.key;
            return (
              <button
                key={item.key}
                type="button"
                onClick={() => {
                  setActiveTab(item.key);
                  setMobileNavOpen(false);
                }}
                className={cn(
                  'relative flex w-full items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-[13px] font-medium transition-colors cursor-pointer select-none',
                  isActive
                    ? 'text-white'
                    : 'text-[var(--ink-secondary)] hover:bg-white/[0.05] hover:text-white',
                )}
              >
                {isActive && (
                  <motion.span
                    layoutId="sidebar-nav-pill"
                    transition={SPRING_SNAPPY}
                    className="absolute inset-0 rounded-[8px] bg-white/[0.11]"
                  />
                )}
                <Icon
                  className={cn(
                    'relative z-10 h-4 w-4 shrink-0 transition-colors',
                    isActive ? 'text-white' : 'text-[var(--ink-muted)]',
                  )}
                />
                <span className="relative z-10 flex-1 text-left">{item.label}</span>
                {item.key === 'live' && liveSpinningCount > 0 && (
                  <span
                    className="relative z-10 tabular-nums text-[11px] font-semibold"
                    style={{ color: primaryLiveAccent.accent }}
                  >
                    {liveSpinningCount}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </nav>

      {/* Active Circle / Listen Along Rail */}
      <div className="mt-6 flex min-h-0 flex-1 flex-col px-2.5">
        <div className="mb-2 flex items-center justify-between px-2.5">
          <span className="text-[11px] font-medium text-[var(--ink-muted)]">
            Active circle
          </span>
          {liveFriends.length > 0 && (
            <span className="tabular-nums text-[11px] text-[var(--ink-muted)]">
              {liveFriends.length}
            </span>
          )}
        </div>

        <div className="min-h-0 flex-1 space-y-0.5 overflow-y-auto pr-0.5">
          {liveFriends.length === 0 ? (
            <div className="px-2.5 py-3 text-[11.5px] leading-relaxed text-[var(--ink-muted)]">
              Quiet right now. Active listeners appear here.
            </div>
          ) : (
            liveFriends.map((friend) => {
              const isSelected =
                selectedUsers.length === 1 && selectedUsers[0] === friend.user_uri;
              return (
                <SidebarLiveRow
                  key={friend.user_uri}
                  friend={friend}
                  avatarUrl={
                    friend.user_image_url ??
                    lookupAvatar(avatarMap, friend.user_uri, friend.user_name)
                  }
                  isSelected={isSelected}
                  onClick={() => {
                    handleFilterByUser(friend.user_uri);
                    setActiveTab('live');
                    setMobileNavOpen(false);
                  }}
                />
              );
            })
          )}
        </div>

        <div className="mt-2 px-2.5 pb-2">
          <div className="flex items-center gap-2 rounded-[8px] border border-white/[0.06] bg-white/[0.02] px-2.5 py-2 text-[11px] text-[var(--ink-muted)]">
            <Headphones className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">Listen along & player soon</span>
          </div>
        </div>
      </div>

      {/* Account Footer */}
      <div className="shrink-0 border-t border-white/[0.07] p-2.5">
        <AccountBadge
          profile={accountProfile}
          liveItem={selfLiveItem}
          isFilteredToSelf={isFilteredToSelf}
          onFilterSelf={handleFilterSelf}
          layout="sidebar"
        />
      </div>
    </div>
  );

  // Playlists uses an internal full-height split scroll; other tabs scroll smoothly as one full-bleed stage
  const isSplitViewport = activeTab === 'playlists';

  return (
    <MotionConfig reducedMotion="user">
      <TooltipProvider delayDuration={180}>
        <div className="flex h-screen w-full overflow-hidden bg-[var(--bg-canvas)] text-[var(--ink-primary)]">
          {/* Desktop Pitch-Black Left Rail */}
          <aside className="hidden w-[236px] shrink-0 border-r border-white/[0.07] lg:block">
            {sidebarContent}
          </aside>

          {/* Mobile Drawer */}
          <AnimatePresence>
            {mobileNavOpen && (
              <>
                <motion.div
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.16 }}
                  onClick={() => setMobileNavOpen(false)}
                  className="fixed inset-0 z-40 bg-black/70 backdrop-blur-xs lg:hidden"
                />
                <motion.aside
                  initial={{ x: -252 }}
                  animate={{ x: 0 }}
                  exit={{ x: -252 }}
                  transition={{ duration: 0.22, ease: EASE_OUT_EXPO }}
                  className="fixed inset-y-0 left-0 z-50 w-[252px] border-r border-white/[0.08] lg:hidden"
                >
                  {sidebarContent}
                </motion.aside>
              </>
            )}
          </AnimatePresence>

          {/* Full-Bleed Right Workspace */}
          <div className="relative flex min-w-0 flex-1 flex-col overflow-hidden bg-[var(--bg-canvas)]">
            {/* Single Unified Translucent Utility Bar (ZERO duplicate page title) */}
            <header className="z-30 flex h-[52px] shrink-0 items-center justify-between gap-3 border-b border-white/[0.06] bg-[#09090b]/80 px-4 backdrop-blur-xl sm:px-6 lg:px-8">
              <div className="flex min-w-0 flex-1 items-center gap-2">
                <button
                  type="button"
                  onClick={() => setMobileNavOpen(true)}
                  className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] text-[var(--ink-secondary)] hover:bg-white/[0.06] hover:text-white lg:hidden cursor-pointer"
                  aria-label="Open navigation"
                >
                  <Menu className="h-4 w-4" />
                </button>

                {/* Active friend filters live inline in the top utility bar — zero extra banner row */}
                <AnimatePresence initial={false}>
                  {selectedUsers.length > 0 && (
                    <motion.div
                      initial={{ opacity: 0, x: -6 }}
                      animate={{ opacity: 1, x: 0 }}
                      exit={{ opacity: 0, x: -6 }}
                      transition={{ duration: 0.15 }}
                      className="flex min-w-0 items-center gap-1.5 overflow-x-auto"
                    >
                      {selectedFriendObjects.map((friend) => (
                        <span
                          key={friend.user_uri}
                          className="inline-flex shrink-0 items-center gap-1.5 rounded-full bg-white/[0.09] py-0.5 pr-1.5 pl-1 text-xs font-medium text-white"
                        >
                          <UserAvatar
                            name={friend.user_name}
                            imageUrl={friend.image_url}
                            size="xs"
                            isLive={friend.is_live}
                            isSelf={friend.is_self}
                          />
                          <span className="max-w-[120px] truncate">{friend.user_name}</span>
                          <button
                            type="button"
                            onClick={() =>
                              setSelectedUsers((prev) =>
                                prev.filter((u) => u !== friend.user_uri),
                              )
                            }
                            className="inline-flex h-4 w-4 items-center justify-center rounded-full text-white/60 transition-colors hover:bg-white/10 hover:text-white cursor-pointer"
                            aria-label={`Remove ${friend.user_name} from filter`}
                          >
                            <X className="h-3 w-3" />
                          </button>
                        </span>
                      ))}
                      {selectedUsers.length > 1 && (
                        <button
                          type="button"
                          onClick={() => setSelectedUsers([])}
                          className="shrink-0 px-1.5 text-[11px] font-medium text-[var(--ink-muted)] hover:text-white cursor-pointer"
                        >
                          Clear
                        </button>
                      )}
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>

              {/* Global Controls */}
              <div className="flex shrink-0 items-center gap-2">
                <FriendMultiSelect
                  friends={friendOptions}
                  selectedUsers={selectedUsers}
                  onChange={setSelectedUsers}
                />

                <InfoTooltip
                  content="Scopes analytics, history, playlists, overlap, and profile changes."
                  side="bottom"
                >
                  <div
                    role="group"
                    aria-label="Timeframe"
                    className="inline-flex h-8 items-center rounded-[8px] bg-white/[0.05] p-0.5 ring-1 ring-inset ring-white/[0.07]"
                  >
                    {TIMEFRAMES.map((tf) => {
                      const active = timeframe === tf.key;
                      return (
                        <button
                          key={tf.key}
                          type="button"
                          onClick={() => setTimeframe(tf.key)}
                          className={cn(
                            'relative h-full rounded-[6px] px-2.5 text-[11.5px] font-medium transition-colors cursor-pointer select-none',
                            active
                              ? 'text-white'
                              : 'text-[var(--ink-muted)] hover:text-[var(--ink-secondary)]',
                          )}
                        >
                          {active && (
                            <motion.span
                              layoutId="timeframe-pill"
                              transition={SPRING_SNAPPY}
                              className="absolute inset-0 rounded-[6px] bg-white/[0.13]"
                            />
                          )}
                          <span className="relative z-10">{tf.label}</span>
                        </button>
                      );
                    })}
                  </div>
                </InfoTooltip>

                <InfoTooltip
                  content={
                    autoRefresh
                      ? 'Auto-refreshing every 30s (click to pause)'
                      : 'Enable 30s auto-refresh'
                  }
                  side="bottom"
                >
                  <Button
                    variant={autoRefresh ? 'accent' : 'ghost'}
                    size="icon"
                    onClick={() => setAutoRefresh((v) => !v)}
                    aria-label="Toggle 30 second auto-refresh"
                    aria-pressed={autoRefresh}
                  >
                    <Timer className="h-3.5 w-3.5" />
                  </Button>
                </InfoTooltip>

                <InfoTooltip content="Sync now and refresh data" side="bottom">
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={triggerRefresh}
                    aria-label="Refresh"
                  >
                    <RefreshCw
                      className={cn(
                        'h-3.5 w-3.5',
                        (loadingFriends || live.polling) && 'animate-spin',
                      )}
                    />
                  </Button>
                </InfoTooltip>
              </div>
            </header>

            {/* Full-Bleed View Stage (NO centered max-w webpage container!) */}
            <main
              className={cn(
                'min-h-0 flex-1 w-full',
                isSplitViewport ? 'overflow-hidden' : 'overflow-y-auto',
              )}
            >
              <AnimatePresence mode="wait">
                {activeTab === 'live' && (
                  <PageTransition key="live" className="w-full">
                    <LiveNowSection
                      live={live}
                      mode="full"
                      selectedUsers={selectedUsers}
                      avatarMap={avatarMap}
                      onSelectPlaylist={handleSelectPlaylist}
                      onFilterByUser={handleFilterByUser}
                    />
                  </PageTransition>
                )}

                {activeTab === 'overview' && (
                  <PageTransition key="overview" className="w-full">
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
                  </PageTransition>
                )}

                {activeTab === 'history' && (
                  <PageTransition key="history" className="w-full">
                    <HistoryTab
                      selectedUsers={selectedUsers}
                      days={days}
                      refreshTick={refreshTick}
                      avatarMap={avatarMap}
                      onSelectPlaylist={handleSelectPlaylist}
                      onFilterByUser={handleFilterByUser}
                    />
                  </PageTransition>
                )}

                {activeTab === 'playlists' && (
                  <PageTransition key="playlists" className="h-full w-full">
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
                  </PageTransition>
                )}

                {activeTab === 'overlap' && (
                  <PageTransition key="overlap" className="w-full">
                    <OverlapTab
                      selectedUsers={selectedUsers}
                      days={days}
                      refreshTick={refreshTick}
                      avatarMap={avatarMap}
                      onFilterByUser={handleFilterByUser}
                    />
                  </PageTransition>
                )}

                {activeTab === 'profiles' && (
                  <PageTransition key="profiles" className="w-full">
                    <ProfilesTab
                      selectedUsers={selectedUsers}
                      days={days}
                      refreshTick={refreshTick}
                      avatarMap={avatarMap}
                      onSelectPlaylist={handleSelectPlaylist}
                      onFilterByUser={handleFilterByUser}
                    />
                  </PageTransition>
                )}
              </AnimatePresence>
            </main>
          </div>
        </div>
      </TooltipProvider>
    </MotionConfig>
  );
}

export default App;
