import { useCallback, useEffect, useState } from 'react';
import {
  BarChart3,
  Clock,
  GitCompare,
  ListMusic,
  Radio,
  RefreshCw,
  Users,
} from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { TooltipProvider } from '@/components/ui/tooltip';
import { PixelDot } from '@/components/PixelEq';
import { LiveNowSection } from '@/components/LiveNowSection';
import { OverviewTab } from '@/components/tabs/OverviewTab';
import { HistoryTab } from '@/components/tabs/HistoryTab';
import { PlaylistsTab } from '@/components/tabs/PlaylistsTab';
import { OverlapTab } from '@/components/tabs/OverlapTab';
import { ProfilesTab } from '@/components/tabs/ProfilesTab';
import { useLiveFeed } from '@/hooks/useLiveFeed';
import { cn } from '@/lib/utils';
import type { FriendSummary, OverviewResponse, TimeframeKey } from '@/types/api';

const TIMEFRAMES: { key: TimeframeKey; label: string }[] = [
  { key: '7', label: '7D' },
  { key: '30', label: '30D' },
  { key: '90', label: '90D' },
  { key: 'all', label: 'ALL' },
];

const ALL_FRIENDS_VALUE = '__all__';

export function App() {
  const [activeTab, setActiveTab] = useState('overview');
  const [selectedFriend, setSelectedFriend] = useState<string>(ALL_FRIENDS_VALUE);
  const [timeframe, setTimeframe] = useState<TimeframeKey>('30');
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [refreshTick, setRefreshTick] = useState(0);

  const [friends, setFriends] = useState<FriendSummary[]>([]);
  const [overview, setOverview] = useState<OverviewResponse | null>(null);
  const [loadingOverview, setLoadingOverview] = useState(true);

  const live = useLiveFeed();

  const friendFilter = selectedFriend === ALL_FRIENDS_VALUE ? '' : selectedFriend;

  const triggerRefresh = useCallback(() => {
    setRefreshTick((t) => t + 1);
    void live.pollNow();
  }, [live.pollNow]);

  const handleSelectLiveFriend = useCallback((userUri: string) => {
    setSelectedFriend(userUri);
    setActiveTab('overview');
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
    setLoadingOverview(true);
    const params = new URLSearchParams();
    if (friendFilter) params.set('friend', friendFilter);
    if (timeframe !== 'all') params.set('days', timeframe);

    fetch(`/api/overview?${params.toString()}`)
      .then((res) => res.json())
      .then((data: OverviewResponse) => {
        if (!active) return;
        setOverview(data);
        if (Array.isArray(data.friends_summary)) {
          setFriends(data.friends_summary);
        }
      })
      .catch(() => {
        if (active) setOverview(null);
      })
      .finally(() => {
        if (active) setLoadingOverview(false);
      });

    return () => {
      active = false;
    };
  }, [friendFilter, timeframe, refreshTick]);

  const liveSpinningCount = live.snapshot
    ? live.snapshot.friends.filter((f) => f.playing).length
    : friends.filter((f) => f.is_live).length;

  const liveConnected = live.snapshot?.connected ?? false;

  // Merge live friends into selector if they aren't in SQLite overview yet
  const selectorFriends = (() => {
    const map = new Map<string, { user_uri: string; user_name: string; is_live: boolean }>();
    for (const f of friends) {
      map.set(f.user_uri, {
        user_uri: f.user_uri,
        user_name: f.user_name,
        is_live: f.is_live,
      });
    }
    if (live.snapshot) {
      for (const lf of live.snapshot.friends) {
        const existing = map.get(lf.user_uri);
        map.set(lf.user_uri, {
          user_uri: lf.user_uri,
          user_name: existing?.user_name || lf.user_name,
          is_live: lf.playing,
        });
      }
    }
    return Array.from(map.values());
  })();

  return (
    <TooltipProvider delayDuration={150}>
      <div className="flex min-h-screen flex-col bg-[#111110] bg-pixel-grid text-[#efeee8]">
        {/* Crisp Editorial Top Header */}
        <header className="sticky top-0 z-30 border-b border-[#2b2b27] bg-[#111110]/95 backdrop-blur-sm">
          <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-6">
            {/* Brand Wordmark + Live Status */}
            <div className="flex items-center gap-4">
              <button
                type="button"
                onClick={() => setActiveTab('overview')}
                className="flex items-baseline gap-0.5 font-display text-3xl font-extrabold leading-none tracking-tight text-[#efeee8] cursor-pointer"
              >
                <span>bside</span>
                <span className="inline-block h-[7px] w-[7px] bg-[#89b471]" aria-hidden="true" />
              </button>

              <div className="hidden h-5 w-px bg-[#2b2b27] sm:block" />

              <button
                type="button"
                onClick={() => setActiveTab('live')}
                className="inline-flex items-center gap-2 border border-[#2b2b27] bg-[#141412] px-2.5 py-1 font-mono text-[10px] font-semibold uppercase tracking-widest transition-colors hover:border-[#45453e] cursor-pointer"
                title="Open Realtime On Air Deck"
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
                  <span className="text-[#89b471]">LIVE &middot; {liveSpinningCount} SPINNING</span>
                ) : liveConnected ? (
                  <span className="text-[#8d8c84]">LIVE &middot; IDLE</span>
                ) : (
                  <span className="text-[#8d8c84]">
                    {live.snapshot?.status ? live.snapshot.status.toUpperCase() : 'ARCHIVE'}
                  </span>
                )}
              </button>
            </div>

            {/* Global Controls */}
            <div className="flex flex-wrap items-center gap-2">
              {/* Friend Filter */}
              <div className="flex items-center gap-1.5">
                <span className="hidden font-mono text-[10px] font-semibold uppercase tracking-widest text-[#8d8c84] md:inline">
                  FRIEND
                </span>
                <Select value={selectedFriend} onValueChange={setSelectedFriend}>
                  <SelectTrigger className="w-44">
                    <SelectValue placeholder="ALL FRIENDS" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={ALL_FRIENDS_VALUE}>ALL FRIENDS</SelectItem>
                    {selectorFriends.map((f) => (
                      <SelectItem key={f.user_uri} value={f.user_uri}>
                        <span className="flex items-center gap-2">
                          <span
                            className={cn(
                              'h-1.5 w-1.5 shrink-0',
                              f.is_live ? 'bg-[#89b471]' : 'bg-[#45453e]',
                            )}
                          />
                          <span className="truncate">{f.user_name}</span>
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Timeframe Segmented Control */}
              <div className="inline-flex items-center border border-[#2b2b27] bg-[#141412]">
                {TIMEFRAMES.map((tf) => (
                  <button
                    key={tf.key}
                    type="button"
                    onClick={() => setTimeframe(tf.key)}
                    className={cn(
                      'border-r border-[#2b2b27] last:border-r-0 px-2.5 py-1.5 font-mono text-[10px] font-semibold uppercase tracking-wider transition-colors cursor-pointer',
                      timeframe === tf.key
                        ? 'bg-[#efeee8] text-[#111110]'
                        : 'text-[#8d8c84] hover:bg-[#20201d] hover:text-[#efeee8]',
                    )}
                  >
                    {tf.label}
                  </button>
                ))}
              </div>

              {/* Auto-Refresh Toggle */}
              <div className="flex h-8 items-center gap-2 border border-[#2b2b27] bg-[#141412] px-2.5">
                <label
                  htmlFor="auto-refresh-toggle"
                  className="font-mono text-[10px] font-semibold uppercase tracking-wider text-[#8d8c84] cursor-pointer"
                >
                  AUTO
                </label>
                <Switch
                  id="auto-refresh-toggle"
                  checked={autoRefresh}
                  onCheckedChange={setAutoRefresh}
                />
              </div>

              {/* Outlined Refresh Button */}
              <Button
                variant="outline"
                size="md"
                onClick={triggerRefresh}
                title="Refresh analytics and poll live activity"
              >
                <RefreshCw
                  className={cn(
                    'h-3 w-3',
                    (loadingOverview || live.polling) && 'animate-spin',
                  )}
                />
                REFRESH
              </Button>
            </div>
          </div>
        </header>

        {/* Main Workspace */}
        <main className="mx-auto w-full max-w-7xl flex-1 px-4 py-6 sm:px-6">
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <div className="overflow-x-auto pb-1">
              <TabsList>
                <TabsTrigger value="live">
                  <Radio className="h-3.5 w-3.5" />
                  <span>LIVE</span>
                  {liveSpinningCount > 0 && (
                    <span className="ml-0.5 border border-current px-1 py-0 text-[9px] leading-tight">
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
                snapshot={live.snapshot}
                loading={live.loading}
                polling={live.polling}
                now={live.now}
                onPollNow={live.pollNow}
                onSelectFriend={handleSelectLiveFriend}
                variant="full"
              />
            </TabsContent>

            <TabsContent value="overview">
              <OverviewTab
                data={overview}
                loading={loadingOverview}
                liveStrip={
                  <LiveNowSection
                    snapshot={live.snapshot}
                    loading={live.loading}
                    polling={live.polling}
                    now={live.now}
                    onPollNow={live.pollNow}
                    onSelectFriend={handleSelectLiveFriend}
                    variant="compact"
                    onOpenLiveTab={() => setActiveTab('live')}
                  />
                }
              />
            </TabsContent>

            <TabsContent value="history">
              <HistoryTab
                friend={friendFilter}
                timeframe={timeframe}
                refreshTick={refreshTick}
              />
            </TabsContent>

            <TabsContent value="playlists">
              <PlaylistsTab friend={friendFilter} refreshTick={refreshTick} />
            </TabsContent>

            <TabsContent value="overlap">
              <OverlapTab timeframe={timeframe} refreshTick={refreshTick} />
            </TabsContent>

            <TabsContent value="profiles">
              <ProfilesTab
                friend={friendFilter}
                timeframe={timeframe}
                refreshTick={refreshTick}
              />
            </TabsContent>
          </Tabs>
        </main>

        {/* Editorial Footer */}
        <footer className="mt-10 border-t border-[#2b2b27] bg-[#141412]">
          <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-4 py-4 sm:px-6">
            <div className="flex items-baseline gap-0.5 font-display text-xl font-extrabold tracking-tight text-[#efeee8]">
              <span>bside</span>
              <span className="inline-block h-[5px] w-[5px] bg-[#89b471]" aria-hidden="true" />
            </div>
            <p className="font-mono text-[10px] font-medium uppercase tracking-widest text-[#8d8c84]">
              LOCAL-FIRST SPOTIFY ANALYTICS &middot; SQLITE + LIBRESPOT
            </p>
          </div>
        </footer>
      </div>
    </TooltipProvider>
  );
}

export default App;
