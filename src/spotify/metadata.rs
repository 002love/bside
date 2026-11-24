use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use anyhow::{Result, anyhow};
use librespot_core::{SpotifyId, SpotifyUri, session::Session};
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

use super::models::{
    FriendTrack, LiveEntry, NameCache, NamedUri, PlaylistInfo, playlist_id_from_uri,
    user_id_from_uri,
};
use super::profile::json_str;

pub(super) enum ItemMetadata {
    Track(ProtoTrack),
    Episode(ProtoEpisode),
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

pub(super) fn extract_cover_url(images: &[ProtoImage]) -> Option<String> {
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

pub(super) async fn fetch_items_metadata(
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

pub(super) async fn resolve_missing_names(
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

pub(super) fn build_track_from_proto(uri: &str, t: &ProtoTrack) -> FriendTrack {
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

    let release_date = album_proto
        .and_then(|al| al.date.as_ref())
        .and_then(format_proto_date);

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

pub(super) fn build_episode_from_proto(uri: &str, ep: &ProtoEpisode) -> FriendTrack {
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
