use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
    pub(super) profiles: HashMap<String, (Option<String>, Option<String>)>,
    pub(super) playlists: HashMap<String, PlaylistInfo>,
}

#[derive(Debug, Clone)]
pub(super) struct LiveEntry {
    pub(super) user_uri: String,
    pub(super) track_uri: String,
    pub(super) context_uri: Option<String>,
    pub(super) playing: bool,
    pub(super) timestamp_ms: i64,
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

pub(super) fn user_id_from_uri(uri: &str) -> String {
    uri.rsplit(':').next().unwrap_or_default().to_string()
}

pub(super) fn playlist_id_from_uri(uri: &str) -> Option<String> {
    uri.contains(":playlist:")
        .then(|| uri.rsplit(':').next().unwrap_or_default().to_string())
        .filter(|s| !s.is_empty())
}

pub(super) fn normalize_context_uri(uri: &str) -> String {
    uri.strip_prefix("spotify:list:popular-release-segments-main-roles:artist_")
        .map(|id| format!("spotify:artist:{id}"))
        .unwrap_or_else(|| uri.to_string())
}
