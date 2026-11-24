use std::collections::{BTreeSet, HashMap};
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
    metadata::Track as ProtoTrack,
    playlist4_external::SelectedListContent,
};
use protobuf::{EnumOrUnknown, Message};
use serde::{Deserialize, Serialize};

const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const LISTENING_ACTIVITY_ENDPOINT: &str = "/listening-activity/v1/feed";
const BUDDYLIST_ENDPOINT: &str = "/presence-view/v1/buddylist";
const LISTENING_ACTIVITY_LIMIT: u32 = 100;

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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NamedUri {
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrackContext {
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub index: Option<i64>,
}

/// Cached display names for users and playlists so polling only fetches track metadata.
#[derive(Default)]
pub struct NameCache {
    profiles: HashMap<String, (Option<String>, Option<String>)>,
    playlists: HashMap<String, String>,
}

impl FriendActivity {
    pub fn artist_name(&self) -> &str {
        self.track
            .artist
            .as_ref()
            .map(|a| a.name.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown Artist")
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
            .filter(|s| s.starts_with("spotify:track:"))
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

async fn fetch_tracks_metadata(
    session: &Session,
    track_uris: &[String],
) -> Result<HashMap<String, ProtoTrack>> {
    if track_uris.is_empty() {
        return Ok(HashMap::new());
    }

    let unique: BTreeSet<&String> = track_uris.iter().collect();
    let entity_request: Vec<EntityRequest> = unique
        .into_iter()
        .map(|uri| EntityRequest {
            entity_uri: uri.clone(),
            query: vec![ExtensionQuery {
                extension_kind: EnumOrUnknown::new(ExtensionKind::TRACK_V4),
                ..Default::default()
            }],
            ..Default::default()
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
        .map_err(|e| anyhow!("Failed to fetch batched track metadata: {e}"))?;

    let mut tracks = HashMap::new();
    for array in resp.extended_metadata {
        if array.extension_kind.enum_value() != Ok(ExtensionKind::TRACK_V4) {
            continue;
        }
        for data in array.extension_data {
            if !matches!(data.header.status_code, 0 | 200) {
                continue;
            }
            if let Some(any) = data.extension_data.as_ref()
                && let Ok(track) = ProtoTrack::parse_from_bytes(&any.value)
            {
                tracks.insert(data.entity_uri.clone(), track);
            }
        }
    }

    Ok(tracks)
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
            let name = json
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let image_url = json
                .get("image_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            cache.profiles.insert(uid, (name, image_url));
        }
    }

    let missing_playlists: BTreeSet<String> = entries
        .iter()
        .filter_map(|e| e.context_uri.as_deref().and_then(playlist_id_from_uri))
        .filter(|id| !cache.playlists.contains_key(id))
        .collect();

    for pid in missing_playlists {
        if let Ok(spotify_id) = SpotifyId::from_base62(&pid)
            && let Ok(bytes) = session.spclient().get_playlist(&spotify_id).await
            && let Ok(list) = SelectedListContent::parse_from_bytes(&bytes)
        {
            let name = list.attributes.name().to_string();
            if !name.is_empty() {
                cache.playlists.insert(pid, name);
            }
        }
    }
}

async fn fetch_legacy_buddylist(session: &Session) -> Result<Vec<FriendActivity>> {
    let raw = session
        .spclient()
        .request_as_json(&Method::GET, BUDDYLIST_ENDPOINT, None, None)
        .await
        .map_err(|e| anyhow!("Failed to fetch legacy buddylist from spclient: {e}"))?;

    let parsed: BuddyListResponse = serde_json::from_slice(&raw).with_context(|| {
        let preview = String::from_utf8_lossy(&raw);
        let snippet: String = preview.chars().take(300).collect();
        format!("Failed to parse buddylist JSON (preview: {snippet})")
    })?;

    Ok(parsed.friends)
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

    let track_uris: Vec<String> = live_entries.iter().map(|e| e.track_uri.clone()).collect();
    let tracks = fetch_tracks_metadata(session, &track_uris).await?;
    resolve_missing_names(session, &live_entries, name_cache).await;

    let mut result = Vec::with_capacity(live_entries.len());
    for entry in live_entries {
        let uid = user_id_from_uri(&entry.user_uri);
        let (user_name, user_image) = name_cache
            .profiles
            .get(&uid)
            .cloned()
            .unwrap_or((None, None));

        let proto = tracks.get(&entry.track_uri);
        let track_name = proto
            .map(|t| t.name().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| entry.track_uri.clone());

        let artist = proto.and_then(|t| t.artist.first()).map(|a| {
            let uri = SpotifyId::from_raw(a.gid())
                .ok()
                .and_then(|id| SpotifyUri::Artist { id }.to_uri().ok())
                .unwrap_or_default();
            NamedUri {
                uri,
                name: a.name().to_string(),
            }
        });

        let album = proto.and_then(|t| t.album.as_ref()).map(|al| {
            let uri = SpotifyId::from_raw(al.gid())
                .ok()
                .and_then(|id| SpotifyUri::Album { id }.to_uri().ok())
                .unwrap_or_default();
            NamedUri {
                uri,
                name: al.name().to_string(),
            }
        });

        let context = entry.context_uri.as_ref().map(|ctx_uri| {
            let ctx_name = if let Some(pid) = playlist_id_from_uri(ctx_uri) {
                name_cache.playlists.get(&pid).cloned().unwrap_or_default()
            } else if album.as_ref().is_some_and(|a| &a.uri == ctx_uri) {
                album.as_ref().map(|a| a.name.clone()).unwrap_or_default()
            } else if artist.as_ref().is_some_and(|a| &a.uri == ctx_uri) {
                artist.as_ref().map(|a| a.name.clone()).unwrap_or_default()
            } else if ctx_uri == &entry.track_uri {
                track_name.clone()
            } else {
                String::new()
            };
            TrackContext {
                uri: ctx_uri.clone(),
                name: ctx_name,
                index: None,
            }
        });

        result.push(FriendActivity {
            timestamp: entry.timestamp_ms,
            playing: entry.playing,
            user: FriendUser {
                uri: entry.user_uri,
                name: user_name.unwrap_or(uid),
                image_url: user_image,
            },
            track: FriendTrack {
                uri: entry.track_uri,
                name: track_name,
                image_url: None,
                album,
                artist,
                context,
            },
        });
    }

    Ok(result)
}
