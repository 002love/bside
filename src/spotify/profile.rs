use anyhow::{Context, Result, anyhow};
use librespot_core::session::Session;

use super::models::{NamedUri, PublicPlaylist, UserProfileSnapshot, user_id_from_uri};

pub(super) fn json_str(val: &serde_json::Value, keys: &[&str]) -> Option<String> {
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

pub(super) fn parse_user_profile(user_uri: &str, json: &serde_json::Value) -> UserProfileSnapshot {
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
