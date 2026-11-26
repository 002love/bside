import type {
  DeviceCommandPayload,
  PlayCommandPayload,
  PlayerCommandResponse,
  PlayerDevicesResponse,
  PlayerSnapshot,
  QueueCommandPayload,
  RepeatCommandPayload,
  RepeatMode,
  SeekCommandPayload,
  ShuffleCommandPayload,
  TransferCommandPayload,
  VolumeCommandPayload,
} from '@/types/player';

export const PLAYER_STREAM_URL = '/api/player/stream';

export interface PlaybackApiErrorInit {
  status: number;
  code: string;
  details?: unknown;
}

export class PlaybackApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details?: unknown;

  constructor(message: string, init: PlaybackApiErrorInit) {
    super(message);
    this.name = 'PlaybackApiError';
    this.status = init.status;
    this.code = init.code;
    this.details = init.details;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

function defaultErrorCodeForStatus(status: number): string {
  if (status === 400 || status === 422) return 'invalid_input';
  if (status === 401 || status === 403) return 'unauthenticated';
  if (status === 503) return 'offline';
  if (status === 0) return 'network_error';
  return 'command_failed';
}

export function toPlaybackApiError(err: unknown): PlaybackApiError {
  if (err instanceof PlaybackApiError) {
    return err;
  }
  if (err instanceof Error) {
    return new PlaybackApiError(err.message || 'Network request failed', {
      status: 0,
      code: 'network_error',
      details: err,
    });
  }
  return new PlaybackApiError('Unexpected playback error', {
    status: 0,
    code: 'unknown',
    details: err,
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export async function requestPlaybackJson<T>(
  url: string,
  init?: RequestInit,
): Promise<T> {
  let response: Response;
  try {
    response = await fetch(url, {
      ...init,
      headers: {
        Accept: 'application/json',
        ...(init?.body !== undefined ? { 'Content-Type': 'application/json' } : {}),
        ...init?.headers,
      },
    });
  } catch (err) {
    throw toPlaybackApiError(err);
  }

  const rawText = await response.text().catch(() => '');
  let parsed: unknown = undefined;
  if (rawText) {
    try {
      parsed = JSON.parse(rawText);
    } catch {
      parsed = undefined;
    }
  }

  if (!response.ok) {
    let code = defaultErrorCodeForStatus(response.status);
    let message = `Request failed with HTTP ${response.status}`;
    let details: unknown = undefined;

    if (isRecord(parsed)) {
      if (typeof parsed.code === 'string' && parsed.code.trim()) {
        code = parsed.code;
      }
      if (typeof parsed.error === 'string' && parsed.error.trim()) {
        message = parsed.error;
      } else if (typeof parsed.message === 'string' && parsed.message.trim()) {
        message = parsed.message;
      }
      details = 'details' in parsed ? parsed.details : parsed;
    } else if (rawText.trim()) {
      message = rawText.trim();
    }

    throw new PlaybackApiError(message, {
      status: response.status,
      code,
      details,
    });
  }

  if (parsed === undefined) {
    throw new PlaybackApiError('Expected JSON response from server', {
      status: response.status,
      code: 'unknown',
      details: rawText,
    });
  }

  return parsed as T;
}

function postJson<T>(url: string, body?: unknown): Promise<T> {
  return requestPlaybackJson<T>(url, {
    method: 'POST',
    body: JSON.stringify(body ?? {}),
  });
}

export function isPlayerSnapshot(value: unknown): value is PlayerSnapshot {
  if (!isRecord(value)) return false;
  return (
    typeof value.connected === 'boolean' &&
    typeof value.updated_at_ms === 'number' &&
    Array.isArray(value.devices) &&
    typeof value.is_playing === 'boolean' &&
    typeof value.is_paused === 'boolean' &&
    typeof value.is_buffering === 'boolean' &&
    typeof value.position_ms === 'number' &&
    typeof value.duration_ms === 'number' &&
    typeof value.shuffle === 'boolean' &&
    (value.repeat_mode === 'off' ||
      value.repeat_mode === 'context' ||
      value.repeat_mode === 'track') &&
    Array.isArray(value.queue_next) &&
    isRecord(value.restrictions)
  );
}

export function computeInterpolatedPositionMs(
  snapshot: PlayerSnapshot | null,
  nowMs: number = Date.now(),
): number {
  if (!snapshot) return 0;

  const base = Number.isFinite(snapshot.position_ms)
    ? Math.max(0, snapshot.position_ms)
    : 0;
  const maxDuration =
    Number.isFinite(snapshot.duration_ms) && snapshot.duration_ms > 0
      ? snapshot.duration_ms
      : snapshot.track &&
          Number.isFinite(snapshot.track.duration_ms) &&
          snapshot.track.duration_ms > 0
        ? snapshot.track.duration_ms
        : 0;

  let estimated = base;
  if (snapshot.is_playing && !snapshot.is_paused && !snapshot.is_buffering) {
    const anchor =
      snapshot.position_timestamp_ms > 0
        ? snapshot.position_timestamp_ms
        : snapshot.updated_at_ms;
    const speed =
      Number.isFinite(snapshot.playback_speed) && snapshot.playback_speed > 0
        ? snapshot.playback_speed
        : 1;
    if (anchor > 0) {
      const elapsed = Math.max(0, nowMs - anchor);
      estimated = base + elapsed * speed;
    }
  }

  const rounded = Math.max(0, Math.round(estimated));
  return maxDuration > 0 ? Math.min(rounded, maxDuration) : rounded;
}

export function getNextRepeatMode(current: RepeatMode): RepeatMode {
  switch (current) {
    case 'off':
      return 'context';
    case 'context':
      return 'track';
    case 'track':
      return 'off';
  }
}

export function getPlayerState(): Promise<PlayerSnapshot> {
  return requestPlaybackJson<PlayerSnapshot>('/api/player', {
    method: 'GET',
  });
}

export function getPlayerDevices(): Promise<PlayerDevicesResponse> {
  return requestPlaybackJson<PlayerDevicesResponse>('/api/player/devices', {
    method: 'GET',
  });
}

export function play(
  payload?: PlayCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/play', payload);
}

export function pause(
  payload?: DeviceCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/pause', payload);
}

export function resume(
  payload?: DeviceCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/resume', payload);
}

export function togglePlayback(
  payload?: DeviceCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/toggle', payload);
}

export function skipNext(
  payload?: DeviceCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/next', payload);
}

export function skipPrevious(
  payload?: DeviceCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/previous', payload);
}

export function seekTo(
  payload: SeekCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/seek', payload);
}

export function setVolume(
  payload: VolumeCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/volume', payload);
}

export function setShuffle(
  payload: ShuffleCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/shuffle', payload);
}

export function setRepeat(
  payload: RepeatCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/repeat', payload);
}

export function addToQueue(
  payload: QueueCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/queue', payload);
}

export function transferPlayback(
  payload: TransferCommandPayload,
): Promise<PlayerCommandResponse> {
  return postJson<PlayerCommandResponse>('/api/player/transfer', payload);
}
