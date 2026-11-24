import { useCallback, useEffect, useRef, useState } from 'react';
import type { LiveSnapshot } from '@/types/api';

const FALLBACK_POLL_MS = 15_000;

export interface UseLiveFeedResult {
  snapshot: LiveSnapshot | null;
  loading: boolean;
  polling: boolean;
  now: number;
  pollNow: () => Promise<void>;
}

function isLiveSnapshot(value: unknown): value is LiveSnapshot {
  return (
    typeof value === 'object' &&
    value !== null &&
    'friends' in value &&
    Array.isArray((value as LiveSnapshot).friends) &&
    'status' in value
  );
}

export function useLiveFeed(): UseLiveFeedResult {
  const [snapshot, setSnapshot] = useState<LiveSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [polling, setPolling] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const fallbackTimerRef = useRef<number | null>(null);

  const fetchSnapshot = useCallback(async () => {
    try {
      const res = await fetch('/api/live');
      if (!res.ok) return;
      const data: unknown = await res.json();
      if (isLiveSnapshot(data)) {
        setSnapshot(data);
        setNow(Date.now());
      }
    } catch {
      // Keep previous snapshot or null if live endpoint is unreachable
    } finally {
      setLoading(false);
    }
  }, []);

  const pollNow = useCallback(async () => {
    setPolling(true);
    try {
      const res = await fetch('/api/live/refresh', { method: 'POST' });
      if (res.ok) {
        const text = await res.text();
        if (text) {
          try {
            const parsed: unknown = JSON.parse(text);
            if (isLiveSnapshot(parsed)) {
              setSnapshot(parsed);
              setNow(Date.now());
              return;
            }
          } catch {
            // Non-JSON acknowledgement; fall through to GET /api/live
          }
        }
      }
      await fetchSnapshot();
    } catch {
      await fetchSnapshot();
    } finally {
      setPolling(false);
    }
  }, [fetchSnapshot]);

  useEffect(() => {
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
      }, FALLBACK_POLL_MS);
    };

    // Initial hydration
    void fetchSnapshot();

    const handleEventData = (raw: string) => {
      if (!active || !raw) return;
      try {
        const parsed: unknown = JSON.parse(raw);
        if (isLiveSnapshot(parsed)) {
          setSnapshot(parsed);
          setLoading(false);
          setNow(Date.now());
        }
      } catch {
        // Ignore malformed SSE keepalive/comments
      }
    };

    if (typeof EventSource !== 'undefined') {
      try {
        source = new EventSource('/api/live/stream');

        source.onopen = () => {
          clearFallback();
        };

        source.onmessage = (event) => {
          handleEventData(event.data);
        };

        source.addEventListener('snapshot', (event) => {
          handleEventData((event as MessageEvent<string>).data);
        });

        source.addEventListener('live', (event) => {
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
  }, [fetchSnapshot]);

  // Lightweight 1s ticker ONLY while at least one friend is actively playing
  const hasPlaying = Boolean(snapshot?.friends.some((f) => f.playing));

  useEffect(() => {
    if (!hasPlaying) return;
    setNow(Date.now());
    const id = window.setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => window.clearInterval(id);
  }, [hasPlaying]);

  return { snapshot, loading, polling, now, pollNow };
}
