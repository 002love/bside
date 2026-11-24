import { useCallback, useEffect, useState } from 'react';
import {
  BarChart3,
  Clock,
  Disc3,
  GitCompare,
  ListMusic,
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
import { OverviewTab } from '@/components/tabs/OverviewTab';
import { HistoryTab } from '@/components/tabs/HistoryTab';
import { PlaylistsTab } from '@/components/tabs/PlaylistsTab';
import { OverlapTab } from '@/components/tabs/OverlapTab';
import { ProfilesTab } from '@/components/tabs/ProfilesTab';
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

  const friendFilter = selectedFriend === ALL_FRIENDS_VALUE ? '' : selectedFriend;

  const triggerRefresh = useCallback(() => {
    setRefreshTick((t) => t + 1);
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

  const liveCount = friends.filter((f) => f.is_live).length;

  return (
    <TooltipProvider delayDuration={150}>
      <div className="min-h-screen bg-zinc-950 text-zinc-100">
        {/* Top Bar */}
        <header className="sticky top-0 z-30 border-b border-zinc-800/80 bg-zinc-950/85 backdrop-blur-md">
          <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-6">
            {/* Wordmark */}
            <div className="flex items-center gap-3">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg border border-emerald-500/30 bg-emerald-500/10 text-emerald-400">
                <Disc3 className="h-4 w-4" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-sm font-semibold tracking-tight text-zinc-50">bside</span>
                  {liveCount > 0 && (
                    <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-300">
                      <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 animate-pulse" />
                      {liveCount} live
                    </span>
                  )}
                </div>
                <p className="text-[11px] text-zinc-500">Local Spotify Analytics</p>
              </div>
            </div>

            {/* Global Controls */}
            <div className="flex flex-wrap items-center gap-2.5">
              {/* Friend Filter */}
              <Select value={selectedFriend} onValueChange={setSelectedFriend}>
                <SelectTrigger className="w-44">
                  <SelectValue placeholder="All friends" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL_FRIENDS_VALUE}>All friends</SelectItem>
                  {friends.map((f) => (
                    <SelectItem key={f.user_uri} value={f.user_uri}>
                      <span className="flex items-center gap-2">
                        <span
                          className={cn(
                            'h-1.5 w-1.5 rounded-full',
                            f.is_live ? 'bg-emerald-400' : 'bg-zinc-600',
                          )}
                        />
                        <span>{f.user_name}</span>
                      </span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              {/* Timeframe Segmented Control */}
              <div className="inline-flex items-center rounded-lg border border-zinc-800 bg-zinc-900 p-0.5">
                {TIMEFRAMES.map((tf) => (
                  <button
                    key={tf.key}
                    type="button"
                    onClick={() => setTimeframe(tf.key)}
                    className={cn(
                      'rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors cursor-pointer',
                      timeframe === tf.key
                        ? 'bg-zinc-800 text-emerald-400 shadow-sm'
                        : 'text-zinc-400 hover:text-zinc-200',
                    )}
                  >
                    {tf.label}
                  </button>
                ))}
              </div>

              {/* Refresh & Auto-Refresh */}
              <Button
                variant="secondary"
                size="sm"
                onClick={triggerRefresh}
                title="Refresh data"
              >
                <RefreshCw className={cn('h-3.5 w-3.5', loadingOverview && 'animate-spin')} />
                Refresh
              </Button>

              <div className="flex items-center gap-1.5 rounded-lg border border-zinc-800/80 bg-zinc-900/70 px-2.5 py-1">
                <label
                  htmlFor="auto-refresh-toggle"
                  className="text-[11px] font-medium text-zinc-400 cursor-pointer"
                >
                  Auto
                </label>
                <Switch
                  id="auto-refresh-toggle"
                  checked={autoRefresh}
                  onCheckedChange={setAutoRefresh}
                />
              </div>
            </div>
          </div>
        </header>

        {/* Main Content */}
        <main className="mx-auto max-w-7xl px-4 py-6 sm:px-6">
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <div className="overflow-x-auto pb-1">
              <TabsList>
                <TabsTrigger value="overview">
                  <BarChart3 className="h-3.5 w-3.5" />
                  Overview
                </TabsTrigger>
                <TabsTrigger value="history">
                  <Clock className="h-3.5 w-3.5" />
                  History
                </TabsTrigger>
                <TabsTrigger value="playlists">
                  <ListMusic className="h-3.5 w-3.5" />
                  Playlists
                </TabsTrigger>
                <TabsTrigger value="overlap">
                  <GitCompare className="h-3.5 w-3.5" />
                  Overlap
                </TabsTrigger>
                <TabsTrigger value="profiles">
                  <Users className="h-3.5 w-3.5" />
                  Profiles & Changes
                </TabsTrigger>
              </TabsList>
            </div>

            <TabsContent value="overview">
              <OverviewTab data={overview} loading={loadingOverview} />
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
      </div>
    </TooltipProvider>
  );
}

export default App;
