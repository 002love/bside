export interface StatsSummary {
  total_plays: number;
  unique_tracks: number;
  unique_artists: number;
  first_played_ms: number | null;
  last_played_ms: number | null;
  top_tracks: [string, string, number][];
  top_artists: [string, number][];
  hourly: number[];
  total_listened_ms: number;
  known_duration_plays: number;
  skipped_plays: number;
  completed_plays: number;
  most_skipped_tracks: [string, string, number][];
  avg_popularity: number | null;
  most_obscure_track: [string, string, number] | null;
  most_mainstream_track: [string, string, number] | null;
  explicit_plays: number;
  release_decades: [string, number][];
  context_breakdown: [string, number][];
  active_days: number;
  longest_streak_days: number;
  current_streak_days: number;
  weekday: number[];
  heatmap_grid: number[][];
  session_count: number;
  avg_session_ms: number | null;
}

export interface FriendSummary {
  user_uri: string;
  user_name: string;
  image_url: string | null;
  total_plays: number;
  last_active_ms: number;
  last_active_local: string;
  latest_track_uri: string;
  latest_track_name: string;
  latest_artist_name: string;
  latest_cover_url: string | null;
  latest_spotify_url: string;
  is_live: boolean;
}

export interface ListenRecord {
  id: number;
  played_at_ms: number;
  played_at_local: string;
  logged_at_ms: number;
  user_uri: string;
  user_name: string;
  track_uri: string;
  track_name: string;
  artist_name: string;
  artist_uri: string | null;
  album_name: string | null;
  album_uri: string | null;
  context_name: string | null;
  context_uri: string | null;
  item_type: string;
  all_artists: string | null;
  duration_ms: number | null;
  popularity: number | null;
  explicit: boolean | null;
  album_type: string | null;
  release_date: string | null;
  album_label: string | null;
  cover_url: string | null;
  isrc: string | null;
  context_type: string | null;
  context_owner: string | null;
  context_is_private: boolean;
  ended_at_ms: number | null;
  listened_ms: number | null;
  was_skipped: boolean | null;
  inferred_repeat: boolean;
  spotify_url: string;
}

export interface OverviewResponse {
  stats: StatsSummary;
  friends_summary: FriendSummary[];
  recent_listens: ListenRecord[];
}

export interface ContextSummary {
  user_uri: string;
  user_name: string;
  context_uri: string;
  context_name: string;
  context_type: string;
  context_owner: string | null;
  is_private: boolean;
  unique_tracks: number;
  total_plays: number;
  first_seen_ms: number;
  last_seen_ms: number;
}

export interface RebuiltTrack {
  track_uri: string;
  track_name: string;
  all_artists: string;
  album_name: string | null;
  duration_ms: number | null;
  release_date: string | null;
  popularity: number | null;
  play_count: number;
  first_seen_ms: number;
  last_seen_ms: number;
  spotify_url: string;
}

export interface FriendPairOverlap {
  friend_a: string;
  friend_b: string;
  shared_artists: number;
  total_union_artists: number;
  artist_jaccard: number;
  shared_tracks: number;
  total_union_tracks: number;
  track_jaccard: number;
  compatibility_score: number;
}

export interface TrendsetterTrack {
  artist_name: string;
  track_name: string;
  track_uri: string;
  friends_who_played: string[];
  first_player_name: string;
  first_played_ms: number;
  total_plays: number;
}

export interface OverlapReport {
  friend_pairs: FriendPairOverlap[];
  shared_artists: [string, string[], number][];
  shared_tracks: TrendsetterTrack[];
  trendsetter_leaderboard: [string, number][];
}

export interface NamedUri {
  uri: string;
  name: string;
}

export interface PublicPlaylist {
  uri: string;
  name: string;
  owner_name: string | null;
  owner_uri: string | null;
  followers_count: number | null;
  image_url: string | null;
}

export interface StoredProfile {
  user_uri: string;
  user_name: string;
  image_url: string | null;
  color: number | null;
  followers_count: number | null;
  following_count: number | null;
  recently_played_artists: NamedUri[];
  public_playlists: PublicPlaylist[];
  first_seen_ms: number;
  updated_at_ms: number;
}

export interface ProfileEvent {
  id: number;
  detected_at_ms: number;
  detected_at_local: string;
  user_uri: string;
  user_name: string;
  event_type: string;
  summary: string;
}

export type TimeframeKey = '7' | '30' | '90' | 'all';
