use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use chrono::DateTime;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};
use http::{Method, Request};
use librespot_core::{session::Session, spclient::CLIENT_TOKEN};

use super::metadata::{
    ItemMetadata, build_episode_from_proto, build_track_from_proto, fetch_items_metadata,
    resolve_missing_names,
};
use super::models::{
    BuddyListResponse, FriendActivity, FriendTrack, FriendUser, LiveEntry, NameCache, TrackContext,
    classify_context_type, normalize_context_uri, playlist_id_from_uri, user_id_from_uri,
};
use super::{BUDDYLIST_ENDPOINT, LISTENING_ACTIVITY_ENDPOINT, LISTENING_ACTIVITY_LIMIT};

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
