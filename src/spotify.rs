use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use chrono::DateTime;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};
use http::{Method, Request};
use librespot_core::{
    SpotifyId, SpotifyUri,
    authentication::Credentials,
    cache::Cache,
    config::SessionConfig,
    session::Session,
    spclient::CLIENT_TOKEN,
};
use librespot_oauth::OAuthClientBuilder;
use librespot_protocol::{
    extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery},
    extension_kind::ExtensionKind,
    metadata::{
        Date as ProtoDate, Episode as ProtoEpisode, Image as ProtoImage, Track as ProtoTrack,
        album::Type as ProtoAlbumType, image::Size as ProtoImageSize,
    },
    playlist4_external::SelectedListContent,
};
use protobuf::{EnumOrUnknown, Message};
use serde::{Deserialize, Serialize};

const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const LISTENING_ACTIVITY_ENDPOINT: &str = "/listening-activity/v1/feed";
const BUDDYLIST_ENDPOINT: &str = "/presence-view/v1/buddylist";
const LISTENING_ACTIVITY_LIMIT: u32 = 100;

fn default_item_type() -> String {
    "track".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuddyListResponse {
    #[serde(default)]
    pub friends: Vec<FriendActivity>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FriendActivity {
    pub timestamp: i64,
    #[serde(default)]
    pub playing: bool,
    pub user: FriendUser,
    pub track: FriendTrack,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FriendUser {
    pub uri: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "imageUrl", default)]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FriendTrack {
    pub uri: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "imageUrl", default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub album: Option<NamedUri>,
    #[serde(default)]
    pub artist: Option<NamedUri>,
    #[serde(default)]
    pub context: Option<TrackContext>,
    #[serde(default = "default_item_type")]
    pub item_type: String,
    #[serde(default)]
    pub artists: Vec<NamedUri>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub popularity: Option<i32>,
    #[serde(default)]
    pub explicit: Option<bool>,
    #[serde(default)]
    pub track_number: Option<i32>,
    #[serde(default)]
    pub disc_number: Option<i32>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub album_type: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub album_label: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
}

impl Default for FriendTrack {
    fn default() -> Self {
        Self {
            uri: String::new(),
            name: String::new(),
            image_url: None,
            album: None,
            artist: None,
            context: None,
            item_type: default_item_type(),
            artists: Vec::new(),
            duration_ms: None,
            popularity: None,
            explicit: None,
            track_number: None,
            disc_number: None,
            isrc: None,
            album_type: None,
            release_date: None,
            album_label: None,
            cover_url: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NamedUri {
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TrackContext {
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub index: Option<i64>,
    #[serde(default)]
    pub context_type: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub is_private: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PlaylistInfo {
    pub name: String,
    pub owner: Option<String>,
    pub is_private: bool,
    pub total_tracks: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct UserProfileSnapshot {
    pub user_uri: String,
    pub user_name: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub color: Option<i64>,
    #[serde(default)]
    pub followers_count: Option<i64>,
    #[serde(default)]
    pub following_count: Option<i64>,
    #[serde(default)]
    pub public_playlists: Vec<PublicPlaylist>,
    #[serde(default)]
    pub recently_played_artists: Vec<NamedUri>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct PublicPlaylist {
    pub uri: String,
    pub name: String,
    #[serde(default)]
    pub owner_name: Option<String>,
    #[serde(default)]
    pub owner_uri: Option<String>,
    #[serde(default)]
    pub followers_count: Option<i64>,
    #[serde(default)]
    pub image_url: Option<String>,
}

/// Cached display names and metadata for users and playlists so polling only fetches item metadata.
#[derive(Default)]
pub struct NameCache {
    profiles: HashMap<String, (Option<String>, Option<String>)>,
    playlists: HashMap<String, PlaylistInfo>,
}

#[allow(dead_code)]
impl FriendActivity {
    pub fn artist_name(&self) -> &str {
        self.track
            .artist
            .as_ref()
            .map(|a| a.name.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown Artist")
    }

    pub fn all_artists_display(&self) -> String {
        let names: Vec<&str> = self
            .track
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        if !names.is_empty() {
            names.join(", ")
        } else {
            self.artist_name().to_string()
        }
    }

    pub fn artist_uri(&self) -> Option<&str> {
        self.track
            .artist
            .as_ref()
            .map(|a| a.uri.as_str())
            .filter(|s| !s.is_empty())
    }

    pub fn album_name(&self) -> Option<&str> {
        self.track
            .album
            .as_ref()
            .map(|a| a.name.as_str())
            .filter(|s| !s.is_empty())
    }

    pub fn album_uri(&self) -> Option<&str> {
        self.track
            .album
            .as_ref()
            .map(|a| a.uri.as_str())
            .filter(|s| !s.is_empty())
    }

    pub fn context_name(&self) -> Option<&str> {
        self.track
            .context
            .as_ref()
            .map(|c| c.name.as_str())
            .filter(|s| !s.is_empty())
    }

    pub fn context_uri(&self) -> Option<&str> {
        self.track
            .context
            .as_ref()
            .map(|c| c.uri.as_str())
            .filter(|s| !s.is_empty())
    }

    pub fn duration_ms(&self) -> Option<i64> {
        self.track.duration_ms
    }

    pub fn is_private_context(&self) -> bool {
        self.track.context.as_ref().is_some_and(|c| c.is_private)
    }

    pub fn context_type(&self) -> Option<&str> {
        self.track.context.as_ref().map(|c| {
            if !c.context_type.is_empty() {
                c.context_type.as_str()
            } else {
                classify_context_type(&c.uri, &c.name)
            }
        })
    }

    pub fn context_owner(&self) -> Option<&str> {
        self.track
            .context
            .as_ref()
            .and_then(|c| c.owner.as_deref())
            .filter(|s| !s.is_empty())
    }

    pub fn item_type(&self) -> &str {
        if self.track.item_type.is_empty() {
            "track"
        } else {
            self.track.item_type.as_str()
        }
    }

    pub fn matches_filter(&self, filters: &[String]) -> bool {
        if filters.is_empty() {
            return true;
        }
        let user_uri_lc = self.user.uri.to_lowercase();
        let user_name_lc = self.user.name.to_lowercase();
        filters.iter().any(|f| {
            let needle = f.trim().to_lowercase();
            !needle.is_empty()
                && (user_uri_lc == needle
                    || user_uri_lc.ends_with(&format!(":{needle}"))
                    || user_name_lc.contains(&needle)
                    || user_uri_lc.contains(&needle))
        })
    }
}

pub fn classify_context_type(uri: &str, name: &str) -> &'static str {
    if uri.contains(":collection") || name == "Liked Songs" {
        "liked_songs"
    } else if uri.contains(":playlist:") {
        "playlist"
    } else if uri.contains(":album:") {
        "album"
    } else if uri.contains(":artist:") {
        "artist"
    } else if uri.contains(":track:") {
        "track"
    } else if uri.contains(":show:") {
        "show"
    } else {
        "other"
    }
}

pub fn format_proto_date(date: &ProtoDate) -> Option<String> {
    let year = date.year.filter(|&y| y > 0)?;
    match date.month.filter(|m| (1..=12).contains(m)) {
        Some(month) => match date.day.filter(|d| (1..=31).contains(d)) {
            Some(day) => Some(format!("{year:04}-{month:02}-{day:02}")),
            None => Some(format!("{year:04}-{month:02}")),
        },
        None => Some(format!("{year:04}")),
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn extract_cover_url(images: &[ProtoImage]) -> Option<String> {
    let pick = images
        .iter()
        .find(|img| {
            img.size.and_then(|s| s.enum_value().ok()) == Some(ProtoImageSize::LARGE)
                && !img.file_id().is_empty()
        })
        .or_else(|| {
            images.iter().find(|img| {
                img.size.and_then(|s| s.enum_value().ok()) == Some(ProtoImageSize::DEFAULT)
                    && !img.file_id().is_empty()
            })
        })
        .or_else(|| images.iter().find(|img| !img.file_id().is_empty()))?;

    Some(format!(
        "https://i.scdn.co/image/{}",
        bytes_to_hex(pick.file_id())
    ))
}

fn format_album_type(album_type: Option<EnumOrUnknown<ProtoAlbumType>>) -> Option<String> {
    match album_type?.enum_value().ok()? {
        ProtoAlbumType::ALBUM => Some("ALBUM".to_string()),
        ProtoAlbumType::SINGLE => Some("SINGLE".to_string()),
        ProtoAlbumType::EP => Some("EP".to_string()),
        ProtoAlbumType::COMPILATION => Some("COMPILATION".to_string()),
        ProtoAlbumType::AUDIOBOOK => Some("AUDIOBOOK".to_string()),
        ProtoAlbumType::PODCAST => Some("PODCAST".to_string()),
    }
}

pub fn open_cache(cache_dir: &Path) -> Result<Cache> {
    Cache::new(Some(cache_dir), None, None, None)
        .map_err(|e| anyhow!("Failed to open credentials cache at {}: {e}", cache_dir.display()))
}

pub async fn login_oauth(
    cache_dir: &Path,
    open_browser: bool,
    redirect_uri: Option<&str>,
) -> Result<Session> {
    let config = SessionConfig::default();
    let cache = open_cache(cache_dir)?;

    let redirect = redirect_uri.unwrap_or(DEFAULT_REDIRECT_URI);
    let mut builder = OAuthClientBuilder::new(&config.client_id, redirect, vec!["streaming"])
        .with_custom_message(
            "Authorization successful! You can close this tab and return to the terminal.",
        );
    if open_browser {
        builder = builder.open_in_browser();
    }

    let oauth_client = builder
        .build()
        .map_err(|e| anyhow!("Failed to initialize OAuth client: {e}"))?;

    let token = oauth_client
        .get_access_token_async()
        .await
        .map_err(|e| anyhow!("OAuth login failed: {e}"))?;

    let credentials = Credentials::with_access_token(token.access_token);
    let session = Session::new(config, Some(cache));
    session
        .connect(credentials, true)
        .await
        .map_err(|e| anyhow!("Failed to connect Spotify session: {e}"))?;

    Ok(session)
}

pub async fn connect_cached(cache_dir: &Path) -> Result<Session> {
    let config = SessionConfig::default();
    let cache = open_cache(cache_dir)?;
    let credentials = cache.credentials().ok_or_else(|| {
        anyhow!(
            "No cached Spotify credentials found in {}. Run `spotify-friend-logger login` first.",
            cache_dir.display()
        )
    })?;

    let session = Session::new(config, Some(cache));
    session
        .connect(credentials, true)
        .await
        .map_err(|e| anyhow!("Failed to connect Spotify session using cached credentials: {e}"))?;

    Ok(session)
}

async fn post_listening_activity(session: &Session, limit: u32) -> Result<Bytes> {
    let base_url = session
        .spclient()
        .base_url()
        .await
        .map_err(|e| anyhow!("Failed to resolve spclient access point: {e}"))?;
    let url = format!("{base_url}{LISTENING_ACTIVITY_ENDPOINT}");
    let body = serde_json::json!({ "unused": true, "resultLimit": limit }).to_string();
    let body_bytes = Bytes::from(body);

    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|e| anyhow!("Failed to obtain login5 auth token: {e}"))?;

    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(&url)
        .header(ACCEPT, HeaderValue::from_static("application/json"))
        .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .header(CONTENT_LENGTH, body_bytes.len())
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("{} {}", token.token_type, token.access_token))?,
        );

    if let Ok(client_token) = session.spclient().client_token().await
        && let Ok(val) = HeaderValue::from_str(&client_token)
    {
        builder = builder.header(CLIENT_TOKEN, val);
    }

    let request = builder.body(body_bytes)?;
    session
        .http_client()
        .request_body(request)
        .await
        .map_err(|e| anyhow!("Failed to fetch listening-activity feed: {e}"))
}

pub async fn fetch_buddylist_raw(session: &Session) -> Result<String> {
    let raw = post_listening_activity(session, LISTENING_ACTIVITY_LIMIT).await?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

fn json_str(val: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = val.get(*key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    None
}

fn json_i64(val: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(n) = val.get(*key).and_then(|v| v.as_i64()) {
            return Some(n);
        }
    }
    None
}

fn json_array<'a>(val: &'a serde_json::Value, keys: &[&str]) -> Option<&'a [serde_json::Value]> {
    for key in keys {
        if let Some(arr) = val.get(*key).and_then(|v| v.as_array()) {
            return Some(arr.as_slice());
        }
    }
    None
}

fn parse_user_profile(user_uri: &str, json: &serde_json::Value) -> UserProfileSnapshot {
    let uid = user_id_from_uri(user_uri);
    let canonical_uri = json_str(json, &["uri"]).unwrap_or_else(|| {
        if user_uri.starts_with("spotify:user:") {
            user_uri.to_string()
        } else {
            format!("spotify:user:{uid}")
        }
    });
    let user_name = json_str(json, &["name", "display_name", "displayName"])
        .unwrap_or_else(|| uid.clone());
    let image_url = json_str(json, &["image_url", "imageUrl"]);
    let color = json_i64(json, &["color"]);
    let followers_count = json_i64(json, &["followers_count", "followersCount"]);
    let following_count = json_i64(json, &["following_count", "followingCount"]);

    let public_playlists = json_array(json, &["public_playlists", "publicPlaylists"])
        .unwrap_or(&[])
        .iter()
        .filter_map(|item| {
            let uri = json_str(item, &["uri"]).unwrap_or_default();
            let name = json_str(item, &["name"]).unwrap_or_default();
            if uri.is_empty() && name.is_empty() {
                return None;
            }
            Some(PublicPlaylist {
                uri,
                name,
                owner_name: json_str(item, &["owner_name", "ownerName"]),
                owner_uri: json_str(item, &["owner_uri", "ownerUri"]),
                followers_count: json_i64(item, &["followers_count", "followersCount"]),
                image_url: json_str(item, &["image_url", "imageUrl"]),
            })
        })
        .collect();

    let recently_played_artists =
        json_array(json, &["recently_played_artists", "recentlyPlayedArtists"])
            .unwrap_or(&[])
            .iter()
            .filter_map(|item| {
                let uri = json_str(item, &["uri"]).unwrap_or_default();
                let name = json_str(item, &["name"]).unwrap_or_default();
                if uri.is_empty() && name.is_empty() {
                    return None;
                }
                Some(NamedUri { uri, name })
            })
            .collect();

    UserProfileSnapshot {
        user_uri: canonical_uri,
        user_name,
        image_url,
        color,
        followers_count,
        following_count,
        public_playlists,
        recently_played_artists,
    }
}

#[allow(dead_code)]
pub async fn fetch_user_profile(session: &Session, user_uri: &str) -> Result<UserProfileSnapshot> {
    let uid = user_id_from_uri(user_uri);
    if uid.is_empty() {
        return Err(anyhow!("Invalid user URI: {user_uri}"));
    }

    let bytes = session
        .spclient()
        .get_user_profile(&uid, Some(50), Some(20))
        .await
        .map_err(|e| anyhow!("Failed to fetch user profile for {user_uri}: {e}"))?;

    let json: serde_json::Value = serde_json::from_slice(&bytes).with_context(|| {
        let preview = String::from_utf8_lossy(&bytes);
        let snippet: String = preview.chars().take(300).collect();
        format!("Failed to parse user profile JSON for {user_uri} (preview: {snippet})")
    })?;

    Ok(parse_user_profile(user_uri, &json))
}

#[derive(Debug, Clone)]
struct LiveEntry {
    user_uri: String,
    track_uri: String,
    context_uri: Option<String>,
    playing: bool,
    timestamp_ms: i64,
}

fn user_id_from_uri(uri: &str) -> String {
    uri.rsplit(':').next().unwrap_or_default().to_string()
}

fn playlist_id_from_uri(uri: &str) -> Option<String> {
    uri.contains(":playlist:")
        .then(|| uri.rsplit(':').next().unwrap_or_default().to_string())
        .filter(|s| !s.is_empty())
}

fn normalize_context_uri(uri: &str) -> String {
    uri.strip_prefix("spotify:list:popular-release-segments-main-roles:artist_")
        .map(|id| format!("spotify:artist:{id}"))
        .unwrap_or_else(|| uri.to_string())
}

fn parse_live_feed(json: &serde_json::Value) -> Result<Vec<LiveEntry>> {
    let entities = json
        .get("entities")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Listening activity response missing `entities` array"))?;

    let mut latest: HashMap<String, LiveEntry> = HashMap::new();
    for item in entities {
        let Some(entity) = item.get("userEntity").or_else(|| item.get("followEntity")) else {
            continue;
        };
        let Some(user_uri) = entity
            .get("uri")
            .and_then(|v| v.as_str())
            .filter(|s| s.starts_with("spotify:user:"))
        else {
            continue;
        };
        let Some(activity) = entity.get("activity") else {
            continue;
        };
        let Some(track_uri) = activity
            .get("entityUri")
            .and_then(|v| v.as_str())
            .filter(|s| s.starts_with("spotify:track:") || s.starts_with("spotify:episode:"))
        else {
            continue;
        };
        let Some(ts_str) = activity.get("timestamp").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) else {
            continue;
        };
        let timestamp_ms = dt.timestamp_millis();

        let context_uri = activity
            .get("contextUri")
            .and_then(|v| v.as_str())
            .map(normalize_context_uri)
            .filter(|s| !s.is_empty());

        let playing = activity
            .get("isPlaying")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let entry = LiveEntry {
            user_uri: user_uri.to_string(),
            track_uri: track_uri.to_string(),
            context_uri,
            playing,
            timestamp_ms,
        };

        if latest
            .get(user_uri)
            .is_none_or(|prev| prev.timestamp_ms < timestamp_ms)
        {
            latest.insert(user_uri.to_string(), entry);
        }
    }

    let mut entries: Vec<LiveEntry> = latest.into_values().collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp_ms));
    Ok(entries)
}

enum ItemMetadata {
    Track(ProtoTrack),
    Episode(ProtoEpisode),
}

async fn fetch_items_metadata(
    session: &Session,
    entity_uris: &[String],
) -> Result<HashMap<String, ItemMetadata>> {
    if entity_uris.is_empty() {
        return Ok(HashMap::new());
    }

    let unique: BTreeSet<&String> = entity_uris.iter().collect();
    let entity_request: Vec<EntityRequest> = unique
        .into_iter()
        .map(|uri| {
            let kind = if uri.starts_with("spotify:episode:") {
                ExtensionKind::EPISODE_V4
            } else {
                ExtensionKind::TRACK_V4
            };
            EntityRequest {
                entity_uri: uri.clone(),
                query: vec![ExtensionQuery {
                    extension_kind: EnumOrUnknown::new(kind),
                    ..Default::default()
                }],
                ..Default::default()
            }
        })
        .collect();

    let req = BatchedEntityRequest {
        entity_request,
        ..Default::default()
    };

    let resp = session
        .spclient()
        .get_extended_metadata(req)
        .await
        .map_err(|e| anyhow!("Failed to fetch batched item metadata: {e}"))?;

    let mut items = HashMap::new();
    for array in resp.extended_metadata {
        match array.extension_kind.enum_value() {
            Ok(ExtensionKind::TRACK_V4) => {
                for data in array.extension_data {
                    if !matches!(data.header.status_code, 0 | 200) {
                        continue;
                    }
                    if let Some(any) = data.extension_data.as_ref()
                        && let Ok(track) = ProtoTrack::parse_from_bytes(&any.value)
                    {
                        items.insert(data.entity_uri.clone(), ItemMetadata::Track(track));
                    }
                }
            }
            Ok(ExtensionKind::EPISODE_V4) => {
                for data in array.extension_data {
                    if !matches!(data.header.status_code, 0 | 200) {
                        continue;
                    }
                    if let Some(any) = data.extension_data.as_ref()
                        && let Ok(episode) = ProtoEpisode::parse_from_bytes(&any.value)
                    {
                        items.insert(data.entity_uri.clone(), ItemMetadata::Episode(episode));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(items)
}

async fn resolve_missing_names(
    session: &Session,
    entries: &[LiveEntry],
    cache: &mut NameCache,
) {
    let missing_users: BTreeSet<String> = entries
        .iter()
        .map(|e| user_id_from_uri(&e.user_uri))
        .filter(|id| !id.is_empty() && !cache.profiles.contains_key(id))
        .collect();

    for uid in missing_users {
        if let Ok(bytes) = session
            .spclient()
            .get_user_profile(&uid, Some(0), Some(0))
            .await
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let name = json_str(&json, &["name", "display_name", "displayName"]);
            let image_url = json_str(&json, &["image_url", "imageUrl"]);
            cache.profiles.insert(uid, (name, image_url));
        }
    }

    let missing_playlists: BTreeSet<String> = entries
        .iter()
        .filter_map(|e| e.context_uri.as_deref().and_then(playlist_id_from_uri))
        .filter(|id| !cache.playlists.contains_key(id))
        .collect();

    for pid in missing_playlists {
        let info = match SpotifyId::from_base62(&pid) {
            Ok(spotify_id) => match session.spclient().get_playlist(&spotify_id).await {
                Ok(bytes) => match SelectedListContent::parse_from_bytes(&bytes) {
                    Ok(list) => PlaylistInfo {
                        name: list.attributes.name().to_string(),
                        owner: list
                            .owner_username
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .map(str::to_string),
                        is_private: false,
                        total_tracks: list.length,
                    },
                    Err(_) => PlaylistInfo {
                        name: String::new(),
                        owner: None,
                        is_private: true,
                        total_tracks: None,
                    },
                },
                Err(_) => PlaylistInfo {
                    name: String::new(),
                    owner: None,
                    is_private: true,
                    total_tracks: None,
                },
            },
            Err(_) => PlaylistInfo {
                name: String::new(),
                owner: None,
                is_private: true,
                total_tracks: None,
            },
        };
        cache.playlists.insert(pid, info);
    }
}

async fn fetch_legacy_buddylist(session: &Session) -> Result<Vec<FriendActivity>> {
    let raw = session
        .spclient()
        .request_as_json(&Method::GET, BUDDYLIST_ENDPOINT, None, None)
        .await
        .map_err(|e| anyhow!("Failed to fetch legacy buddylist from spclient: {e}"))?;

    let mut parsed: BuddyListResponse = serde_json::from_slice(&raw).with_context(|| {
        let preview = String::from_utf8_lossy(&raw);
        let snippet: String = preview.chars().take(300).collect();
        format!("Failed to parse buddylist JSON (preview: {snippet})")
    })?;

    for item in &mut parsed.friends {
        if item.track.artists.is_empty()
            && let Some(artist) = item.track.artist.clone()
        {
            item.track.artists.push(artist);
        }
        if item.track.cover_url.is_none() {
            item.track.cover_url = item.track.image_url.clone();
        }
        if let Some(ctx) = item.track.context.as_mut() {
            if ctx.context_type.is_empty() {
                ctx.context_type = classify_context_type(&ctx.uri, &ctx.name).to_string();
            }
            if ctx.context_type == "liked_songs" {
                ctx.is_private = true;
            }
        }
    }

    Ok(parsed.friends)
}

fn build_track_from_proto(uri: &str, t: &ProtoTrack) -> FriendTrack {
    let name = if !t.name().is_empty() {
        t.name().to_string()
    } else {
        uri.to_string()
    };

    let artists: Vec<NamedUri> = t
        .artist
        .iter()
        .map(|a| {
            let artist_uri = SpotifyId::from_raw(a.gid())
                .ok()
                .and_then(|id| SpotifyUri::Artist { id }.to_uri().ok())
                .unwrap_or_default();
            NamedUri {
                uri: artist_uri,
                name: a.name().to_string(),
            }
        })
        .collect();
    let artist = artists.first().cloned();

    let album_proto = t.album.as_ref();
    let album = album_proto.map(|al| {
        let album_uri = SpotifyId::from_raw(al.gid())
            .ok()
            .and_then(|id| SpotifyUri::Album { id }.to_uri().ok())
            .unwrap_or_default();
        NamedUri {
            uri: album_uri,
            name: al.name().to_string(),
        }
    });

    let isrc = t
        .external_id
        .iter()
        .find(|ext| ext.type_().eq_ignore_ascii_case("isrc"))
        .map(|ext| ext.id().to_string())
        .filter(|s| !s.is_empty());

    let album_type = album_proto.and_then(|al| {
        format_album_type(al.type_).or_else(|| {
            al.type_str
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_uppercase())
        })
    });

    let release_date = album_proto.and_then(|al| al.date.as_ref()).and_then(format_proto_date);

    let album_label = album_proto
        .and_then(|al| al.label.as_deref())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let cover_url = album_proto.and_then(|al| {
        al.cover_group
            .as_ref()
            .and_then(|g| extract_cover_url(&g.image))
            .or_else(|| extract_cover_url(&al.cover))
    });

    FriendTrack {
        uri: uri.to_string(),
        name,
        image_url: cover_url.clone(),
        album,
        artist,
        context: None,
        item_type: "track".to_string(),
        artists,
        duration_ms: t.duration.map(i64::from),
        popularity: t.popularity,
        explicit: t.explicit,
        track_number: t.number,
        disc_number: t.disc_number,
        isrc,
        album_type,
        release_date,
        album_label,
        cover_url,
    }
}

fn build_episode_from_proto(uri: &str, ep: &ProtoEpisode) -> FriendTrack {
    let name = if !ep.name().is_empty() {
        ep.name().to_string()
    } else {
        uri.to_string()
    };

    let show_proto = ep.show.as_ref();
    let show_named = show_proto.map(|show| {
        let show_uri = SpotifyId::from_raw(show.gid())
            .ok()
            .and_then(|id| SpotifyUri::Show { id }.to_uri().ok())
            .unwrap_or_default();
        NamedUri {
            uri: show_uri,
            name: show.name().to_string(),
        }
    });

    let artists = show_named.clone().into_iter().collect();
    let release_date = ep.publish_time.as_ref().and_then(format_proto_date);
    let cover_url = ep
        .cover_image
        .as_ref()
        .and_then(|g| extract_cover_url(&g.image))
        .or_else(|| {
            show_proto
                .and_then(|s| s.cover_image.as_ref())
                .and_then(|g| extract_cover_url(&g.image))
        });
    let album_type = Some(
        if show_proto.is_some_and(|s| s.is_audiobook()) || ep.is_audiobook_chapter() {
            "AUDIOBOOK".to_string()
        } else {
            "PODCAST".to_string()
        },
    );
    let album_label = show_proto
        .and_then(|s| s.publisher.as_deref())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    FriendTrack {
        uri: uri.to_string(),
        name,
        image_url: cover_url.clone(),
        album: show_named.clone(),
        artist: show_named,
        context: None,
        item_type: "episode".to_string(),
        artists,
        duration_ms: ep.duration.map(i64::from),
        popularity: None,
        explicit: ep.explicit,
        track_number: ep.number,
        disc_number: None,
        isrc: None,
        album_type,
        release_date,
        album_label,
        cover_url,
    }
}

pub async fn fetch_buddylist(
    session: &Session,
    name_cache: &mut NameCache,
) -> Result<Vec<FriendActivity>> {
    let live_entries = match post_listening_activity(session, LISTENING_ACTIVITY_LIMIT).await {
        Ok(raw) => {
            let json: serde_json::Value = serde_json::from_slice(&raw).with_context(|| {
                let preview = String::from_utf8_lossy(&raw);
                let snippet: String = preview.chars().take(300).collect();
                format!("Failed to parse listening-activity JSON (preview: {snippet})")
            })?;
            parse_live_feed(&json)?
        }
        Err(_) => {
            return fetch_legacy_buddylist(session).await;
        }
    };

    if live_entries.is_empty() {
        return Ok(Vec::new());
    }

    let entity_uris: Vec<String> = live_entries.iter().map(|e| e.track_uri.clone()).collect();
    let items = fetch_items_metadata(session, &entity_uris).await?;
    resolve_missing_names(session, &live_entries, name_cache).await;

    let mut result = Vec::with_capacity(live_entries.len());
    for entry in live_entries {
        let uid = user_id_from_uri(&entry.user_uri);
        let (user_name, user_image) = name_cache
            .profiles
            .get(&uid)
            .cloned()
            .unwrap_or((None, None));

        let mut track = match items.get(&entry.track_uri) {
            Some(ItemMetadata::Track(t)) => build_track_from_proto(&entry.track_uri, t),
            Some(ItemMetadata::Episode(ep)) => build_episode_from_proto(&entry.track_uri, ep),
            None => FriendTrack {
                uri: entry.track_uri.clone(),
                name: entry.track_uri.clone(),
                item_type: if entry.track_uri.starts_with("spotify:episode:") {
                    "episode".to_string()
                } else {
                    "track".to_string()
                },
                ..Default::default()
            },
        };

        let context = entry.context_uri.as_ref().map(|ctx_uri| {
            if ctx_uri.contains(":collection") {
                TrackContext {
                    uri: ctx_uri.clone(),
                    name: "Liked Songs".to_string(),
                    index: None,
                    context_type: "liked_songs".to_string(),
                    owner: None,
                    is_private: true,
                }
            } else if let Some(pid) = playlist_id_from_uri(ctx_uri) {
                let (name, owner, is_private) = match name_cache.playlists.get(&pid) {
                    Some(info) => (info.name.clone(), info.owner.clone(), info.is_private),
                    None => (String::new(), None, true),
                };
                TrackContext {
                    uri: ctx_uri.clone(),
                    name,
                    index: None,
                    context_type: "playlist".to_string(),
                    owner,
                    is_private,
                }
            } else {
                let ctx_name = if track.album.as_ref().is_some_and(|a| &a.uri == ctx_uri) {
                    track.album.as_ref().map(|a| a.name.clone()).unwrap_or_default()
                } else if let Some(a) = track.artists.iter().find(|a| &a.uri == ctx_uri) {
                    a.name.clone()
                } else if track.artist.as_ref().is_some_and(|a| &a.uri == ctx_uri) {
                    track.artist.as_ref().map(|a| a.name.clone()).unwrap_or_default()
                } else if ctx_uri == &entry.track_uri {
                    track.name.clone()
                } else {
                    String::new()
                };
                let context_type = classify_context_type(ctx_uri, &ctx_name).to_string();
                let is_private = context_type == "liked_songs";
                TrackContext {
                    uri: ctx_uri.clone(),
                    name: ctx_name,
                    index: None,
                    context_type,
                    owner: None,
                    is_private,
                }
            }
        });
        track.context = context;

        result.push(FriendActivity {
            timestamp: entry.timestamp_ms,
            playing: entry.playing,
            user: FriendUser {
                uri: entry.user_uri,
                name: user_name.unwrap_or(uid),
                image_url: user_image,
            },
            track,
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_context_types() {
        assert_eq!(
            classify_context_type("spotify:user:alice:collection", ""),
            "liked_songs"
        );
        assert_eq!(
            classify_context_type("spotify:something:else", "Liked Songs"),
            "liked_songs"
        );
        assert_eq!(
            classify_context_type("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M", "Today's Top Hits"),
            "playlist"
        );
        assert_eq!(
            classify_context_type("spotify:album:4aawyAB9vmqN3uQ7FjRGTy", "Global Warming"),
            "album"
        );
        assert_eq!(
            classify_context_type("spotify:artist:0OdUWJ0sBjDrqHygGUXeCF", "Band of Horses"),
            "artist"
        );
        assert_eq!(
            classify_context_type("spotify:track:6rqhFgbbKwnb9MLmUQDhG6", "Song"),
            "track"
        );
        assert_eq!(
            classify_context_type("spotify:show:4rOoJ6Egrf8K2IrywzwOMk", "Podcast"),
            "show"
        );
        assert_eq!(classify_context_type("spotify:station:abc", "Radio"), "other");
    }

    #[test]
    fn format_proto_dates() {
        let full = ProtoDate {
            year: Some(2024),
            month: Some(3),
            day: Some(9),
            ..Default::default()
        };
        assert_eq!(format_proto_date(&full).as_deref(), Some("2024-03-09"));

        let year_month = ProtoDate {
            year: Some(2024),
            month: Some(11),
            day: None,
            ..Default::default()
        };
        assert_eq!(format_proto_date(&year_month).as_deref(), Some("2024-11"));

        let year_only = ProtoDate {
            year: Some(1999),
            month: None,
            day: None,
            ..Default::default()
        };
        assert_eq!(format_proto_date(&year_only).as_deref(), Some("1999"));

        let missing_year = ProtoDate {
            year: None,
            month: Some(5),
            day: Some(10),
            ..Default::default()
        };
        assert_eq!(format_proto_date(&missing_year), None);
    }

    #[test]
    fn parse_live_feed_tracks_episodes_and_contexts() {
        let json = serde_json::json!({
            "entities": [
                {
                    "userEntity": {
                        "uri": "spotify:user:alice",
                        "activity": {
                            "entityUri": "spotify:track:1111111111111111111111",
                            "contextUri": "spotify:user:alice:collection",
                            "timestamp": "2026-11-24T20:00:00Z",
                            "isPlaying": true
                        }
                    }
                },
                {
                    "followEntity": {
                        "uri": "spotify:user:bob",
                        "activity": {
                            "entityUri": "spotify:episode:2222222222222222222222",
                            "contextUri": "spotify:show:3333333333333333333333",
                            "timestamp": "2026-11-24T20:05:00Z",
                            "isPlaying": false
                        }
                    }
                },
                {
                    "userEntity": {
                        "uri": "spotify:user:carol",
                        "activity": {
                            "entityUri": "spotify:track:4444444444444444444444",
                            "contextUri": "spotify:list:popular-release-segments-main-roles:artist_5555555555555555555555",
                            "timestamp": "2026-11-24T19:55:00Z",
                            "isPlaying": true
                        }
                    }
                },
                {
                    "userEntity": {
                        "uri": "spotify:user:ignored",
                        "activity": {
                            "entityUri": "spotify:local:artist:album:track:120",
                            "timestamp": "2026-11-24T20:10:00Z",
                            "isPlaying": true
                        }
                    }
                }
            ]
        });

        let entries = parse_live_feed(&json).unwrap();
        assert_eq!(entries.len(), 3);

        // Sorted newest first: bob (20:05), alice (20:00), carol (19:55)
        assert_eq!(entries[0].user_uri, "spotify:user:bob");
        assert_eq!(entries[0].track_uri, "spotify:episode:2222222222222222222222");
        assert_eq!(
            entries[0].context_uri.as_deref(),
            Some("spotify:show:3333333333333333333333")
        );
        assert!(!entries[0].playing);

        assert_eq!(entries[1].user_uri, "spotify:user:alice");
        assert_eq!(entries[1].track_uri, "spotify:track:1111111111111111111111");
        assert_eq!(
            entries[1].context_uri.as_deref(),
            Some("spotify:user:alice:collection")
        );
        assert!(entries[1].playing);

        assert_eq!(entries[2].user_uri, "spotify:user:carol");
        assert_eq!(
            entries[2].context_uri.as_deref(),
            Some("spotify:artist:5555555555555555555555")
        );
    }

    #[test]
    fn parse_user_profile_handles_snake_and_camel_case() {
        let json = serde_json::json!({
            "uri": "spotify:user:alice",
            "name": "Alice",
            "image_url": "https://i.scdn.co/image/abc",
            "color": 123456,
            "followersCount": 42,
            "following_count": 17,
            "public_playlists": [
                {
                    "uri": "spotify:playlist:123",
                    "name": "Late Night",
                    "ownerName": "Alice",
                    "owner_uri": "spotify:user:alice",
                    "followers_count": 5,
                    "imageUrl": "https://i.scdn.co/image/pl"
                }
            ],
            "recentlyPlayedArtists": [
                {
                    "uri": "spotify:artist:999",
                    "name": "Daft Punk"
                }
            ]
        });

        let snap = parse_user_profile("spotify:user:alice", &json);
        assert_eq!(snap.user_uri, "spotify:user:alice");
        assert_eq!(snap.user_name, "Alice");
        assert_eq!(snap.image_url.as_deref(), Some("https://i.scdn.co/image/abc"));
        assert_eq!(snap.color, Some(123456));
        assert_eq!(snap.followers_count, Some(42));
        assert_eq!(snap.following_count, Some(17));
        assert_eq!(snap.public_playlists.len(), 1);
        assert_eq!(snap.public_playlists[0].name, "Late Night");
        assert_eq!(snap.public_playlists[0].owner_name.as_deref(), Some("Alice"));
        assert_eq!(snap.public_playlists[0].followers_count, Some(5));
        assert_eq!(snap.recently_played_artists.len(), 1);
        assert_eq!(snap.recently_played_artists[0].name, "Daft Punk");
    }

    #[test]
    fn legacy_buddylist_json_deserializes_with_defaults() {
        let legacy = r#"{
            "friends": [
                {
                    "timestamp": 1700000000000,
                    "user": { "uri": "spotify:user:bob", "name": "Bob" },
                    "track": {
                        "uri": "spotify:track:abc",
                        "name": "Legacy Song",
                        "artist": { "uri": "spotify:artist:1", "name": "Artist A" },
                        "context": { "uri": "spotify:playlist:1", "name": "Mix" }
                    }
                }
            ]
        }"#;

        let resp: BuddyListResponse = serde_json::from_str(legacy).unwrap();
        assert_eq!(resp.friends.len(), 1);
        let f = &resp.friends[0];
        assert_eq!(f.item_type(), "track");
        assert_eq!(f.all_artists_display(), "Artist A");
        assert_eq!(f.context_type(), Some("playlist"));
        assert!(!f.is_private_context());
        assert_eq!(f.duration_ms(), None);
        assert_eq!(f.context_owner(), None);
    }

    #[test]
    fn extract_cover_url_prefers_large_then_default() {
        let images = vec![
            ProtoImage {
                file_id: Some(vec![0xaa, 0xbb]),
                size: Some(EnumOrUnknown::new(ProtoImageSize::SMALL)),
                ..Default::default()
            },
            ProtoImage {
                file_id: Some(vec![0x01, 0x0f, 0xcd]),
                size: Some(EnumOrUnknown::new(ProtoImageSize::LARGE)),
                ..Default::default()
            },
        ];
        assert_eq!(
            extract_cover_url(&images).as_deref(),
            Some("https://i.scdn.co/image/010fcd")
        );
    }
}
