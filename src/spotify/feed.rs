use std::collections::HashMap;
use std::fmt;

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};
use http::{Method, Request};
use librespot_core::{session::Session, spclient::CLIENT_TOKEN};

use super::metadata::{
    ItemMetadata, build_episode_from_proto, build_track_from_proto, fetch_items_metadata,
    resolve_missing_names,
};
use super::models::{
    BuddyListResponse, FriendActivity, FriendTrack, FriendUser, LiveEntry, NameCache, NamedUri,
    TrackContext, canonical_spotify_dj_uri, classify_context_type, is_spotify_dj_context,
    normalize_context_uri, playlist_id_from_uri, user_id_from_uri,
};
use super::{BUDDYLIST_ENDPOINT, LISTENING_ACTIVITY_ENDPOINT, LISTENING_ACTIVITY_LIMIT};

#[allow(dead_code)]
const WEB_API_CURRENTLY_PLAYING_URL: &str =
    "https://api.spotify.com/v1/me/player/currently-playing?additional_types=track";
#[allow(dead_code)]
const WEB_API_RECENTLY_PLAYED_URL: &str = "https://api.spotify.com/v1/me/player/recently-played";

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WebApiAuthError {
    pub status: u16,
    pub message: String,
}

impl fmt::Display for WebApiAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Spotify Web API auth/scope error ({}): {}", self.status, self.message)
    }
}

impl std::error::Error for WebApiAuthError {}

#[allow(dead_code)]
pub fn is_web_api_auth_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<WebApiAuthError>().is_some()
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

fn is_liked_songs_context(uri: &str, context_type: &str, name: &str) -> bool {
    let u = uri.trim();
    let t = context_type.trim();
    let n = name.trim();
    u.eq_ignore_ascii_case("spotify:collection")
        || u.ends_with(":collection")
        || u.contains(":collection:")
        || t.eq_ignore_ascii_case("collection")
        || t.eq_ignore_ascii_case("liked_songs")
        || n.eq_ignore_ascii_case("liked songs")
}

fn canonical_liked_songs_uri(user_uri: &str) -> String {
    let uid = user_uri
        .strip_prefix("spotify:user:")
        .unwrap_or(user_uri)
        .trim();
    format!("spotify:user:{uid}:collection")
}

fn listener_display_name(user_uri: &str, user_name: &str) -> String {
    let trimmed = user_name.trim();
    if trimmed.is_empty() {
        user_id_from_uri(user_uri)
    } else {
        trimmed.to_string()
    }
}

pub(super) fn parse_live_feed(json: &serde_json::Value) -> Result<Vec<LiveEntry>> {
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
            .filter(|s| !s.is_empty())
            .map(|uri| {
                if is_liked_songs_context(&uri, "", "") {
                    canonical_liked_songs_uri(user_uri)
                } else if is_spotify_dj_context(Some(&uri), None) {
                    canonical_spotify_dj_uri(user_uri)
                } else {
                    uri
                }
            });

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

pub(super) fn normalize_legacy_friends(friends: &mut [FriendActivity]) {
    for item in friends {
        if item.track.artists.is_empty()
            && let Some(artist) = item.track.artist.clone()
        {
            item.track.artists.push(artist);
        }
        if item.track.cover_url.is_none() {
            item.track.cover_url = item.track.image_url.clone();
        }
        let owner_name = listener_display_name(&item.user.uri, &item.user.name);
        if let Some(ctx) = item.track.context.as_mut() {
            if is_liked_songs_context(&ctx.uri, &ctx.context_type, &ctx.name) {
                ctx.uri = canonical_liked_songs_uri(&item.user.uri);
                ctx.name = "Liked Songs".to_string();
                ctx.context_type = "liked_songs".to_string();
                ctx.owner = Some(owner_name);
                ctx.is_private = true;
                ctx.image_url = None;
            } else if is_spotify_dj_context(Some(&ctx.uri), Some(&ctx.context_type)) {
                ctx.uri = canonical_spotify_dj_uri(&item.user.uri);
                ctx.name = "DJ".to_string();
                ctx.context_type = "dj".to_string();
                ctx.owner = Some(owner_name);
                ctx.is_private = true;
                ctx.image_url = None;
            } else if ctx.context_type.is_empty() {
                ctx.context_type = classify_context_type(&ctx.uri, &ctx.name).to_string();
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

    let mut parsed: BuddyListResponse = serde_json::from_slice(&raw).with_context(|| {
        let preview = String::from_utf8_lossy(&raw);
        let snippet: String = preview.chars().take(300).collect();
        format!("Failed to parse buddylist JSON (preview: {snippet})")
    })?;

    normalize_legacy_friends(&mut parsed.friends);
    Ok(parsed.friends)
}

pub(super) fn materialize_live_entries(
    entries: Vec<LiveEntry>,
    name_cache: &NameCache,
) -> Vec<FriendActivity> {
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let uid = user_id_from_uri(&entry.user_uri);
        let (user_name, user_image) = name_cache
            .profiles
            .get(&uid)
            .cloned()
            .unwrap_or((None, None));
        let resolved_user_name = user_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(uid);

        let mut track = name_cache.get_track(&entry.track_uri).unwrap_or_else(|| {
            FriendTrack {
                uri: entry.track_uri.clone(),
                name: entry.track_uri.clone(),
                item_type: if entry.track_uri.starts_with("spotify:episode:") {
                    "episode".to_string()
                } else {
                    "track".to_string()
                },
                ..Default::default()
            }
        });

        let context = entry.context_uri.as_ref().map(|ctx_uri| {
            if is_liked_songs_context(ctx_uri, "", "") {
                TrackContext {
                    uri: canonical_liked_songs_uri(&entry.user_uri),
                    name: "Liked Songs".to_string(),
                    index: None,
                    context_type: "liked_songs".to_string(),
                    owner: Some(resolved_user_name.clone()),
                    is_private: true,
                    image_url: None,
                }
            } else if is_spotify_dj_context(Some(ctx_uri), None) {
                TrackContext {
                    uri: canonical_spotify_dj_uri(&entry.user_uri),
                    name: "DJ".to_string(),
                    index: None,
                    context_type: "dj".to_string(),
                    owner: Some(resolved_user_name.clone()),
                    is_private: true,
                    image_url: None,
                }
            } else if let Some(pid) = playlist_id_from_uri(ctx_uri) {
                let (name, owner, is_private, image_url) = match name_cache.playlists.get(&pid) {
                    Some(info) => (
                        info.name.clone(),
                        info.owner.clone(),
                        info.is_private,
                        info.image_url.clone(),
                    ),
                    None => (String::new(), None, true, None),
                };
                TrackContext {
                    uri: ctx_uri.clone(),
                    name,
                    index: None,
                    context_type: "playlist".to_string(),
                    owner,
                    is_private,
                    image_url,
                }
            } else {
                let is_album_ctx = track.album.as_ref().is_some_and(|a| &a.uri == ctx_uri);
                let ctx_name = if is_album_ctx {
                    track
                        .album
                        .as_ref()
                        .map(|a| a.name.clone())
                        .unwrap_or_default()
                } else if let Some(a) = track.artists.iter().find(|a| &a.uri == ctx_uri) {
                    a.name.clone()
                } else if track.artist.as_ref().is_some_and(|a| &a.uri == ctx_uri) {
                    track
                        .artist
                        .as_ref()
                        .map(|a| a.name.clone())
                        .unwrap_or_default()
                } else if ctx_uri == &entry.track_uri {
                    track.name.clone()
                } else {
                    String::new()
                };
                let context_type = classify_context_type(ctx_uri, &ctx_name).to_string();
                if is_liked_songs_context(ctx_uri, &context_type, &ctx_name) {
                    TrackContext {
                        uri: canonical_liked_songs_uri(&entry.user_uri),
                        name: "Liked Songs".to_string(),
                        index: None,
                        context_type: "liked_songs".to_string(),
                        owner: Some(resolved_user_name.clone()),
                        is_private: true,
                        image_url: None,
                    }
                } else {
                    let image_url = if is_album_ctx {
                        track.cover_url.clone().or_else(|| track.image_url.clone())
                    } else {
                        None
                    };
                    TrackContext {
                        uri: ctx_uri.clone(),
                        name: ctx_name,
                        index: None,
                        context_type,
                        owner: None,
                        is_private: false,
                        image_url,
                    }
                }
            }
        });
        track.context = context;

        result.push(FriendActivity {
            timestamp: entry.timestamp_ms,
            playing: entry.playing,
            user: FriendUser {
                uri: entry.user_uri,
                name: resolved_user_name,
                image_url: user_image,
            },
            track,
        });
    }

    result
}

pub(super) async fn enrich_live_entries(
    session: &Session,
    entries: Vec<LiveEntry>,
    name_cache: &mut NameCache,
) -> Result<Vec<FriendActivity>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let missing_uris: Vec<String> = entries
        .iter()
        .map(|e| &e.track_uri)
        .filter(|uri| !name_cache.tracks.contains_key(uri.as_str()))
        .cloned()
        .collect();

    if !missing_uris.is_empty() {
        let items = fetch_items_metadata(session, &missing_uris).await?;
        for (uri, meta) in items {
            let track = match meta {
                ItemMetadata::Track(t) => build_track_from_proto(&uri, &t),
                ItemMetadata::Episode(ep) => build_episode_from_proto(&uri, &ep),
            };
            name_cache.insert_track(uri, track);
        }
    }

    resolve_missing_names(session, &entries, name_cache).await;

    Ok(materialize_live_entries(entries, name_cache))
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

    enrich_live_entries(session, live_entries, name_cache).await
}

pub async fn enrich_activity(
    session: &Session,
    activity: &mut FriendActivity,
    name_cache: &mut NameCache,
) {
    let needs_track_meta = activity.track.duration_ms.is_none()
        || activity.track.popularity.is_none()
        || activity.track.cover_url.is_none()
        || activity.track.artists.is_empty();

    if needs_track_meta
        && (activity.track.uri.starts_with("spotify:track:")
            || activity.track.uri.starts_with("spotify:episode:"))
    {
        let uris = vec![activity.track.uri.clone()];
        if let Ok(items) = fetch_items_metadata(session, &uris).await {
            let enriched = match items.get(&activity.track.uri) {
                Some(ItemMetadata::Track(t)) => Some(build_track_from_proto(&activity.track.uri, t)),
                Some(ItemMetadata::Episode(ep)) => {
                    Some(build_episode_from_proto(&activity.track.uri, ep))
                }
                None => None,
            };
            if let Some(mut new_track) = enriched {
                new_track.context = activity.track.context.take();
                activity.track = new_track;
            }
        }
    }

    let ctx_uri = activity.track.context.as_ref().map(|c| c.uri.clone());
    if let Some(ref uri) = ctx_uri
        && !is_spotify_dj_context(Some(uri), None)
        && let Some(pid) = playlist_id_from_uri(uri)
        && !name_cache.playlists.contains_key(&pid)
    {
        let entry = LiveEntry {
            user_uri: activity.user.uri.clone(),
            track_uri: activity.track.uri.clone(),
            context_uri: Some(uri.clone()),
            playing: activity.playing,
            timestamp_ms: activity.timestamp,
        };
        resolve_missing_names(session, &[entry], name_cache).await;
    }

    let owner_name = listener_display_name(&activity.user.uri, &activity.user.name);
    if let Some(ctx) = activity.track.context.as_mut() {
        if is_liked_songs_context(&ctx.uri, &ctx.context_type, &ctx.name) {
            ctx.uri = canonical_liked_songs_uri(&activity.user.uri);
            ctx.name = "Liked Songs".to_string();
            ctx.context_type = "liked_songs".to_string();
            ctx.owner = Some(owner_name);
            ctx.is_private = true;
            ctx.image_url = None;
        } else if is_spotify_dj_context(Some(&ctx.uri), Some(&ctx.context_type)) {
            ctx.uri = canonical_spotify_dj_uri(&activity.user.uri);
            ctx.name = "DJ".to_string();
            ctx.context_type = "dj".to_string();
            ctx.owner = Some(owner_name);
            ctx.is_private = true;
            ctx.image_url = None;
        } else if let Some(pid) = playlist_id_from_uri(&ctx.uri) {
            if let Some(info) = name_cache.playlists.get(&pid) {
                if !info.name.is_empty() {
                    ctx.name = info.name.clone();
                }
                if info.owner.is_some() {
                    ctx.owner = info.owner.clone();
                }
                ctx.is_private = info.is_private;
                if info.image_url.is_some() {
                    ctx.image_url = info.image_url.clone();
                }
            }
            if ctx.context_type.is_empty() {
                ctx.context_type = "playlist".to_string();
            }
        } else {
            let is_album_ctx = activity
                .track
                .album
                .as_ref()
                .is_some_and(|a| a.uri == ctx.uri);
            if ctx.name.is_empty() {
                if is_album_ctx {
                    ctx.name = activity
                        .track
                        .album
                        .as_ref()
                        .map(|a| a.name.clone())
                        .unwrap_or_default();
                } else if let Some(a) = activity.track.artists.iter().find(|a| a.uri == ctx.uri) {
                    ctx.name = a.name.clone();
                } else if activity
                    .track
                    .artist
                    .as_ref()
                    .is_some_and(|a| a.uri == ctx.uri)
                {
                    ctx.name = activity
                        .track
                        .artist
                        .as_ref()
                        .map(|a| a.name.clone())
                        .unwrap_or_default();
                } else if ctx.uri == activity.track.uri {
                    ctx.name = activity.track.name.clone();
                }
            }
            if ctx.context_type.is_empty() {
                ctx.context_type = classify_context_type(&ctx.uri, &ctx.name).to_string();
            }
            if is_liked_songs_context(&ctx.uri, &ctx.context_type, &ctx.name) {
                ctx.uri = canonical_liked_songs_uri(&activity.user.uri);
                ctx.name = "Liked Songs".to_string();
                ctx.context_type = "liked_songs".to_string();
                ctx.owner = Some(owner_name);
                ctx.is_private = true;
                ctx.image_url = None;
            } else if ctx.image_url.is_none() && is_album_ctx {
                ctx.image_url = activity
                    .track
                    .cover_url
                    .clone()
                    .or_else(|| activity.track.image_url.clone());
            }
        }
    }
}

#[allow(dead_code)]
async fn get_web_api_json(session: &Session, url: &str) -> Result<Option<Bytes>> {
    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|e| anyhow!("Failed to obtain login5 auth token: {e}"))?;

    let request = Request::builder()
        .method(Method::GET)
        .uri(url)
        .header(ACCEPT, HeaderValue::from_static("application/json"))
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("{} {}", token.token_type, token.access_token))?,
        )
        .body(Bytes::new())?;

    match session.http_client().request_body(request).await {
        Ok(raw) => {
            if raw.is_empty() {
                // 204 No Content
                Ok(None)
            } else {
                Ok(Some(raw))
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("401") || msg.contains("Unauthorized") {
                return Err(WebApiAuthError {
                    status: 401,
                    message: msg,
                }
                .into());
            }
            if msg.contains("403") || msg.contains("Forbidden") {
                return Err(WebApiAuthError {
                    status: 403,
                    message: msg,
                }
                .into());
            }
            Err(anyhow!("Spotify Web API request failed ({url}): {e}"))
        }
    }
}

#[allow(dead_code)]
fn parse_web_api_track(
    item: &serde_json::Value,
    context_json: Option<&serde_json::Value>,
    user_uri: &str,
    owner_name: &str,
) -> Option<FriendTrack> {
    let uri = item.get("uri").and_then(|v| v.as_str()).filter(|s| !s.is_empty())?;
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(uri)
        .to_string();

    let item_type = item
        .get("type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("track")
        .to_string();

    let artists: Vec<NamedUri> = item
        .get("artists")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                    let a_uri = a.get("uri").and_then(|v| v.as_str()).unwrap_or_default();
                    if a_name.is_empty() && a_uri.is_empty() {
                        None
                    } else {
                        Some(NamedUri {
                            uri: a_uri.to_string(),
                            name: a_name.to_string(),
                        })
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let artist = artists.first().cloned();

    let album_val = item.get("album");
    let album = album_val.and_then(|al| {
        let al_uri = al.get("uri").and_then(|v| v.as_str()).unwrap_or_default();
        let al_name = al.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        if al_uri.is_empty() && al_name.is_empty() {
            None
        } else {
            Some(NamedUri {
                uri: al_uri.to_string(),
                name: al_name.to_string(),
            })
        }
    });

    let cover_url = album_val
        .and_then(|al| al.get("images"))
        .and_then(|v| v.as_array())
        .and_then(|imgs| {
            imgs.iter()
                .filter_map(|img| img.get("url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()))
                .next()
        })
        .map(str::to_string);

    let album_type = album_val
        .and_then(|al| al.get("album_type"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_uppercase());

    let release_date = album_val
        .and_then(|al| al.get("release_date"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let isrc = item
        .get("external_ids")
        .and_then(|ext| ext.get("isrc"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let duration_ms = item.get("duration_ms").and_then(|v| v.as_i64());
    let popularity = item
        .get("popularity")
        .and_then(|v| v.as_i64())
        .and_then(|n| i32::try_from(n).ok());
    let explicit = item.get("explicit").and_then(|v| v.as_bool());
    let track_number = item
        .get("track_number")
        .and_then(|v| v.as_i64())
        .and_then(|n| i32::try_from(n).ok());
    let disc_number = item
        .get("disc_number")
        .and_then(|v| v.as_i64())
        .and_then(|n| i32::try_from(n).ok());

    let context = context_json.and_then(|ctx| {
        let raw_uri = ctx.get("uri").and_then(|v| v.as_str()).filter(|s| !s.is_empty())?;
        let ctx_uri = normalize_context_uri(raw_uri);
        let raw_type = ctx.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        if is_liked_songs_context(&ctx_uri, raw_type, "") {
            return Some(TrackContext {
                uri: canonical_liked_songs_uri(user_uri),
                name: "Liked Songs".to_string(),
                index: None,
                context_type: "liked_songs".to_string(),
                owner: Some(owner_name.to_string()),
                is_private: true,
                image_url: None,
            });
        }
        if is_spotify_dj_context(Some(&ctx_uri), Some(raw_type)) {
            return Some(TrackContext {
                uri: canonical_spotify_dj_uri(user_uri),
                name: "DJ".to_string(),
                index: None,
                context_type: "dj".to_string(),
                owner: Some(owner_name.to_string()),
                is_private: true,
                image_url: None,
            });
        }
        let is_album_ctx = album.as_ref().is_some_and(|a| a.uri == ctx_uri);
        let ctx_name = if is_album_ctx {
            album.as_ref().map(|a| a.name.clone()).unwrap_or_default()
        } else if let Some(a) = artists.iter().find(|a| a.uri == ctx_uri) {
            a.name.clone()
        } else {
            String::new()
        };
        let context_type = if !raw_type.is_empty() {
            raw_type.to_lowercase()
        } else {
            classify_context_type(&ctx_uri, &ctx_name).to_string()
        };
        let is_private = context_type == "liked_songs";
        let image_url = if is_album_ctx { cover_url.clone() } else { None };
        Some(TrackContext {
            uri: ctx_uri,
            name: ctx_name,
            index: None,
            context_type,
            owner: None,
            is_private,
            image_url,
        })
    });

    Some(FriendTrack {
        uri: uri.to_string(),
        name,
        image_url: cover_url.clone(),
        album,
        artist,
        context,
        item_type,
        artists,
        duration_ms,
        popularity,
        explicit,
        track_number,
        disc_number,
        isrc,
        album_type,
        release_date,
        album_label: None,
        cover_url,
    })
}

#[allow(dead_code)]
pub(super) fn parse_own_currently_playing(
    json: &serde_json::Value,
    username: &str,
    user_name: &str,
    user_image_url: Option<&str>,
) -> Option<FriendActivity> {
    let item = json.get("item").filter(|v| v.is_object())?;
    let context_json = json.get("context").filter(|v| v.is_object());
    let display_name = if user_name.trim().is_empty() {
        username.to_string()
    } else {
        user_name.to_string()
    };
    let user_uri = format!("spotify:user:{username}");
    let track = parse_web_api_track(item, context_json, &user_uri, &display_name)?;

    let is_playing = json
        .get("is_playing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let progress_ms = json
        .get("progress_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0);
    let raw_ts = json
        .get("timestamp")
        .and_then(|v| v.as_i64())
        .filter(|&ts| ts > 0)
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    let started_at_ms = raw_ts.saturating_sub(progress_ms);

    Some(FriendActivity {
        timestamp: started_at_ms,
        playing: is_playing,
        user: FriendUser {
            uri: user_uri,
            name: display_name,
            image_url: user_image_url.map(str::to_string),
        },
        track,
    })
}

#[allow(dead_code)]
pub(super) fn parse_own_recently_played(
    json: &serde_json::Value,
    username: &str,
    user_name: &str,
    user_image_url: Option<&str>,
) -> Vec<FriendActivity> {
    let Some(items) = json.get("items").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let display_name = if user_name.trim().is_empty() {
        username.to_string()
    } else {
        user_name.to_string()
    };
    let user_uri = format!("spotify:user:{username}");

    let mut out = Vec::with_capacity(items.len());
    for entry in items {
        let Some(track_val) = entry.get("track").filter(|v| v.is_object()) else {
            continue;
        };
        let Some(played_at_str) = entry.get("played_at").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(dt) = DateTime::parse_from_rfc3339(played_at_str) else {
            continue;
        };
        let context_json = entry.get("context").filter(|v| v.is_object());
        let Some(track) = parse_web_api_track(track_val, context_json, &user_uri, &display_name) else {
            continue;
        };
        out.push(FriendActivity {
            timestamp: dt.timestamp_millis(),
            playing: false,
            user: FriendUser {
                uri: user_uri.clone(),
                name: display_name.clone(),
                image_url: user_image_url.map(str::to_string),
            },
            track,
        });
    }
    out
}

#[allow(dead_code)]
pub async fn fetch_own_currently_playing(
    session: &Session,
    user_name: &str,
    user_image_url: Option<&str>,
) -> Result<Option<FriendActivity>> {
    let Some(raw) = get_web_api_json(session, WEB_API_CURRENTLY_PLAYING_URL).await? else {
        return Ok(None);
    };

    let json: serde_json::Value = serde_json::from_slice(&raw).with_context(|| {
        let preview = String::from_utf8_lossy(&raw);
        let snippet: String = preview.chars().take(300).collect();
        format!("Failed to parse currently-playing JSON (preview: {snippet})")
    })?;

    Ok(parse_own_currently_playing(
        &json,
        &session.username(),
        user_name,
        user_image_url,
    ))
}

#[allow(dead_code)]
pub async fn fetch_own_recently_played(
    session: &Session,
    user_name: &str,
    user_image_url: Option<&str>,
    limit: usize,
) -> Result<Vec<FriendActivity>> {
    let url = format!("{WEB_API_RECENTLY_PLAYED_URL}?limit={}", limit.clamp(1, 50));
    let Some(raw) = get_web_api_json(session, &url).await? else {
        return Ok(Vec::new());
    };

    let json: serde_json::Value = serde_json::from_slice(&raw).with_context(|| {
        let preview = String::from_utf8_lossy(&raw);
        let snippet: String = preview.chars().take(300).collect();
        format!("Failed to parse recently-played JSON (preview: {snippet})")
    })?;

    Ok(parse_own_recently_played(
        &json,
        &session.username(),
        user_name,
        user_image_url,
    ))
}
