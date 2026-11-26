use std::fmt;

use bytes::Bytes;
use chrono::Utc;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};
use http::{Method, Request};
use librespot_core::{
    Error as LibrespotError,
    dealer::protocol::TransferOptions,
    error::ErrorKind,
    session::Session,
    spclient::{CLIENT_TOKEN, TransferRequest},
};
use librespot_protocol::{
    connect::{Cluster, DeviceInfo, SetVolumeCommand},
    devices::DeviceType,
    player::{ProvidedTrack, Restrictions},
};
use protobuf::Message;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::models::{
    NamedUri, classify_context_type, is_spotify_dj_context, normalize_context_uri,
};

pub const MAX_SPOTIFY_VOLUME: u32 = 65_535;
const MAX_QUEUE_NEXT_ITEMS: usize = 20;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    Context,
    Track,
}

impl RepeatMode {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Context => "context",
            Self::Track => "track",
        }
    }
}

impl fmt::Display for RepeatMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerDevice {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub is_active: bool,
    pub is_private_session: bool,
    pub is_restricted: bool,
    pub is_offline: bool,
    pub volume_percent: u8,
    pub volume_raw: u32,
    pub supports_volume: bool,
    pub volume_steps: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerTrack {
    pub uri: String,
    pub name: String,
    pub artists: Vec<NamedUri>,
    pub all_artists: String,
    pub album: Option<NamedUri>,
    pub cover_url: Option<String>,
    pub duration_ms: i64,
    pub item_type: String,
    pub spotify_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerContext {
    pub uri: String,
    pub name: Option<String>,
    pub context_type: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerQueueItem {
    pub uri: String,
    pub name: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub cover_url: Option<String>,
    pub duration_ms: Option<i64>,
    pub item_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRestrictions {
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_seek: bool,
    pub can_skip_next: bool,
    pub can_skip_prev: bool,
    pub can_toggle_shuffle: bool,
    pub can_toggle_repeat_context: bool,
    pub can_toggle_repeat_track: bool,
    pub can_add_to_queue: bool,
    pub can_transfer: bool,
}

impl Default for PlayerRestrictions {
    fn default() -> Self {
        Self {
            can_pause: true,
            can_resume: true,
            can_seek: true,
            can_skip_next: true,
            can_skip_prev: true,
            can_toggle_shuffle: true,
            can_toggle_repeat_context: true,
            can_toggle_repeat_track: true,
            can_add_to_queue: true,
            can_transfer: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub connected: bool,
    pub updated_at_ms: i64,
    pub active_device_id: Option<String>,
    pub devices: Vec<PlayerDevice>,
    pub is_playing: bool,
    pub is_paused: bool,
    pub is_buffering: bool,
    pub playback_speed: f64,
    pub position_ms: i64,
    pub position_timestamp_ms: i64,
    pub duration_ms: i64,
    pub shuffle: bool,
    pub repeat_mode: RepeatMode,
    pub track: Option<PlayerTrack>,
    pub context: Option<PlayerContext>,
    pub queue_next: Vec<PlayerQueueItem>,
    pub restrictions: PlayerRestrictions,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            connected: false,
            updated_at_ms: 0,
            active_device_id: None,
            devices: Vec::new(),
            is_playing: false,
            is_paused: false,
            is_buffering: false,
            playback_speed: 1.0,
            position_ms: 0,
            position_timestamp_ms: 0,
            duration_ms: 0,
            shuffle: false,
            repeat_mode: RepeatMode::Off,
            track: None,
            context: None,
            queue_next: Vec::new(),
            restrictions: PlayerRestrictions::default(),
        }
    }
}

impl PlayerSnapshot {
    pub fn estimated_position_ms(&self, now_ms: i64) -> i64 {
        let base = self.position_ms.max(0);
        let pos = if self.is_playing
            && !self.is_paused
            && !self.is_buffering
            && self.position_timestamp_ms > 0
        {
            let elapsed = (now_ms - self.position_timestamp_ms).max(0) as f64;
            let speed = if self.playback_speed > 0.0 && self.playback_speed.is_finite() {
                self.playback_speed
            } else {
                1.0
            };
            base.saturating_add((elapsed * speed) as i64)
        } else {
            base
        };

        if self.duration_ms > 0 {
            pos.clamp(0, self.duration_ms)
        } else {
            pos.max(0)
        }
    }

    pub fn resolve_target_device_id(&self, requested: Option<&str>) -> Option<String> {
        if let Some(req) = requested.map(str::trim).filter(|s| !s.is_empty()) {
            return Some(req.to_string());
        }
        if let Some(active) = self
            .active_device_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(active.to_string());
        }
        if let Some(device) = self
            .devices
            .iter()
            .find(|d| !d.is_offline && !d.is_restricted)
        {
            return Some(device.id.clone());
        }
        if let Some(device) = self.devices.iter().find(|d| !d.is_offline) {
            return Some(device.id.clone());
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerErrorCode {
    NoActiveDevice,
    Unauthenticated,
    Offline,
    InvalidInput,
    CommandFailed,
}

impl PlayerErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoActiveDevice => "no_active_device",
            Self::Unauthenticated => "unauthenticated",
            Self::Offline => "offline",
            Self::InvalidInput => "invalid_input",
            Self::CommandFailed => "command_failed",
        }
    }
}

impl fmt::Display for PlayerErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerError {
    pub code: PlayerErrorCode,
    pub message: String,
}

impl PlayerError {
    pub fn new(code: PlayerErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn no_active_device(message: impl Into<String>) -> Self {
        Self::new(PlayerErrorCode::NoActiveDevice, message)
    }

    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(PlayerErrorCode::Unauthenticated, message)
    }

    pub fn offline(message: impl Into<String>) -> Self {
        Self::new(PlayerErrorCode::Offline, message)
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(PlayerErrorCode::InvalidInput, message)
    }

    pub fn command_failed(message: impl Into<String>) -> Self {
        Self::new(PlayerErrorCode::CommandFailed, message)
    }
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for PlayerError {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayRequest {
    #[serde(default)]
    pub track_uri: Option<String>,
    #[serde(default)]
    pub context_uri: Option<String>,
    #[serde(default)]
    pub position_ms: Option<i64>,
    #[serde(default)]
    pub skip_to_uri: Option<String>,
    #[serde(default)]
    pub skip_to_index: Option<u32>,
    #[serde(default)]
    pub initially_paused: Option<bool>,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[allow(dead_code)]
pub type PlayOptionsInput = PlayRequest;

pub fn percent_to_spotify_volume(percent: u8) -> u32 {
    if percent == 0 {
        return 0;
    }
    if percent >= 100 {
        return MAX_SPOTIFY_VOLUME;
    }
    ((percent as f64 / 100.0) * MAX_SPOTIFY_VOLUME as f64).round() as u32
}

pub fn spotify_volume_to_percent(raw: u32) -> u8 {
    let clamped = raw.min(MAX_SPOTIFY_VOLUME);
    ((clamped as f64 / MAX_SPOTIFY_VOLUME as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

fn device_type_to_str(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::COMPUTER => "computer",
        DeviceType::SMARTPHONE => "smartphone",
        DeviceType::SPEAKER => "speaker",
        DeviceType::TV => "tv",
        DeviceType::TABLET => "tablet",
        DeviceType::CAST_AUDIO => "cast_audio",
        DeviceType::CAST_VIDEO => "cast_video",
        DeviceType::AVR => "avr",
        DeviceType::STB => "stb",
        DeviceType::AUDIO_DONGLE => "audio_dongle",
        DeviceType::GAME_CONSOLE => "game_console",
        DeviceType::AUTOMOBILE => "automobile",
        DeviceType::SMARTWATCH => "smartwatch",
        DeviceType::CHROMEBOOK => "chromebook",
        DeviceType::CAR_THING => "car_thing",
        DeviceType::HOME_THING => "home_thing",
        DeviceType::OBSERVER => "observer",
        DeviceType::UNKNOWN | DeviceType::UNKNOWN_SPOTIFY => "unknown",
    }
}

fn normalize_spotify_image_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(id) = trimmed.strip_prefix("spotify:image:") {
        if id.is_empty() {
            None
        } else {
            Some(format!("https://i.scdn.co/image/{id}"))
        }
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn spotify_uri_to_web_url(uri: &str) -> String {
    let trimmed = uri.trim();
    let mut parts = trimmed.split(':');
    if let (Some("spotify"), Some(kind), Some(id)) = (parts.next(), parts.next(), parts.next())
        && !kind.is_empty()
        && !id.is_empty()
    {
        return format!("https://open.spotify.com/{kind}/{id}");
    }
    "https://open.spotify.com".to_string()
}

fn is_valid_playable_track_uri(uri: &str) -> bool {
    !uri.is_empty()
        && !uri.starts_with("spotify:ad:")
        && !uri.starts_with("spotify:delimiter")
}

fn extract_artists(track: &ProvidedTrack) -> Vec<NamedUri> {
    let meta = &track.metadata;
    let mut artists: Vec<NamedUri> = Vec::new();
    let primary_artist_name = meta
        .get("artist_name")
        .map(String::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let primary_artist_uri = if !track.artist_uri.trim().is_empty() {
        track.artist_uri.trim()
    } else {
        meta.get("artist_uri")
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default()
    };
    if !primary_artist_name.is_empty() || !primary_artist_uri.is_empty() {
        artists.push(NamedUri {
            uri: primary_artist_uri.to_string(),
            name: primary_artist_name.to_string(),
        });
    }
    for idx in 1..=10 {
        let name_key = format!("artist_name:{idx}");
        let uri_key = format!("artist_uri:{idx}");
        let extra_name = meta
            .get(&name_key)
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let extra_uri = meta
            .get(&uri_key)
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if extra_name.is_empty() && extra_uri.is_empty() {
            break;
        }
        artists.push(NamedUri {
            uri: extra_uri.to_string(),
            name: extra_name.to_string(),
        });
    }
    artists
}

fn format_all_artists(artists: &[NamedUri]) -> String {
    artists
        .iter()
        .map(|a| a.name.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn extract_cover_url(meta: &std::collections::HashMap<String, String>) -> Option<String> {
    [
        "image_xlarge_url",
        "image_large_url",
        "image_url",
        "image_small_url",
    ]
    .into_iter()
    .find_map(|key| meta.get(key).and_then(|v| normalize_spotify_image_url(v)))
}

fn parse_device(map_id: &str, info: &DeviceInfo, active_device_id: Option<&str>) -> PlayerDevice {
    let id = if !info.device_id.trim().is_empty() {
        info.device_id.trim().to_string()
    } else {
        map_id.trim().to_string()
    };
    let name = if info.name.trim().is_empty() {
        id.clone()
    } else {
        info.name.trim().to_string()
    };
    let device_type = device_type_to_str(info.device_type.enum_value_or(DeviceType::UNKNOWN))
        .to_string();
    let caps = info.capabilities.as_ref();
    let is_restricted = !info.can_play
        || !info.disallow_playback_reasons.is_empty()
        || caps.is_some_and(|c| !c.is_controllable);
    let volume_raw = info.volume.min(MAX_SPOTIFY_VOLUME);
    let volume_percent = spotify_volume_to_percent(volume_raw);
    let supports_volume = !caps.is_some_and(|c| c.disable_volume);
    let volume_steps = caps.map(|c| c.volume_steps).unwrap_or(0);
    let is_active = active_device_id.is_some_and(|active| active == id);

    PlayerDevice {
        id,
        name,
        device_type,
        is_active,
        is_private_session: info.is_private_session,
        is_restricted,
        is_offline: info.is_offline,
        volume_percent,
        volume_raw,
        supports_volume,
        volume_steps,
    }
}

fn parse_restrictions(restrictions: Option<&Restrictions>) -> PlayerRestrictions {
    let Some(r) = restrictions else {
        return PlayerRestrictions::default();
    };
    PlayerRestrictions {
        can_pause: r.disallow_pausing_reasons.is_empty(),
        can_resume: r.disallow_resuming_reasons.is_empty(),
        can_seek: r.disallow_seeking_reasons.is_empty(),
        can_skip_next: r.disallow_skipping_next_reasons.is_empty(),
        can_skip_prev: r.disallow_skipping_prev_reasons.is_empty(),
        can_toggle_shuffle: r.disallow_toggling_shuffle_reasons.is_empty(),
        can_toggle_repeat_context: r.disallow_toggling_repeat_context_reasons.is_empty(),
        can_toggle_repeat_track: r.disallow_toggling_repeat_track_reasons.is_empty(),
        can_add_to_queue: r.disallow_add_to_queue_reasons.is_empty(),
        can_transfer: r.disallow_transferring_playback_reasons.is_empty(),
    }
}

fn parse_current_track(track: &ProvidedTrack, state_duration_ms: i64) -> Option<PlayerTrack> {
    let track_uri = track.uri.trim();
    if !is_valid_playable_track_uri(track_uri) {
        return None;
    }

    let meta = &track.metadata;
    let name = meta
        .get("title")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(track_uri)
        .to_string();

    let artists = extract_artists(track);
    let all_artists = format_all_artists(&artists);

    let album_name = meta
        .get("album_title")
        .map(String::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let album_uri = if !track.album_uri.trim().is_empty() {
        track.album_uri.trim()
    } else {
        meta.get("album_uri")
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default()
    };
    let album = if !album_name.is_empty() || !album_uri.is_empty() {
        Some(NamedUri {
            uri: album_uri.to_string(),
            name: album_name.to_string(),
        })
    } else {
        None
    };

    let cover_url = extract_cover_url(meta);
    let duration_ms = if state_duration_ms > 0 {
        state_duration_ms
    } else {
        meta.get("duration")
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|&d| d > 0)
            .unwrap_or(0)
    };
    let item_type = if track_uri.starts_with("spotify:episode:") {
        "episode".to_string()
    } else {
        "track".to_string()
    };
    let spotify_url = spotify_uri_to_web_url(track_uri);

    Some(PlayerTrack {
        uri: track_uri.to_string(),
        name,
        artists,
        all_artists,
        album,
        cover_url,
        duration_ms,
        item_type,
        spotify_url,
    })
}

fn parse_queue_item(track: &ProvidedTrack) -> Option<PlayerQueueItem> {
    let uri = track.uri.trim();
    if !is_valid_playable_track_uri(uri) {
        return None;
    }

    let meta = &track.metadata;
    let name = meta
        .get("title")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(uri)
        .to_string();

    let artists = extract_artists(track);
    let artist_name = format_all_artists(&artists);
    let album_name = meta
        .get("album_title")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let cover_url = extract_cover_url(meta);
    let duration_ms = meta
        .get("duration")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|&d| d > 0);
    let item_type = if uri.starts_with("spotify:episode:") {
        "episode".to_string()
    } else {
        "track".to_string()
    };

    Some(PlayerQueueItem {
        uri: uri.to_string(),
        name,
        artist_name,
        album_name,
        cover_url,
        duration_ms,
        item_type,
    })
}

pub fn cluster_to_player_snapshot(
    cluster: &Cluster,
    self_device_id: &str,
    now_ms: i64,
) -> PlayerSnapshot {
    let self_id = self_device_id.trim();
    let active_device_id = {
        let raw = cluster.active_device_id.trim();
        if raw.is_empty() || (!self_id.is_empty() && raw == self_id) {
            None
        } else {
            Some(raw.to_string())
        }
    };

    let mut devices: Vec<PlayerDevice> = cluster
        .device
        .iter()
        .filter(|(map_id, info)| {
            let id_trimmed = info.device_id.trim();
            let is_self = !self_id.is_empty()
                && (map_id.trim() == self_id || id_trimmed == self_id);
            let is_observer = info.device_type.enum_value() == Ok(DeviceType::OBSERVER);
            let is_hidden = info.capabilities.as_ref().is_some_and(|c| c.hidden);
            !is_self && !is_observer && !is_hidden
        })
        .map(|(map_id, info)| parse_device(map_id, info, active_device_id.as_deref()))
        .collect();

    devices.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });

    let Some(player_state) = cluster.player_state.as_ref() else {
        return PlayerSnapshot {
            connected: true,
            updated_at_ms: now_ms,
            active_device_id,
            devices,
            ..PlayerSnapshot::default()
        };
    };

    let is_playing = player_state.is_playing && !player_state.is_paused;
    let is_paused = player_state.is_paused;
    let is_buffering = player_state.is_buffering;
    let playback_speed = if player_state.playback_speed > 0.0
        && player_state.playback_speed.is_finite()
    {
        player_state.playback_speed
    } else {
        1.0
    };
    let position_ms = player_state.position_as_of_timestamp.max(0);
    let position_timestamp_ms = player_state.timestamp;
    let duration_ms = player_state.duration.max(0);

    let (shuffle, repeat_mode) = match player_state.options.as_ref() {
        Some(opts) => {
            let mode = if opts.repeating_track {
                RepeatMode::Track
            } else if opts.repeating_context {
                RepeatMode::Context
            } else {
                RepeatMode::Off
            };
            (opts.shuffling_context, mode)
        }
        None => (false, RepeatMode::Off),
    };

    let track = player_state
        .track
        .as_ref()
        .and_then(|t| parse_current_track(t, duration_ms));

    let raw_ctx = player_state.context_uri.trim();
    let context = if !raw_ctx.is_empty() {
        let ctx_uri = normalize_context_uri(raw_ctx);
        let ctx_meta_title = player_state
            .context_metadata
            .get("context_description")
            .or_else(|| player_state.context_metadata.get("title"))
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let is_liked_songs = ctx_uri.eq_ignore_ascii_case("spotify:collection")
            || ctx_uri.ends_with(":collection")
            || ctx_uri.contains(":collection:")
            || ctx_meta_title.is_some_and(|t| t.eq_ignore_ascii_case("liked songs"));
        let is_dj = is_spotify_dj_context(Some(&ctx_uri), None);

        if is_liked_songs {
            Some(PlayerContext {
                uri: ctx_uri,
                name: Some("Liked Songs".to_string()),
                context_type: "liked_songs".to_string(),
                image_url: None,
            })
        } else if is_dj {
            Some(PlayerContext {
                uri: ctx_uri,
                name: Some("DJ".to_string()),
                context_type: "dj".to_string(),
                image_url: None,
            })
        } else {
            let is_album_ctx = track
                .as_ref()
                .and_then(|t| t.album.as_ref())
                .is_some_and(|a| a.uri == ctx_uri);
            let name = if is_album_ctx {
                track
                    .as_ref()
                    .and_then(|t| t.album.as_ref())
                    .map(|a| a.name.clone())
                    .filter(|s| !s.is_empty())
                    .or_else(|| ctx_meta_title.map(str::to_string))
            } else if let Some(artist) = track
                .as_ref()
                .and_then(|t| t.artists.iter().find(|a| a.uri == ctx_uri))
                && !artist.name.is_empty()
            {
                Some(artist.name.clone())
            } else if track.as_ref().is_some_and(|t| t.uri == ctx_uri) {
                track.as_ref().map(|t| t.name.clone())
            } else {
                ctx_meta_title.map(str::to_string)
            };

            let context_type =
                classify_context_type(&ctx_uri, name.as_deref().unwrap_or("")).to_string();
            let image_url = player_state
                .context_metadata
                .get("image_url")
                .and_then(|v| normalize_spotify_image_url(v))
                .or_else(|| {
                    if is_album_ctx {
                        track.as_ref().and_then(|t| t.cover_url.clone())
                    } else {
                        None
                    }
                });

            Some(PlayerContext {
                uri: ctx_uri,
                name,
                context_type,
                image_url,
            })
        }
    } else {
        None
    };

    let queue_next: Vec<PlayerQueueItem> = player_state
        .next_tracks
        .iter()
        .filter_map(parse_queue_item)
        .take(MAX_QUEUE_NEXT_ITEMS)
        .collect();

    let restrictions = parse_restrictions(player_state.restrictions.as_ref());

    PlayerSnapshot {
        connected: true,
        updated_at_ms: now_ms,
        active_device_id,
        devices,
        is_playing,
        is_paused,
        is_buffering,
        playback_speed,
        position_ms,
        position_timestamp_ms,
        duration_ms,
        shuffle,
        repeat_mode,
        track,
        context,
        queue_next,
        restrictions,
    }
}

fn validate_spotify_uri<'a>(value: &'a str, field_name: &str) -> Result<&'a str, PlayerError> {
    let trimmed = value.trim();
    if !trimmed.starts_with("spotify:") {
        return Err(PlayerError::invalid_input(format!(
            "{field_name} must be a valid spotify: URI"
        )));
    }
    Ok(trimmed)
}

pub fn build_play_command_json(req: &PlayRequest, now_ms: i64) -> Result<Value, PlayerError> {
    let context_uri = req
        .context_uri
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let track_uri = req
        .track_uri
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let skip_to_uri = req
        .skip_to_uri
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(ctx) = context_uri {
        validate_spotify_uri(ctx, "context_uri")?;
    }
    if let Some(track) = track_uri {
        validate_spotify_uri(track, "track_uri")?;
    }
    if let Some(skip) = skip_to_uri {
        validate_spotify_uri(skip, "skip_to_uri")?;
    }

    if context_uri.is_none() && track_uri.is_none() {
        return Ok(build_simple_command_json("resume", now_ms));
    }

    let mut options = Map::new();
    let context = if let Some(ctx) = context_uri {
        if let Some(uri) = skip_to_uri.or(track_uri) {
            options.insert("skip_to".to_string(), json!({ "track_uri": uri }));
        } else if let Some(idx) = req.skip_to_index {
            options.insert("skip_to".to_string(), json!({ "track_index": idx }));
        }
        json!({
            "uri": ctx,
            "url": format!("context://{ctx}"),
            "metadata": {}
        })
    } else {
        let track = track_uri.expect("checked track_uri is Some when context_uri is None");
        options.insert("skip_to".to_string(), json!({ "track_uri": track }));
        json!({
            "uri": track,
            "url": format!("context://{track}"),
            "pages": [{ "tracks": [{ "uri": track }] }],
            "metadata": {}
        })
    };

    if let Some(pos_ms) = req.position_ms {
        let seek_to = pos_ms.clamp(0, u32::MAX as i64) as u32;
        options.insert("seek_to".to_string(), json!(seek_to));
    }
    if let Some(initially_paused) = req.initially_paused {
        options.insert("initially_paused".to_string(), json!(initially_paused));
    }

    Ok(json!({
        "command": {
            "endpoint": "play",
            "context": context,
            "play_origin": {
                "feature_identifier": "harmony",
                "feature_version": "4.0.0"
            },
            "options": Value::Object(options),
            "logging_params": {
                "command_initiated_time": now_ms
            }
        }
    }))
}

pub fn build_simple_command_json(endpoint: &str, now_ms: i64) -> Value {
    json!({
        "command": {
            "endpoint": endpoint,
            "logging_params": {
                "command_initiated_time": now_ms
            }
        }
    })
}

pub fn build_seek_command_json(position_ms: u32, now_ms: i64) -> Value {
    json!({
        "command": {
            "endpoint": "seek_to",
            "value": position_ms,
            "position": position_ms,
            "logging_params": {
                "command_initiated_time": now_ms
            }
        }
    })
}

pub fn build_shuffle_command_json(enabled: bool, now_ms: i64) -> Value {
    json!({
        "command": {
            "endpoint": "set_shuffling_context",
            "value": enabled,
            "logging_params": {
                "command_initiated_time": now_ms
            }
        }
    })
}

pub fn build_repeat_command_json(mode: RepeatMode, now_ms: i64) -> Value {
    let (repeating_context, repeating_track) = match mode {
        RepeatMode::Off => (false, false),
        RepeatMode::Context => (true, false),
        RepeatMode::Track => (false, true),
    };
    json!({
        "command": {
            "endpoint": "set_options",
            "repeating_context": repeating_context,
            "repeating_track": repeating_track,
            "logging_params": {
                "command_initiated_time": now_ms
            }
        }
    })
}

pub fn build_add_to_queue_command_json(track_uri: &str, now_ms: i64) -> Value {
    json!({
        "command": {
            "endpoint": "add_to_queue",
            "track": {
                "uri": track_uri,
                "provider": "queue"
            },
            "logging_params": {
                "command_initiated_time": now_ms
            }
        }
    })
}

fn map_librespot_error(context: &str, err: LibrespotError) -> PlayerError {
    let code = match err.kind {
        ErrorKind::Unauthenticated => PlayerErrorCode::Unauthenticated,
        ErrorKind::Unavailable | ErrorKind::DeadlineExceeded => PlayerErrorCode::Offline,
        ErrorKind::InvalidArgument | ErrorKind::OutOfRange => PlayerErrorCode::InvalidInput,
        _ => PlayerErrorCode::CommandFailed,
    };
    PlayerError::new(code, format!("{context}: {err}"))
}

fn require_target_device_id(target_device_id: &str) -> Result<&str, PlayerError> {
    let trimmed = target_device_id.trim();
    if trimmed.is_empty() {
        return Err(PlayerError::no_active_device(
            "No target Spotify Connect device available",
        ));
    }
    Ok(trimmed)
}

pub async fn send_connect_command(
    session: &Session,
    target_device_id: &str,
    payload: &Value,
) -> Result<(), PlayerError> {
    let target = require_target_device_id(target_device_id)?;
    let from_device_id = session.device_id();

    let base_url = session
        .spclient()
        .base_url()
        .await
        .map_err(|e| map_librespot_error("Failed to resolve spclient access point", e))?;
    let url = format!(
        "{base_url}/connect-state/v1/player/command/from/{from_device_id}/to/{target}"
    );

    let body_vec = serde_json::to_vec(payload)
        .map_err(|e| PlayerError::invalid_input(format!("Failed to serialize command JSON: {e}")))?;
    let body_bytes = Bytes::from(body_vec);

    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|e| PlayerError::unauthenticated(format!("Failed to obtain auth token: {e}")))?;
    let auth_header = HeaderValue::from_str(&format!("{} {}", token.token_type, token.access_token))
        .map_err(|e| PlayerError::unauthenticated(format!("Invalid auth header: {e}")))?;

    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(&url)
        .header(ACCEPT, HeaderValue::from_static("application/json"))
        .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .header(CONTENT_LENGTH, body_bytes.len())
        .header(AUTHORIZATION, auth_header);

    if let Ok(client_token) = session.spclient().client_token().await
        && let Ok(val) = HeaderValue::from_str(&client_token)
    {
        builder = builder.header(CLIENT_TOKEN, val);
    }

    let request = builder
        .body(body_bytes)
        .map_err(|e| PlayerError::command_failed(format!("Failed to build command request: {e}")))?;

    session
        .http_client()
        .request_body(request)
        .await
        .map(|_| ())
        .map_err(|e| map_librespot_error("Spotify Connect command failed", e))
}

pub async fn send_volume_command(
    session: &Session,
    target_device_id: &str,
    volume_percent: u8,
) -> Result<(), PlayerError> {
    let target = require_target_device_id(target_device_id)?;
    let from_device_id = session.device_id();

    let cmd = SetVolumeCommand {
        volume: percent_to_spotify_volume(volume_percent) as i32,
        ..Default::default()
    };
    let body_vec = cmd.write_to_bytes().map_err(|e| {
        PlayerError::command_failed(format!("Failed to encode SetVolumeCommand protobuf: {e}"))
    })?;
    let body_bytes = Bytes::from(body_vec);

    let base_url = session
        .spclient()
        .base_url()
        .await
        .map_err(|e| map_librespot_error("Failed to resolve spclient access point", e))?;
    let url = format!(
        "{base_url}/connect-state/v1/connect/volume/from/{from_device_id}/to/{target}"
    );

    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|e| PlayerError::unauthenticated(format!("Failed to obtain auth token: {e}")))?;
    let auth_header = HeaderValue::from_str(&format!("{} {}", token.token_type, token.access_token))
        .map_err(|e| PlayerError::unauthenticated(format!("Invalid auth header: {e}")))?;

    let mut builder = Request::builder()
        .method(Method::PUT)
        .uri(&url)
        .header(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-protobuf"),
        )
        .header(CONTENT_LENGTH, body_bytes.len())
        .header(AUTHORIZATION, auth_header);

    if let Ok(client_token) = session.spclient().client_token().await
        && let Ok(val) = HeaderValue::from_str(&client_token)
    {
        builder = builder.header(CLIENT_TOKEN, val);
    }

    let request = builder
        .body(body_bytes)
        .map_err(|e| PlayerError::command_failed(format!("Failed to build volume request: {e}")))?;

    session
        .http_client()
        .request_body(request)
        .await
        .map(|_| ())
        .map_err(|e| map_librespot_error("Spotify Connect volume command failed", e))
}

pub async fn send_transfer_command(
    session: &Session,
    target_device_id: &str,
    play: Option<bool>,
) -> Result<(), PlayerError> {
    let target = require_target_device_id(target_device_id)?;
    let transfer_req = play.map(|should_play| TransferRequest {
        transfer_options: TransferOptions {
            restore_paused: Some(if should_play { "resume" } else { "pause" }.to_string()),
            ..Default::default()
        },
    });

    session
        .spclient()
        .transfer(session.device_id(), target, transfer_req.as_ref())
        .await
        .map(|_| ())
        .map_err(|e| map_librespot_error("Spotify Connect transfer failed", e))
}

pub async fn play_with_context_fallback(
    session: &Session,
    target_device_id: &str,
    req: &PlayRequest,
) -> Result<(), PlayerError> {
    let now_ms = Utc::now().timestamp_millis();
    let payload = build_play_command_json(req, now_ms)?;
    match send_connect_command(session, target_device_id, &payload).await {
        Ok(()) => Ok(()),
        Err(err) => {
            let has_context = req
                .context_uri
                .as_deref()
                .map(str::trim)
                .is_some_and(|s| !s.is_empty());
            let has_track = req
                .track_uri
                .as_deref()
                .map(str::trim)
                .is_some_and(|s| !s.is_empty());
            let can_fallback = has_context
                && has_track
                && matches!(
                    err.code,
                    PlayerErrorCode::CommandFailed | PlayerErrorCode::InvalidInput
                );

            if !can_fallback {
                return Err(err);
            }

            let fallback_req = PlayRequest {
                track_uri: req.track_uri.clone(),
                context_uri: None,
                position_ms: req.position_ms,
                skip_to_uri: None,
                skip_to_index: None,
                initially_paused: req.initially_paused,
                device_id: req.device_id.clone(),
            };
            let fallback_payload =
                build_play_command_json(&fallback_req, Utc::now().timestamp_millis())?;
            send_connect_command(session, target_device_id, &fallback_payload).await
        }
    }
}

pub async fn play(
    session: &Session,
    target_device_id: &str,
    req: &PlayRequest,
) -> Result<(), PlayerError> {
    play_with_context_fallback(session, target_device_id, req).await
}

pub async fn pause(session: &Session, target_device_id: &str) -> Result<(), PlayerError> {
    let payload = build_simple_command_json("pause", Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn resume(session: &Session, target_device_id: &str) -> Result<(), PlayerError> {
    let payload = build_simple_command_json("resume", Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn skip_next(session: &Session, target_device_id: &str) -> Result<(), PlayerError> {
    let payload = build_simple_command_json("skip_next", Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn skip_prev(session: &Session, target_device_id: &str) -> Result<(), PlayerError> {
    let payload = build_simple_command_json("skip_prev", Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn seek(
    session: &Session,
    target_device_id: &str,
    position_ms: i64,
) -> Result<(), PlayerError> {
    if position_ms < 0 {
        return Err(PlayerError::invalid_input(
            "position_ms must be non-negative",
        ));
    }
    let pos = position_ms.min(u32::MAX as i64) as u32;
    let payload = build_seek_command_json(pos, Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn set_volume(
    session: &Session,
    target_device_id: &str,
    volume_percent: u8,
) -> Result<(), PlayerError> {
    if volume_percent > 100 {
        return Err(PlayerError::invalid_input(
            "volume_percent must be between 0 and 100",
        ));
    }
    send_volume_command(session, target_device_id, volume_percent).await
}

pub async fn set_shuffle(
    session: &Session,
    target_device_id: &str,
    enabled: bool,
) -> Result<(), PlayerError> {
    let payload = build_shuffle_command_json(enabled, Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn set_repeat(
    session: &Session,
    target_device_id: &str,
    mode: RepeatMode,
) -> Result<(), PlayerError> {
    let payload = build_repeat_command_json(mode, Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn add_to_queue(
    session: &Session,
    target_device_id: &str,
    track_uri: &str,
) -> Result<(), PlayerError> {
    let uri = track_uri.trim();
    if !(uri.starts_with("spotify:track:") || uri.starts_with("spotify:episode:")) {
        return Err(PlayerError::invalid_input(
            "track_uri must be a spotify:track: or spotify:episode: URI",
        ));
    }
    let payload = build_add_to_queue_command_json(uri, Utc::now().timestamp_millis());
    send_connect_command(session, target_device_id, &payload).await
}

pub async fn transfer(
    session: &Session,
    target_device_id: &str,
    play: Option<bool>,
) -> Result<(), PlayerError> {
    send_transfer_command(session, target_device_id, play).await
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use librespot_protocol::connect::{Capabilities, Cluster, DeviceInfo};
    use librespot_protocol::devices::DeviceType;
    use librespot_protocol::player::{
        ContextPlayerOptions, PlayerState, ProvidedTrack, Restrictions,
    };
    use protobuf::MessageField;

    use super::*;

    fn make_device_info(
        id: &str,
        name: &str,
        device_type: DeviceType,
        volume: u32,
        caps: Capabilities,
    ) -> DeviceInfo {
        DeviceInfo {
            can_play: true,
            volume,
            name: name.to_string(),
            device_id: id.to_string(),
            device_type: device_type.into(),
            capabilities: MessageField::some(caps),
            ..Default::default()
        }
    }

    fn controllable_caps() -> Capabilities {
        Capabilities {
            can_be_player: true,
            is_controllable: true,
            volume_steps: 64,
            ..Default::default()
        }
    }

    fn sample_provided_track(uri: &str, title: &str, artist: &str) -> ProvidedTrack {
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), title.to_string());
        metadata.insert("artist_name".to_string(), artist.to_string());
        metadata.insert("artist_name:1".to_string(), "Featured Guest".to_string());
        metadata.insert("artist_uri:1".to_string(), "spotify:artist:guest".to_string());
        metadata.insert("album_title".to_string(), "Test Album".to_string());
        metadata.insert(
            "image_large_url".to_string(),
            "spotify:image:ab67616d0000b273abcdef".to_string(),
        );
        metadata.insert("duration".to_string(), "215000".to_string());

        ProvidedTrack {
            uri: uri.to_string(),
            artist_uri: "spotify:artist:primary".to_string(),
            album_uri: "spotify:album:alb123".to_string(),
            metadata,
            ..Default::default()
        }
    }

    #[test]
    fn volume_conversion_boundaries_and_roundtrip() {
        assert_eq!(percent_to_spotify_volume(0), 0);
        assert_eq!(percent_to_spotify_volume(50), 32_768);
        assert_eq!(percent_to_spotify_volume(100), 65_535);
        assert_eq!(percent_to_spotify_volume(255), 65_535);

        assert_eq!(spotify_volume_to_percent(0), 0);
        assert_eq!(spotify_volume_to_percent(32_768), 50);
        assert_eq!(spotify_volume_to_percent(65_535), 100);
        assert_eq!(spotify_volume_to_percent(99_999), 100);

        for pct in 0..=100u8 {
            let raw = percent_to_spotify_volume(pct);
            assert_eq!(spotify_volume_to_percent(raw), pct);
        }
    }

    #[test]
    fn cluster_to_player_snapshot_filters_and_sorts_devices() {
        let mut devices = HashMap::new();
        // Self observer device -> filtered out
        devices.insert(
            "self-dev".to_string(),
            make_device_info(
                "self-dev",
                "bside",
                DeviceType::OBSERVER,
                0,
                Capabilities {
                    hidden: true,
                    ..Default::default()
                },
            ),
        );
        // Another hidden device -> filtered out
        devices.insert(
            "hidden-dev".to_string(),
            make_device_info(
                "hidden-dev",
                "Hidden",
                DeviceType::COMPUTER,
                65_535,
                Capabilities {
                    hidden: true,
                    is_controllable: true,
                    ..Default::default()
                },
            ),
        );
        // Another observer -> filtered out
        devices.insert(
            "other-obs".to_string(),
            make_device_info(
                "other-obs",
                "Other Observer",
                DeviceType::OBSERVER,
                0,
                Capabilities::default(),
            ),
        );
        // Regular devices
        devices.insert(
            "dev-z".to_string(),
            make_device_info(
                "dev-z",
                "Zebra Speaker",
                DeviceType::SPEAKER,
                32_768,
                Capabilities {
                    is_controllable: true,
                    disable_volume: true,
                    volume_steps: 16,
                    ..Default::default()
                },
            ),
        );
        let mut restricted_phone = make_device_info(
            "dev-a",
            "Alpha Phone",
            DeviceType::SMARTPHONE,
            65_535,
            Capabilities {
                is_controllable: false,
                ..Default::default()
            },
        );
        restricted_phone.is_private_session = true;
        devices.insert("dev-a".to_string(), restricted_phone);

        devices.insert(
            "dev-m".to_string(),
            make_device_info(
                "dev-m",
                "MacBook Pro",
                DeviceType::COMPUTER,
                65_535,
                controllable_caps(),
            ),
        );

        let cluster = Cluster {
            active_device_id: "dev-m".to_string(),
            device: devices,
            ..Default::default()
        };

        let snap = cluster_to_player_snapshot(&cluster, "self-dev", 1_700_000_000_000);
        assert!(snap.connected);
        assert_eq!(snap.updated_at_ms, 1_700_000_000_000);
        assert_eq!(snap.active_device_id.as_deref(), Some("dev-m"));
        assert_eq!(snap.devices.len(), 3);

        // Active device first, then alphabetical by name
        assert_eq!(snap.devices[0].id, "dev-m");
        assert!(snap.devices[0].is_active);
        assert_eq!(snap.devices[0].device_type, "computer");
        assert_eq!(snap.devices[0].volume_percent, 100);
        assert!(snap.devices[0].supports_volume);
        assert!(!snap.devices[0].is_restricted);
        assert_eq!(snap.devices[0].volume_steps, 64);

        assert_eq!(snap.devices[1].id, "dev-a");
        assert!(!snap.devices[1].is_active);
        assert_eq!(snap.devices[1].device_type, "smartphone");
        assert!(snap.devices[1].is_private_session);
        assert!(snap.devices[1].is_restricted);

        assert_eq!(snap.devices[2].id, "dev-z");
        assert_eq!(snap.devices[2].device_type, "speaker");
        assert_eq!(snap.devices[2].volume_percent, 50);
        assert!(!snap.devices[2].supports_volume);
        assert_eq!(snap.devices[2].volume_steps, 16);
    }

    #[test]
    fn cluster_to_player_snapshot_parses_full_player_state_queue_and_restrictions() {
        let mut next_tracks = Vec::new();
        next_tracks.push(sample_provided_track("spotify:ad:skipme", "Ad", "Sponsor"));
        next_tracks.push(sample_provided_track("spotify:delimiter", "", ""));
        for i in 0..25 {
            next_tracks.push(sample_provided_track(
                &format!("spotify:track:next{i}"),
                &format!("Next Track {i}"),
                "Queue Artist",
            ));
        }

        let mut context_metadata = HashMap::new();
        context_metadata.insert("context_description".to_string(), "Late Night Drive".to_string());
        context_metadata.insert(
            "image_url".to_string(),
            "spotify:image:ctximg123".to_string(),
        );

        let player_state = PlayerState {
            timestamp: 1_700_000_050_000,
            position_as_of_timestamp: 30_000,
            duration: 215_000,
            playback_speed: 0.0, // should default to 1.0
            is_playing: true,
            is_paused: false,
            is_buffering: false,
            context_uri: "spotify:playlist:pl999".to_string(),
            context_metadata,
            track: MessageField::some(sample_provided_track(
                "spotify:track:abc123",
                "Nightcall",
                "Kavinsky",
            )),
            options: MessageField::some(ContextPlayerOptions {
                shuffling_context: true,
                repeating_context: true,
                repeating_track: false,
                ..Default::default()
            }),
            restrictions: MessageField::some(Restrictions {
                disallow_skipping_prev_reasons: vec!["no_prev_track".to_string()],
                disallow_seeking_reasons: vec!["not_supported".to_string()],
                ..Default::default()
            }),
            next_tracks,
            ..Default::default()
        };

        let cluster = Cluster {
            active_device_id: "dev-1".to_string(),
            player_state: MessageField::some(player_state),
            ..Default::default()
        };

        let snap = cluster_to_player_snapshot(&cluster, "self-dev", 1_700_000_060_000);
        assert!(snap.connected);
        assert!(snap.is_playing);
        assert!(!snap.is_paused);
        assert!(!snap.is_buffering);
        assert_eq!(snap.playback_speed, 1.0);
        assert_eq!(snap.position_ms, 30_000);
        assert_eq!(snap.position_timestamp_ms, 1_700_000_050_000);
        assert_eq!(snap.duration_ms, 215_000);
        assert!(snap.shuffle);
        assert_eq!(snap.repeat_mode, RepeatMode::Context);

        let track = snap.track.as_ref().expect("track");
        assert_eq!(track.uri, "spotify:track:abc123");
        assert_eq!(track.name, "Nightcall");
        assert_eq!(track.all_artists, "Kavinsky, Featured Guest");
        assert_eq!(track.artists.len(), 2);
        assert_eq!(
            track.album.as_ref().map(|a| a.name.as_str()),
            Some("Test Album")
        );
        assert_eq!(
            track.cover_url.as_deref(),
            Some("https://i.scdn.co/image/ab67616d0000b273abcdef")
        );
        assert_eq!(track.duration_ms, 215_000);
        assert_eq!(track.item_type, "track");
        assert_eq!(track.spotify_url, "https://open.spotify.com/track/abc123");

        let ctx = snap.context.as_ref().expect("context");
        assert_eq!(ctx.uri, "spotify:playlist:pl999");
        assert_eq!(ctx.name.as_deref(), Some("Late Night Drive"));
        assert_eq!(ctx.context_type, "playlist");
        assert_eq!(
            ctx.image_url.as_deref(),
            Some("https://i.scdn.co/image/ctximg123")
        );

        // Filtered ads/delimiters and capped at 20
        assert_eq!(snap.queue_next.len(), 20);
        assert_eq!(snap.queue_next[0].uri, "spotify:track:next0");
        assert_eq!(snap.queue_next[0].artist_name, "Queue Artist, Featured Guest");
        assert_eq!(snap.queue_next[19].uri, "spotify:track:next19");

        // Restrictions
        assert!(snap.restrictions.can_pause);
        assert!(snap.restrictions.can_resume);
        assert!(!snap.restrictions.can_seek);
        assert!(snap.restrictions.can_skip_next);
        assert!(!snap.restrictions.can_skip_prev);
        assert!(snap.restrictions.can_toggle_shuffle);
        assert!(snap.restrictions.can_add_to_queue);
        assert!(snap.restrictions.can_transfer);
    }

    #[test]
    fn cluster_to_player_snapshot_handles_ads_dj_and_liked_songs() {
        let ad_cluster = Cluster {
            player_state: MessageField::some(PlayerState {
                track: MessageField::some(sample_provided_track("spotify:ad:123", "Ad", "Co")),
                context_uri: "spotify:playlist:37i9dQZF1EYkqdzj48dyYq".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let snap = cluster_to_player_snapshot(&ad_cluster, "self", 1000);
        assert!(snap.track.is_none());
        let ctx = snap.context.expect("dj context");
        assert_eq!(ctx.context_type, "dj");
        assert_eq!(ctx.name.as_deref(), Some("DJ"));

        let liked_cluster = Cluster {
            player_state: MessageField::some(PlayerState {
                context_uri: "spotify:user:alice:collection".to_string(),
                options: MessageField::some(ContextPlayerOptions {
                    repeating_track: true,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let liked_snap = cluster_to_player_snapshot(&liked_cluster, "self", 2000);
        assert_eq!(liked_snap.repeat_mode, RepeatMode::Track);
        let lctx = liked_snap.context.expect("liked songs context");
        assert_eq!(lctx.context_type, "liked_songs");
        assert_eq!(lctx.name.as_deref(), Some("Liked Songs"));
    }

    #[test]
    fn estimated_position_ms_advances_only_when_playing_and_clamps() {
        let mut snap = PlayerSnapshot {
            is_playing: true,
            is_paused: false,
            is_buffering: false,
            playback_speed: 1.0,
            position_ms: 10_000,
            position_timestamp_ms: 1_000_000,
            duration_ms: 60_000,
            ..PlayerSnapshot::default()
        };
        assert_eq!(snap.estimated_position_ms(1_015_000), 25_000);
        // Clamped to duration_ms
        assert_eq!(snap.estimated_position_ms(1_100_000), 60_000);
        // Earlier timestamp doesn't go backwards
        assert_eq!(snap.estimated_position_ms(999_000), 10_000);

        // 1.5x speed (e.g. podcast)
        snap.playback_speed = 1.5;
        assert_eq!(snap.estimated_position_ms(1_010_000), 25_000);

        // Paused -> stays at position_ms
        snap.is_paused = true;
        assert_eq!(snap.estimated_position_ms(1_020_000), 10_000);

        // Buffering -> stays at position_ms
        snap.is_paused = false;
        snap.is_buffering = true;
        assert_eq!(snap.estimated_position_ms(1_020_000), 10_000);
    }

    #[test]
    fn resolve_target_device_id_priority_order() {
        let mk = |id: &str, offline: bool, restricted: bool| PlayerDevice {
            id: id.to_string(),
            name: id.to_string(),
            device_type: "computer".to_string(),
            is_active: false,
            is_private_session: false,
            is_restricted: restricted,
            is_offline: offline,
            volume_percent: 50,
            volume_raw: 32_768,
            supports_volume: true,
            volume_steps: 64,
        };

        let mut snap = PlayerSnapshot {
            active_device_id: Some("active-dev".to_string()),
            devices: vec![
                mk("offline-dev", true, false),
                mk("restricted-dev", false, true),
                mk("healthy-dev", false, false),
            ],
            ..PlayerSnapshot::default()
        };

        // 1. Explicit requested device wins
        assert_eq!(
            snap.resolve_target_device_id(Some("explicit-dev")).as_deref(),
            Some("explicit-dev")
        );
        // Whitespace-only requested falls through to active
        assert_eq!(
            snap.resolve_target_device_id(Some("   ")).as_deref(),
            Some("active-dev")
        );

        // 2. Active device wins when no request
        assert_eq!(
            snap.resolve_target_device_id(None).as_deref(),
            Some("active-dev")
        );

        // 3. First non-offline, non-restricted device
        snap.active_device_id = None;
        assert_eq!(
            snap.resolve_target_device_id(None).as_deref(),
            Some("healthy-dev")
        );

        // 4. First non-offline device even if restricted
        snap.devices.retain(|d| d.id != "healthy-dev");
        assert_eq!(
            snap.resolve_target_device_id(None).as_deref(),
            Some("restricted-dev")
        );

        // 5. Only offline devices -> None
        snap.devices.retain(|d| d.id != "restricted-dev");
        assert_eq!(snap.resolve_target_device_id(None), None);
    }

    #[test]
    fn build_play_command_json_variants() {
        // Context + track + position + initially_paused
        let with_ctx = build_play_command_json(
            &PlayRequest {
                track_uri: Some("spotify:track:111".to_string()),
                context_uri: Some("spotify:playlist:222".to_string()),
                position_ms: Some(45_000),
                initially_paused: Some(true),
                ..Default::default()
            },
            1_700_000_000_000,
        )
        .unwrap();
        assert_eq!(with_ctx["command"]["endpoint"], "play");
        assert_eq!(with_ctx["command"]["context"]["uri"], "spotify:playlist:222");
        assert_eq!(
            with_ctx["command"]["context"]["url"],
            "context://spotify:playlist:222"
        );
        assert_eq!(
            with_ctx["command"]["options"]["skip_to"]["track_uri"],
            "spotify:track:111"
        );
        assert_eq!(with_ctx["command"]["options"]["seek_to"], 45_000);
        assert_eq!(with_ctx["command"]["options"]["initially_paused"], true);
        assert_eq!(
            with_ctx["command"]["play_origin"]["feature_identifier"],
            "harmony"
        );
        assert_eq!(
            with_ctx["command"]["logging_params"]["command_initiated_time"],
            1_700_000_000_000i64
        );

        // Context + skip_to_index
        let with_idx = build_play_command_json(
            &PlayRequest {
                context_uri: Some("spotify:album:333".to_string()),
                skip_to_index: Some(4),
                ..Default::default()
            },
            1234,
        )
        .unwrap();
        assert_eq!(with_idx["command"]["options"]["skip_to"]["track_index"], 4);

        // Standalone track (no context)
        let standalone = build_play_command_json(
            &PlayRequest {
                track_uri: Some("spotify:track:999".to_string()),
                position_ms: Some(-50), // clamps to 0
                ..Default::default()
            },
            5678,
        )
        .unwrap();
        assert_eq!(standalone["command"]["endpoint"], "play");
        assert_eq!(standalone["command"]["context"]["uri"], "spotify:track:999");
        assert_eq!(
            standalone["command"]["context"]["pages"][0]["tracks"][0]["uri"],
            "spotify:track:999"
        );
        assert_eq!(
            standalone["command"]["options"]["skip_to"]["track_uri"],
            "spotify:track:999"
        );
        assert_eq!(standalone["command"]["options"]["seek_to"], 0);

        // Empty request -> resume
        let empty = build_play_command_json(&PlayRequest::default(), 9999).unwrap();
        assert_eq!(empty["command"]["endpoint"], "resume");
        assert_eq!(
            empty["command"]["logging_params"]["command_initiated_time"],
            9999
        );

        // Invalid URI rejected
        let err = build_play_command_json(
            &PlayRequest {
                track_uri: Some("https://example.com".to_string()),
                ..Default::default()
            },
            100,
        )
        .unwrap_err();
        assert_eq!(err.code, PlayerErrorCode::InvalidInput);
    }

    #[test]
    fn build_control_command_json_payloads() {
        let pause_cmd = build_simple_command_json("pause", 111);
        assert_eq!(pause_cmd["command"]["endpoint"], "pause");
        assert_eq!(
            pause_cmd["command"]["logging_params"]["command_initiated_time"],
            111
        );

        let seek_cmd = build_seek_command_json(12_345, 222);
        assert_eq!(seek_cmd["command"]["endpoint"], "seek_to");
        assert_eq!(seek_cmd["command"]["value"], 12_345);
        assert_eq!(seek_cmd["command"]["position"], 12_345);

        let shuffle_cmd = build_shuffle_command_json(true, 333);
        assert_eq!(shuffle_cmd["command"]["endpoint"], "set_shuffling_context");
        assert_eq!(shuffle_cmd["command"]["value"], true);

        let repeat_off = build_repeat_command_json(RepeatMode::Off, 444);
        assert_eq!(repeat_off["command"]["endpoint"], "set_options");
        assert_eq!(repeat_off["command"]["repeating_context"], false);
        assert_eq!(repeat_off["command"]["repeating_track"], false);

        let repeat_ctx = build_repeat_command_json(RepeatMode::Context, 445);
        assert_eq!(repeat_ctx["command"]["repeating_context"], true);
        assert_eq!(repeat_ctx["command"]["repeating_track"], false);

        let repeat_track = build_repeat_command_json(RepeatMode::Track, 446);
        assert_eq!(repeat_track["command"]["repeating_context"], false);
        assert_eq!(repeat_track["command"]["repeating_track"], true);

        let queue_cmd = build_add_to_queue_command_json("spotify:track:abc", 555);
        assert_eq!(queue_cmd["command"]["endpoint"], "add_to_queue");
        assert_eq!(queue_cmd["command"]["track"]["uri"], "spotify:track:abc");
        assert_eq!(queue_cmd["command"]["track"]["provider"], "queue");
    }

    #[test]
    fn player_error_codes_and_display() {
        assert_eq!(PlayerErrorCode::NoActiveDevice.as_str(), "no_active_device");
        assert_eq!(PlayerErrorCode::Unauthenticated.as_str(), "unauthenticated");
        assert_eq!(PlayerErrorCode::Offline.as_str(), "offline");
        assert_eq!(PlayerErrorCode::InvalidInput.as_str(), "invalid_input");
        assert_eq!(PlayerErrorCode::CommandFailed.as_str(), "command_failed");

        let err = PlayerError::no_active_device("no device");
        assert_eq!(err.to_string(), "no_active_device: no device");
    }
}
