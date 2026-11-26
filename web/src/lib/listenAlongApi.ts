import { requestPlaybackJson } from '@/lib/playerApi';
import type {
  ListenAlongState,
  ListenAlongStatus,
  ListenAlongTarget,
  ResyncListenAlongPayload,
  StartListenAlongPayload,
  StopListenAlongPayload,
  UpdateListenAlongSettingsPayload,
} from '@/types/listenAlong';

export const LISTEN_ALONG_STREAM_URL = '/api/listen-along/stream';

const VALID_STATUSES: ReadonlySet<ListenAlongStatus> = new Set([
  'idle',
  'connecting',
  'waiting',
  'syncing',
  'synced',
  'host_paused',
  'diverged',
  'error',
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function isListenAlongState(value: unknown): value is ListenAlongState {
  if (!isRecord(value)) return false;
  return (
    typeof value.active === 'boolean' &&
    typeof value.status === 'string' &&
    VALID_STATUSES.has(value.status as ListenAlongStatus) &&
    isRecord(value.settings) &&
    typeof value.sync_count === 'number' &&
    typeof value.diverged === 'boolean' &&
    typeof value.updated_at_ms === 'number'
  );
}

export function normalizeSpotifyUserId(input: string): string {
  const trimmed = input.trim().toLowerCase();
  if (!trimmed) return '';
  if (trimmed.startsWith('spotify:user:')) {
    return trimmed.slice('spotify:user:'.length).trim();
  }
  const urlMatch = trimmed.match(/^https?:\/\/open\.spotify\.com\/user\/([^/?#]+)/);
  if (urlMatch?.[1]) {
    return decodeURIComponent(urlMatch[1]).trim().toLowerCase();
  }
  return trimmed;
}

export function computeEstimatedHostPositionMs(
  target: ListenAlongTarget | null,
  nowMs: number = Date.now(),
): number | null {
  if (!target || !target.playing || !target.track_uri) {
    return null;
  }
  if (!Number.isFinite(target.timestamp_ms) || target.timestamp_ms <= 0) {
    return null;
  }

  const elapsed = Math.max(0, nowMs - target.timestamp_ms);
  const rounded = Math.round(elapsed);
  if (
    target.duration_ms != null &&
    Number.isFinite(target.duration_ms) &&
    target.duration_ms > 0
  ) {
    return Math.min(rounded, target.duration_ms);
  }
  return rounded;
}

export function getListenAlongState(): Promise<ListenAlongState> {
  return requestPlaybackJson<ListenAlongState>('/api/listen-along', {
    method: 'GET',
  });
}

export function startListenAlong(
  payload: StartListenAlongPayload,
): Promise<ListenAlongState> {
  return requestPlaybackJson<ListenAlongState>('/api/listen-along/start', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function stopListenAlong(
  payload?: StopListenAlongPayload,
): Promise<ListenAlongState> {
  return requestPlaybackJson<ListenAlongState>('/api/listen-along/stop', {
    method: 'POST',
    body: JSON.stringify(payload ?? {}),
  });
}

export function resyncListenAlong(
  payload?: ResyncListenAlongPayload,
): Promise<ListenAlongState> {
  return requestPlaybackJson<ListenAlongState>('/api/listen-along/resync', {
    method: 'POST',
    body: JSON.stringify(payload ?? {}),
  });
}

export function updateListenAlongSettings(
  payload: UpdateListenAlongSettingsPayload,
): Promise<ListenAlongState> {
  return requestPlaybackJson<ListenAlongState>('/api/listen-along/settings', {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
}
