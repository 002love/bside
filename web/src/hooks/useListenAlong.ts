import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import {
  LISTEN_ALONG_STREAM_URL,
  computeEstimatedHostPositionMs,
  getListenAlongState,
  isListenAlongState,
  normalizeSpotifyUserId,
  resyncListenAlong,
  startListenAlong,
  stopListenAlong,
  updateListenAlongSettings,
} from '@/lib/listenAlongApi';
import { PlaybackApiError, toPlaybackApiError } from '@/lib/playerApi';
import type {
  ListenAlongSettings,
  ListenAlongState,
  ListenAlongStatus,
  ListenAlongTarget,
  ResyncListenAlongPayload,
  StartListenAlongPayload,
  StopListenAlongPayload,
  UpdateListenAlongSettingsPayload,
} from '@/types/listenAlong';

const DEFAULT_TICK_MS = 500;
const DEFAULT_FALLBACK_POLL_MS = 10_000;

const DEFAULT_SETTINGS: ListenAlongSettings = {
  device_id: null,
  sync_pause: true,
  auto_resync: true,
  drift_threshold_ms: 4_000,
  latency_offset_ms: 0,
  auto_stop_on_manual_change: false,
};

export interface UseListenAlongOptions {
  tickMs?: number;
  fallbackPollMs?: number;
  enabled?: boolean;
}

export type StartListenAlongOptions = Omit<StartListenAlongPayload, 'user_uri'>;

export interface UseListenAlongResult {
  state: ListenAlongState | null;
  loading: boolean;
  active: boolean;
  status: ListenAlongStatus;
  target: ListenAlongTarget | null;
  settings: ListenAlongSettings;
  resolvedDeviceId: string | null;
  driftMs: number | null;
  diverged: boolean;
  syncCount: number;
  estimatedHostPositionMs: number | null;
  hostProgress: number | null;
  pendingAction: string | null;
  error: PlaybackApiError | null;
  isFollowing: (userUri: string) => boolean;
  refresh: () => Promise<ListenAlongState | null>;
  start: (
    userUri: string,
    options?: StartListenAlongOptions,
  ) => Promise<ListenAlongState | null>;
  stop: (options?: StopListenAlongPayload) => Promise<ListenAlongState | null>;
  toggle: (
    userUri: string,
    options?: StartListenAlongOptions,
  ) => Promise<ListenAlongState | null>;
  resync: (
    options?: ResyncListenAlongPayload,
  ) => Promise<ListenAlongState | null>;
  updateSettings: (
    patch: UpdateListenAlongSettingsPayload,
  ) => Promise<ListenAlongState | null>;
  clearError: () => void;
}

export function useListenAlong(
  options: UseListenAlongOptions = {},
): UseListenAlongResult {
  const tickMs = options.tickMs ?? DEFAULT_TICK_MS;
  const fallbackPollMs = options.fallbackPollMs ?? DEFAULT_FALLBACK_POLL_MS;
  const enabled = options.enabled ?? true;

  const [state, setState] = useState<ListenAlongState | null>(null);
  const [loading, setLoading] = useState<boolean>(enabled);
  const [now, setNow] = useState<number>(() => Date.now());
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [error, setError] = useState<PlaybackApiError | null>(null);

  const fallbackTimerRef = useRef<number | null>(null);
  const mountedRef = useRef(true);
  const stateRef = useRef<ListenAlongState | null>(null);
  const actionSeqRef = useRef(0);

  stateRef.current = state;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const applyState = useCallback((next: ListenAlongState) => {
    if (!mountedRef.current) return;
    stateRef.current = next;
    setState(next);
    setLoading(false);
    setNow(Date.now());
  }, []);

  const fetchState = useCallback(async (): Promise<ListenAlongState | null> => {
    try {
      const data = await getListenAlongState();
      if (isListenAlongState(data)) {
        applyState(data);
        return data;
      }
      return null;
    } catch {
      // Keep previous state when listen-along endpoint is temporarily unreachable
      return null;
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, [applyState]);

  useEffect(() => {
    if (!enabled) {
      setLoading(false);
      return;
    }

    let active = true;
    let source: EventSource | null = null;

    const clearFallback = () => {
      if (fallbackTimerRef.current != null) {
        window.clearInterval(fallbackTimerRef.current);
        fallbackTimerRef.current = null;
      }
    };

    const startFallback = () => {
      if (!active || fallbackTimerRef.current != null) return;
      fallbackTimerRef.current = window.setInterval(() => {
        if (active) void fetchState();
      }, fallbackPollMs);
    };

    void fetchState();

    const handleEventData = (raw: string) => {
      if (!active || !raw) return;
      try {
        const parsed: unknown = JSON.parse(raw);
        if (isListenAlongState(parsed)) {
          applyState(parsed);
        }
      } catch {
        // Ignore malformed SSE keepalive/comments
      }
    };

    if (typeof EventSource !== 'undefined') {
      try {
        source = new EventSource(LISTEN_ALONG_STREAM_URL);

        source.onopen = () => {
          clearFallback();
        };

        source.onmessage = (event) => {
          handleEventData(event.data);
        };

        source.addEventListener('listen_along', (event) => {
          handleEventData((event as MessageEvent<string>).data);
        });

        source.onerror = () => {
          if (!active) return;
          if (source && source.readyState === EventSource.CLOSED) {
            source.close();
            source = null;
          }
          startFallback();
        };
      } catch {
        startFallback();
      }
    } else {
      startFallback();
    }

    return () => {
      active = false;
      clearFallback();
      if (source) {
        source.close();
      }
    };
  }, [applyState, enabled, fallbackPollMs, fetchState]);

  // Lightweight 500ms ticker ONLY while actively following a playing host
  const shouldTick = Boolean(enabled && state?.active && state.target?.playing);

  useEffect(() => {
    if (!shouldTick) return;
    setNow(Date.now());
    const intervalMs = Math.max(100, tickMs);
    const id = window.setInterval(() => {
      setNow(Date.now());
    }, intervalMs);
    return () => window.clearInterval(id);
  }, [shouldTick, tickMs]);

  const runAction = useCallback(
    async (
      actionName: string,
      executor: () => Promise<ListenAlongState>,
    ): Promise<ListenAlongState | null> => {
      const seq = ++actionSeqRef.current;
      setPendingAction(actionName);
      setError(null);

      try {
        const next = await executor();
        if (isListenAlongState(next)) {
          applyState(next);
          return next;
        }
        return null;
      } catch (err) {
        const apiError = toPlaybackApiError(err);
        if (mountedRef.current) {
          setError(apiError);
        }
        return null;
      } finally {
        if (mountedRef.current && actionSeqRef.current === seq) {
          setPendingAction(null);
        }
      }
    },
    [applyState],
  );

  const isFollowing = useCallback((userUri: string): boolean => {
    const current = stateRef.current;
    if (!current?.active || !current.target?.user_uri) return false;
    const candidateId = normalizeSpotifyUserId(userUri);
    const targetId = normalizeSpotifyUserId(current.target.user_uri);
    return candidateId !== '' && candidateId === targetId;
  }, []);

  const refresh = useCallback(
    () => runAction('refresh', () => getListenAlongState()),
    [runAction],
  );

  const start = useCallback(
    (userUri: string, startOptions?: StartListenAlongOptions) =>
      runAction('start', () =>
        startListenAlong({
          user_uri: userUri,
          ...startOptions,
        }),
      ),
    [runAction],
  );

  const stop = useCallback(
    (stopOptions?: StopListenAlongPayload) =>
      runAction('stop', () => stopListenAlong(stopOptions)),
    [runAction],
  );

  const toggle = useCallback(
    (userUri: string, startOptions?: StartListenAlongOptions) => {
      if (isFollowing(userUri)) {
        return stop();
      }
      return start(userUri, startOptions);
    },
    [isFollowing, start, stop],
  );

  const resync = useCallback(
    (resyncOptions?: ResyncListenAlongPayload) =>
      runAction('resync', () => resyncListenAlong(resyncOptions)),
    [runAction],
  );

  const updateSettings = useCallback(
    (patch: UpdateListenAlongSettingsPayload) =>
      runAction('updateSettings', () => updateListenAlongSettings(patch)),
    [runAction],
  );

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  const target = state?.target ?? null;

  const estimatedHostPositionMs = useMemo(
    () => computeEstimatedHostPositionMs(target, now),
    [target, now],
  );

  const hostProgress = useMemo(() => {
    if (
      estimatedHostPositionMs == null ||
      !target?.duration_ms ||
      target.duration_ms <= 0
    ) {
      return null;
    }
    return Math.min(1, Math.max(0, estimatedHostPositionMs / target.duration_ms));
  }, [estimatedHostPositionMs, target]);

  return {
    state,
    loading,
    active: state?.active ?? false,
    status: state?.status ?? 'idle',
    target,
    settings: state?.settings ?? DEFAULT_SETTINGS,
    resolvedDeviceId: state?.resolved_device_id ?? null,
    driftMs: state?.drift_ms ?? null,
    diverged: state?.diverged ?? false,
    syncCount: state?.sync_count ?? 0,
    estimatedHostPositionMs,
    hostProgress,
    pendingAction,
    error,
    isFollowing,
    refresh,
    start,
    stop,
    toggle,
    resync,
    updateSettings,
    clearError,
  };
}

const ListenAlongReactContext = createContext<UseListenAlongResult | null>(null);

export interface ListenAlongProviderProps {
  children: ReactNode;
  options?: UseListenAlongOptions;
}

export function ListenAlongProvider({
  children,
  options,
}: ListenAlongProviderProps) {
  const value = useListenAlong(options);
  return createElement(ListenAlongReactContext.Provider, { value }, children);
}

export function useListenAlongContext(): UseListenAlongResult {
  const ctx = useContext(ListenAlongReactContext);
  if (!ctx) {
    throw new Error(
      'useListenAlongContext must be used within a ListenAlongProvider',
    );
  }
  return ctx;
}
