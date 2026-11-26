export type RepeatMode = 'off' | 'context' | 'track';

export type PlayerErrorCode =
  | 'no_active_device'
  | 'unauthenticated'
  | 'offline'
  | 'invalid_input'
  | 'command_failed'
  | 'network_error'
  | 'unknown';

export interface PlayerNamedUri {
  uri: string;
  name: string;
}

export interface PlayerDevice {
  id: string;
  name: string;
  device_type: string;
  is_active: boolean;
  is_private_session: boolean;
  is_restricted: boolean;
  is_offline: boolean;
  volume_percent: number;
  volume_raw: number;
  supports_volume: boolean;
  volume_steps: number;
}

export interface PlayerTrack {
  uri: string;
  name: string;
  artists: PlayerNamedUri[];
  all_artists: string;
  album: PlayerNamedUri | null;
  cover_url: string | null;
  duration_ms: number;
  item_type: 'track' | 'episode' | string;
  spotify_url: string;
}

export interface PlayerContext {
  uri: string;
  name: string | null;
  context_type: string;
  image_url: string | null;
}

export interface PlayerQueueItem {
  uri: string;
  name: string;
  artist_name: string;
  album_name: string | null;
  cover_url: string | null;
  duration_ms: number | null;
  item_type: string;
}

export interface PlayerRestrictions {
  can_pause: boolean;
  can_resume: boolean;
  can_seek: boolean;
  can_skip_next: boolean;
  can_skip_prev: boolean;
  can_toggle_shuffle: boolean;
  can_toggle_repeat_context: boolean;
  can_toggle_repeat_track: boolean;
  can_add_to_queue: boolean;
  can_transfer: boolean;
}

export interface PlayerSnapshot {
  connected: boolean;
  updated_at_ms: number;
  active_device_id: string | null;
  devices: PlayerDevice[];
  is_playing: boolean;
  is_paused: boolean;
  is_buffering: boolean;
  playback_speed: number;
  position_ms: number;
  position_timestamp_ms: number;
  duration_ms: number;
  shuffle: boolean;
  repeat_mode: RepeatMode;
  track: PlayerTrack | null;
  context: PlayerContext | null;
  queue_next: PlayerQueueItem[];
  restrictions: PlayerRestrictions;
}

export interface PlayerDevicesResponse {
  active_device_id: string | null;
  devices: PlayerDevice[];
}

export interface PlayCommandPayload {
  track_uri?: string;
  context_uri?: string;
  position_ms?: number;
  skip_to_uri?: string;
  skip_to_index?: number;
  initially_paused?: boolean;
  device_id?: string;
}

export interface DeviceCommandPayload {
  device_id?: string;
}

export interface SeekCommandPayload {
  position_ms: number;
  device_id?: string;
}

export interface VolumeCommandPayload {
  volume_percent: number;
  device_id?: string;
}

export interface ShuffleCommandPayload {
  enabled: boolean;
  device_id?: string;
}

export interface RepeatCommandPayload {
  mode: RepeatMode;
  device_id?: string;
}

export interface QueueCommandPayload {
  track_uri: string;
  device_id?: string;
}

export interface TransferCommandPayload {
  device_id: string;
  play?: boolean;
}

export interface PlayerCommandResponse {
  ok: boolean;
  device_id: string;
  player: PlayerSnapshot;
  action?: 'paused' | 'resumed';
  position_ms?: number;
  volume_percent?: number;
  enabled?: boolean;
  mode?: RepeatMode;
  track_uri?: string;
}
