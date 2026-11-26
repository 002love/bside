import { createElement, type ReactNode } from 'react';
import {
  ListenAlongProvider,
  type ListenAlongProviderProps,
  type StartListenAlongOptions,
  type UseListenAlongOptions,
  type UseListenAlongResult,
  useListenAlong,
  useListenAlongContext,
} from '@/hooks/useListenAlong';
import {
  PlayerProvider,
  type PlayerProviderProps,
  type UsePlayerOptions,
  type UsePlayerResult,
  usePlayer,
  usePlayerContext,
} from '@/hooks/usePlayer';

export type {
  DeviceCommandPayload,
  PlayCommandPayload,
  PlayerCommandResponse,
  PlayerContext,
  PlayerDevice,
  PlayerDevicesResponse,
  PlayerErrorCode,
  PlayerNamedUri,
  PlayerQueueItem,
  PlayerRestrictions,
  PlayerSnapshot,
  PlayerTrack,
  QueueCommandPayload,
  RepeatCommandPayload,
  RepeatMode,
  SeekCommandPayload,
  ShuffleCommandPayload,
  TransferCommandPayload,
  VolumeCommandPayload,
} from '@/types/player';

export type {
  ListenAlongErrorCode,
  ListenAlongSettings,
  ListenAlongState,
  ListenAlongStatus,
  ListenAlongTarget,
  ResyncListenAlongPayload,
  StartListenAlongPayload,
  StopListenAlongPayload,
  UpdateListenAlongSettingsPayload,
} from '@/types/listenAlong';

export {
  PLAYER_STREAM_URL,
  PlaybackApiError,
  addToQueue,
  computeInterpolatedPositionMs,
  getNextRepeatMode,
  getPlayerDevices,
  getPlayerState,
  isPlayerSnapshot,
  pause,
  play,
  requestPlaybackJson,
  resume,
  seekTo,
  setRepeat,
  setShuffle,
  setVolume,
  skipNext,
  skipPrevious,
  toPlaybackApiError,
  togglePlayback,
  transferPlayback,
  type PlaybackApiErrorInit,
} from '@/lib/playerApi';

export {
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

export {
  ListenAlongProvider,
  PlayerProvider,
  useListenAlong,
  useListenAlongContext,
  usePlayer,
  usePlayerContext,
  type ListenAlongProviderProps,
  type PlayerProviderProps,
  type StartListenAlongOptions,
  type UseListenAlongOptions,
  type UseListenAlongResult,
  type UsePlayerOptions,
  type UsePlayerResult,
};

export interface PlaybackProviderProps {
  children: ReactNode;
  playerOptions?: UsePlayerOptions;
  listenAlongOptions?: UseListenAlongOptions;
}

export function PlaybackProvider({
  children,
  playerOptions,
  listenAlongOptions,
}: PlaybackProviderProps) {
  return createElement(PlayerProvider, {
    options: playerOptions,
    children: createElement(ListenAlongProvider, {
      options: listenAlongOptions,
      children,
    }),
  });
}
