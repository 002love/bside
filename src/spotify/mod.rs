mod auth;
mod feed;
mod metadata;
mod models;
mod own_observer;
mod profile;

pub(super) const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
pub(super) const LISTENING_ACTIVITY_ENDPOINT: &str = "/listening-activity/v1/feed";
pub(super) const BUDDYLIST_ENDPOINT: &str = "/presence-view/v1/buddylist";
pub(super) const LISTENING_ACTIVITY_LIMIT: u32 = 100;

#[allow(unused_imports)]
pub use auth::{connect_cached, login_oauth, open_cache};
#[allow(unused_imports)]
pub use feed::{
    WebApiAuthError, enrich_activity, fetch_buddylist, fetch_buddylist_raw,
    fetch_own_currently_playing, fetch_own_recently_played, is_web_api_auth_error,
};
#[allow(unused_imports)]
pub use metadata::format_proto_date;
#[allow(unused_imports)]
pub use models::{
    BuddyListResponse, FriendActivity, FriendTrack, FriendUser, NameCache, NamedUri, PlaylistInfo,
    PublicPlaylist, TrackContext, UserProfileSnapshot, classify_context_type,
};
pub use own_observer::OwnPlaybackObserver;
pub use profile::fetch_user_profile;

#[cfg(test)]
mod tests {
    use librespot_protocol::metadata::{
        Date as ProtoDate, Image as ProtoImage, image::Size as ProtoImageSize,
    };
    use protobuf::EnumOrUnknown;

    use super::feed::parse_live_feed;
    use super::metadata::extract_cover_url;
    use super::profile::parse_user_profile;
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

    #[test]
    fn extract_playlist_image_url_prefers_picture_size_then_hex() {
        use librespot_protocol::playlist4_external::{ListAttributes, PictureSize};
        use super::metadata::extract_playlist_image_url;

        let attrs_with_sizes = ListAttributes {
            picture: Some(vec![0xde, 0xad, 0xbe, 0xef]),
            picture_size: vec![
                PictureSize {
                    target_name: Some("small".to_string()),
                    url: Some("https://mosaic.scdn.co/300/abc".to_string()),
                    ..Default::default()
                },
                PictureSize {
                    target_name: Some("default".to_string()),
                    url: Some("https://mosaic.scdn.co/640/abc".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            extract_playlist_image_url(&attrs_with_sizes).as_deref(),
            Some("https://mosaic.scdn.co/640/abc")
        );

        let attrs_hex_only = ListAttributes {
            picture: Some(vec![0x01, 0xab, 0xff]),
            ..Default::default()
        };
        assert_eq!(
            extract_playlist_image_url(&attrs_hex_only).as_deref(),
            Some("https://i.scdn.co/image/01abff")
        );
    }

    #[test]
    fn parse_own_currently_playing_and_recently_played() {
        use super::feed::{parse_own_currently_playing, parse_own_recently_played};

        let cp_json = serde_json::json!({
            "timestamp": 1_700_000_050_000i64,
            "progress_ms": 50_000i64,
            "is_playing": true,
            "context": {
                "type": "playlist",
                "uri": "spotify:playlist:pl123"
            },
            "item": {
                "uri": "spotify:track:t123",
                "name": "My Track",
                "duration_ms": 200_000,
                "popularity": 72,
                "explicit": true,
                "artists": [
                    { "uri": "spotify:artist:a1", "name": "Artist 1" }
                ],
                "album": {
                    "uri": "spotify:album:al1",
                    "name": "Album 1",
                    "release_date": "2024-01-15",
                    "images": [
                        { "url": "https://i.scdn.co/image/cover1", "width": 640, "height": 640 }
                    ]
                }
            }
        });

        let cp = parse_own_currently_playing(&cp_json, "me123", "Me", Some("https://img/me")).unwrap();
        assert_eq!(cp.timestamp, 1_700_000_000_000);
        assert!(cp.playing);
        assert_eq!(cp.user.uri, "spotify:user:me123");
        assert_eq!(cp.user.name, "Me");
        assert_eq!(cp.track.uri, "spotify:track:t123");
        assert_eq!(cp.track.duration_ms, Some(200_000));
        assert_eq!(cp.context_uri(), Some("spotify:playlist:pl123"));
        assert_eq!(cp.context_type(), Some("playlist"));

        let rp_json = serde_json::json!({
            "items": [
                {
                    "played_at": "2026-11-24T20:00:00Z",
                    "context": { "type": "album", "uri": "spotify:album:al1" },
                    "track": {
                        "uri": "spotify:track:t456",
                        "name": "Recent Track",
                        "duration_ms": 180_000,
                        "artists": [
                            { "uri": "spotify:artist:a2", "name": "Artist 2" }
                        ],
                        "album": {
                            "uri": "spotify:album:al1",
                            "name": "Album 1",
                            "images": [{ "url": "https://i.scdn.co/image/cover1" }]
                        }
                    }
                }
            ]
        });

        let rp = parse_own_recently_played(&rp_json, "me123", "Me", None);
        assert_eq!(rp.len(), 1);
        assert!(!rp[0].playing);
        assert_eq!(rp[0].track.uri, "spotify:track:t456");
        assert_eq!(rp[0].context_image_url(), Some("https://i.scdn.co/image/cover1"));
    }
}
