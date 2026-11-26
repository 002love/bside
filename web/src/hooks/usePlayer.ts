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
  PLAYER_STREAM_URL,
  PlaybackApiError,
  addToQueue as apiAddToQueue,
  computeInterpolatedPositionMs,
  getNextRepeatMode,
  getPlayerState,
  isPlayerSnapshot,
  pause as apiPause,
  play as apiPlay,
  resume as apiResume,
  seekTo as apiSeekTo,
  setRepeat as apiSetRepeat,
  setShuffle as apiSetShuffle,
  setVolume as apiSetVolume,
  skipNext as apiSkipNext,
  skipPrevious as apiSkipPrevious,
  toPlaybackApiError,
  togglePlayback as apiTogglePlayback,
  transferPlayback as apiTransferPlayback,
} from '@/lib/playerApi';
import type {
  PlayCommandPayload,
  PlayerCommandResponse,
  PlayerContext,
  PlayerDevice,
  PlayerQueueItem,
  PlayerRestrictions,
  PlayerSnapshot,
  PlayerTrack,
  RepeatMode,
} from '@/types/player';

const DEFAULT_TICK_MS = 250;
const DEFAULT_FALLBACK_POLL_MS = 10_000;

const EMPTY_DEVICES: PlayerDevice[] = [];
const EMPTY_QUEUE: PlayerQueueItem[] = [];

const DEFAULT_RESTRICTIONS: PlayerRestrictions = {
  can_pause: false,
  can_resume: false,
  can_seek: false,
  can_skip_next: false,
  can_skip_prev: false,
  can_toggle_shuffle: false,
  can_toggle_repeat_context: false,
  can_toggle_repeat_track: false,
  can_add_to_queue: false,
  can_transfer: false,
};

export interface UsePlayerOptions {
  tickMs?: number;
  fallbackPollMs?: number;
  enabled?: boolean;
}

export interface UsePlayerResult {
  snapshot: PlayerSnapshot | null;
  loading: boolean;
  connected: boolean;
  isPlaying: boolean;
  isPaused: boolean;
  isBuffering: boolean;
  track: PlayerTrack | null;
  context: PlayerContext | null;
  devices: PlayerDevice[];
  activeDevice: PlayerDevice | null;
  queueNext: PlayerQueueItem[];
  restrictions: PlayerRestrictions;
  shuffle: boolean;
  repeatMode: RepeatMode;
  volumePercent: number | null;
  positionMs: number;
  durationMs: number;
  progress: number;
  pendingAction: string | null;
  error: PlaybackApiError | null;
  refresh: () => Promise<PlayerSnapshot | null>;
  play: (payload?: PlayCommandPayload) => Promise<PlayerCommandResponse | null>;
  pause: (deviceId?: string) => Promise<PlayerCommandResponse | null>;
  resume: (deviceId?: string) => Promise<PlayerCommandResponse | null>;
  togglePlay: (deviceId?: string) => Promise<PlayerCommandResponse | null>;
  next: (deviceId?: string) => Promise<PlayerCommandResponse | null>;
  previous: (deviceId?: string) => Promise<PlayerCommandResponse | null>;
  seek: (
    positionMs: number,
    deviceId?: string,
  ) => Promise<PlayerCommandResponse | null>;
  setVolume: (
    volumePercent: number,
    deviceId?: string,
  ) => Promise<PlayerCommandResponse | null>;
  setShuffle: (
    enabled: boolean,
    deviceId?: string,
  ) => Promise<PlayerCommandResponse | null>;
  toggleShuffle: (deviceId?: string) => Promise<PlayerCommandResponse | null>;
  setRepeat: (
    mode: RepeatMode,
    deviceId?: string,
  ) => Promise<PlayerCommandResponse | null>;
  cycleRepeat: (deviceId?: string) => Promise<PlayerCommandResponse | null>;
  addToQueue: (
    trackUri: string,
    deviceId?: string,
  ) => Promise<PlayerCommandResponse | null>;
  transferDevice: (
    deviceId: string,
    play?: boolean,
  ) => Promise<PlayerCommandResponse | null>;
  clearError: () => void;
}

export function usePlayer(options: UsePlayerOptions = {}): UsePlayerResult {
  const tickMs = options.tickMs ?? DEFAULT_TICK_MS;
  const fallbackPollMs = options.fallbackPollMs ?? DEFAULT_FALLBACK_POLL_MS;
  const enabled = options.enabled ?? true;

  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [loading, setLoading] = useState<boolean>(enabled);
  const [now, setNow] = useState<number>(() => Date.now());
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [error, setError] = useState<PlaybackApiError | null>(null);

  const fallbackTimerRef = useRef<number | null>(null);
  const mountedRef = useRef(true);
  const snapshotRef = useRef<PlayerSnapshot | null>(null);
  const actionSeqRef = useRef(0);

  snapshotRef.current = snapshot;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const applySnapshot = useCallback((next: PlayerSnapshot) => {
    if (!mountedRef.current) return;
    snapshotRef.current = next;
    setSnapshot(next);
    setLoading(false);
    setNow(Date.now());
  }, []);

  const fetchSnapshot = useCallback(async (): Promise<PlayerSnapshot | null> => {
    try {
      const data = await getPlayerState();
      if (isPlayerSnapshot(data)) {
        applySnapshot(data);
        return data;
      }
      return null;
    } catch {
      // Keep previous snapshot when player endpoint is temporarily unreachable
      return null;
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, [applySnapshot]);

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
        if (active) void fetchSnapshot();
      }, fallbackPollMs);
    };

    void fetchSnapshot();

    const handleEventData = (raw: string) => {
      if (!active || !raw) return;
      try {
        const parsed: unknown = JSON.parse(raw);
        if (isPlayerSnapshot(parsed)) {
          applySnapshot(parsed);
        }
      } catch {
        // Ignore malformed SSE keepalive/comments
      }
    };

    if (typeof EventSource !== 'undefined') {
      try {
        source = new EventSource(PLAYER_STREAM_URL);

        source.onopen = () => {
          clearFallback();
        };

        source.onmessage = (event) => {
          handleEventData(event.data);
        };

        source.addEventListener('player', (event) => {
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
  }, [applySnapshot, enabled, fallbackPollMs, fetchSnapshot]);

  // Smooth progress ticker ONLY while actively playing (never while paused, buffering, or idle)
  const shouldTick = Boolean(
    enabled &&
      snapshot?.is_playing &&
      !snapshot.is_paused &&
      !snapshot.is_buffering,
  );

  useEffect(() => {
    if (!shouldTick) return;
    setNow(Date.now());
    const intervalMs = Math.max(50, tickMs);
    const id = window.setInterval(() => {
      setNow(Date.now());
    }, intervalMs);
    return () => window.clearInterval(id);
  }, [shouldTick, tickMs]);

  const runCommand = useCallback(
    async (
      actionName: string,
      executor: () => Promise<PlayerCommandResponse>,
    ): Promise<PlayerCommandResponse | null> => {
      const seq = ++actionSeqRef.current;
      setPendingAction(actionName);
      setError(null);

      try {
        const res = await executor();
        if (isPlayerSnapshot(res.player)) {
          applySnapshot(res.player);
        }
        return res;
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
    [applySnapshot],
  );

  const refresh = useCallback(async (): Promise<PlayerSnapshot | null> => {
    const seq = ++actionSeqRef.current;
    setPendingAction('refresh');
    setError(null);
    try {
      const data = await getPlayerState();
      if (isPlayerSnapshot(data)) {
        applySnapshot(data);
        return data;
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
  }, [applySnapshot]);

  const play = useCallback(
    (payload?: PlayCommandPayload) =>
      runCommand('play', () => apiPlay(payload)),
    [runCommand],
  );

  const pause = useCallback(
    (deviceId?: string) =>
      runCommand('pause', () =>
        apiPause(deviceId ? { device_id: deviceId } : undefined),
      ),
    [runCommand],
  );

  const resume = useCallback(
    (deviceId?: string) =>
      runCommand('resume', () =>
        apiResume(deviceId ? { device_id: deviceId } : undefined),
      ),
    [runCommand],
  );

  const togglePlay = useCallback(
    (deviceId?: string) =>
      runCommand('togglePlay', () =>
        apiTogglePlayback(deviceId ? { device_id: deviceId } : undefined),
      ),
    [runCommand],
  );

  const next = useCallback(
    (deviceId?: string) =>
      runCommand('next', () =>
        apiSkipNext(deviceId ? { device_id: deviceId } : undefined),
      ),
    [runCommand],
  );

  const previous = useCallback(
    (deviceId?: string) =>
      runCommand('previous', () =>
        apiSkipPrevious(deviceId ? { device_id: deviceId } : undefined),
      ),
    [runCommand],
  );

  const seek = useCallback(
    (positionMs: number, deviceId?: string) =>
      runCommand('seek', () =>
        apiSeekTo({
          position_ms: Math.max(0, Math.round(positionMs)),
          ...(deviceId ? { device_id: deviceId } : {}),
        }),
      ),
    [runCommand],
  );

  const setVolume = useCallback(
    (volumePercent: number, deviceId?: string) =>
      runCommand('setVolume', () =>
        apiSetVolume({
          volume_percent: Math.min(100, Math.max(0, Math.round(volumePercent))),
          ...(deviceId ? { device_id: deviceId } : {}),
        }),
      ),
    [runCommand],
  );

  const setShuffle = useCallback(
    (nextEnabled: boolean, deviceId?: string) =>
      runCommand('setShuffle', () =>
        apiSetShuffle({
          enabled: nextEnabled,
          ...(deviceId ? { device_id: deviceId } : {}),
        }),
      ),
    [runCommand],
  );

  const toggleShuffle = useCallback(
    (deviceId?: string) => {
      const current = snapshotRef.current?.shuffle ?? false;
      return setShuffle(!current, deviceId);
    },
    [setShuffle],
  );

  const setRepeat = useCallback(
    (mode: RepeatMode, deviceId?: string) =>
      runCommand('setRepeat', () =>
        apiSetRepeat({
          mode,
          ...(deviceId ? { device_id: deviceId } : {}),
        }),
      ),
    [runCommand],
  );

  const cycleRepeat = useCallback(
    (deviceId?: string) => {
      const current = snapshotRef.current?.repeat_mode ?? 'off';
      return setRepeat(getNextRepeatMode(current), deviceId);
    },
    [setRepeat],
  );

  const addToQueue = useCallback(
    (trackUri: string, deviceId?: string) =>
      runCommand('addToQueue', () =>
        apiAddToQueue({
          track_uri: trackUri,
          ...(deviceId ? { device_id: deviceId } : {}),
        }),
      ),
    [runCommand],
  );

  const transferDevice = useCallback(
    (deviceId: string, playOnTransfer?: boolean) =>
      runCommand('transferDevice', () =>
        apiTransferPlayback({
          device_id: deviceId,
          ...(playOnTransfer !== undefined ? { play: playOnTransfer } : {}),
        }),
      ),
    [runCommand],
  );

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  const devices = snapshot?.devices ?? EMPTY_DEVICES;
  const activeDevice = useMemo(() => {
    if (!snapshot) return null;
    if (snapshot.active_device_id) {
      const matched = snapshot.devices.find(
        (device) => device.id === snapshot.active_device_id,
      );
      if (matched) return matched;
    }
    return snapshot.devices.find((device) => device.is_active) ?? null;
  }, [snapshot]);

  const positionMs = useMemo(
    () => computeInterpolatedPositionMs(snapshot, now),
    [snapshot, now],
  );

  const durationMs =
    snapshot && snapshot.duration_ms > 0
      ? snapshot.duration_ms
      : snapshot?.track?.duration_ms ?? 0;

  const progress =
    durationMs > 0 ? Math.min(1, Math.max(0, positionMs / durationMs)) : 0;

  return {
    snapshot,
    loading,
    connected: snapshot?.connected ?? false,
    isPlaying: snapshot?.is_playing ?? false,
    isPaused: snapshot?.is_paused ?? false,
    isBuffering: snapshot?.is_buffering ?? false,
    track: snapshot?.track ?? null,
    context: snapshot?.context ?? null,
    devices,
    activeDevice,
    queueNext: snapshot?.queue_next ?? EMPTY_QUEUE,
    restrictions: snapshot?.restrictions ?? DEFAULT_RESTRICTIONS,
    shuffle: snapshot?.shuffle ?? false,
    repeatMode: snapshot?.repeat_mode ?? 'off',
    volumePercent: activeDevice ? activeDevice.volume_percent : null,
    positionMs,
    durationMs,
    progress,
    pendingAction,
    error,
    refresh,
    play,
    pause,
    resume,
    togglePlay,
    next,
    previous,
    seek,
    setVolume,
    setShuffle,
    toggleShuffle,
    setRepeat,
    cycleRepeat,
    addToQueue,
    transferDevice,
    clearError,
  };
}

const PlayerReactContext = createContext<UsePlayerResult | null>(null);

export interface PlayerProviderProps {
  children: ReactNode;
  options?: UsePlayerOptions;
}

export function PlayerProvider({ children, options }: PlayerProviderProps) {
  const value = usePlayer(options);
  return createElement(PlayerReactContext.Provider, { value }, children);
}

export function usePlayerContext(): UsePlayerResult {
  const ctx = useContext(PlayerReactContext);
  if (!ctx) {
    throw new Error('usePlayerContext must be used within a PlayerProvider');
  }
  return ctx;
}
