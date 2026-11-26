export type ListenAlongStatus =
  | 'idle'
  | 'connecting'
  | 'waiting'
  | 'syncing'
  | 'synced'
  | 'host_paused'
  | 'diverged'
  | 'error';

export type ListenAlongErrorCode =
  | 'invalid_input'
  | 'self_target'
  | 'not_active'
  | 'unauthenticated'
  | 'offline'
  | 'no_active_device'
  | 'command_failed'
  | 'network_error'
  | 'unknown';

export interface ListenAlongTarget {
  user_uri: string;
  user_name: string;
  user_image_url: string | null;
  playing: boolean;
  timestamp_ms: number;
  track_uri: string | null;
  track_name: string | null;
  all_artists: string | null;
  album_name: string | null;
  cover_url: string | null;
  duration_ms: number | null;
  context_uri: string | null;
  context_name: string | null;
  context_type: string | null;
  context_is_private: boolean;
  spotify_url: string | null;
}

export interface ListenAlongSettings {
  device_id: string | null;
  sync_pause: boolean;
  auto_resync: boolean;
  drift_threshold_ms: number;
  latency_offset_ms: number;
  auto_stop_on_manual_change: boolean;
}

export interface ListenAlongState {
  active: boolean;
  status: ListenAlongStatus;
  target: ListenAlongTarget | null;
  settings: ListenAlongSettings;
  resolved_device_id: string | null;
  started_at_ms: number | null;
  last_synced_at_ms: number | null;
  last_synced_track_uri: string | null;
  last_synced_position_ms: number | null;
  sync_count: number;
  drift_ms: number | null;
  diverged: boolean;
  last_error: string | null;
  last_error_code: ListenAlongErrorCode | string | null;
  updated_at_ms: number;
}

export interface StartListenAlongPayload {
  user_uri: string;
  device_id?: string | null;
  sync_pause?: boolean;
  auto_resync?: boolean;
  drift_threshold_ms?: number;
  latency_offset_ms?: number;
  auto_stop_on_manual_change?: boolean;
}

export interface StopListenAlongPayload {
  pause_playback?: boolean;
}

export interface ResyncListenAlongPayload {
  device_id?: string;
}

export type UpdateListenAlongSettingsPayload = Partial<ListenAlongSettings>;
