mod auth;
mod feed;
mod metadata;
mod models;
mod own_observer;
mod profile;
mod realtime;

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
#[allow(unused_imports)]
pub use realtime::{
    ActivityEvent, StreamConfig, TransportMode, TransportPreference, spawn_activity_stream,
};

#[cfg(test)]
mod tests {
    use librespot_protocol::metadata::{
        Date as ProtoDate, Image as ProtoImage, image::Size as ProtoImageSize,
    };
    use protobuf::EnumOrUnknown;

    use super::feed::{materialize_live_entries, parse_live_feed};
    use super::metadata::extract_cover_url;
    use super::models::{LiveEntry, TRACK_CACHE_CAP};
    use super::profile::parse_user_profile;
    use super::realtime::{ParsedPushPayload, parse_push_payload, percent_encode_path_segment};
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

    #[test]
    fn percent_encode_path_segment_rfc3986() {
        assert_eq!(
            percent_encode_path_segment("abcXYZ0129-._~"),
            "abcXYZ0129-._~"
        );
        assert_eq!(
            percent_encode_path_segment("YWJj+123/456=="),
            "YWJj%2B123%2F456%3D%3D"
        );
        assert_eq!(
            percent_encode_path_segment("hello world%20é"),
            "hello%20world%2520%C3%A9"
        );
    }

    #[test]
    fn transport_mode_display_as_str_and_serde() {
        assert_eq!(TransportMode::DealerPush.as_str(), "dealer_push");
        assert_eq!(TransportMode::MercuryPush.as_str(), "mercury_push");
        assert_eq!(TransportMode::HttpPolling.as_str(), "http_polling");
        assert_eq!(TransportMode::DealerPush.to_string(), "dealer_push");
        assert_eq!(
            serde_json::to_string(&TransportMode::MercuryPush).unwrap(),
            "\"mercury_push\""
        );
    }

    #[test]
    fn parse_push_payload_listening_activity_json() {
        let payload = serde_json::json!({
            "entities": [
                {
                    "userEntity": {
                        "uri": "spotify:user:alice",
                        "activity": {
                            "entityUri": "spotify:track:1111111111111111111111",
                            "contextUri": "spotify:playlist:2222222222222222222222",
                            "timestamp": "2026-11-24T20:00:00Z",
                            "isPlaying": true
                        }
                    }
                }
            ]
        })
        .to_string();

        match parse_push_payload("hm://listening-activity/v1/feed", payload.as_bytes()) {
            Some(ParsedPushPayload::Entries(entries)) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].user_uri, "spotify:user:alice");
                assert_eq!(entries[0].track_uri, "spotify:track:1111111111111111111111");
                assert_eq!(
                    entries[0].context_uri.as_deref(),
                    Some("spotify:playlist:2222222222222222222222")
                );
                assert!(entries[0].playing);
            }
            other => panic!("expected Entries, got {other:?}"),
        }
    }

    #[test]
    fn parse_push_payload_legacy_buddylist_json_normalizes_fields() {
        let payload = serde_json::json!({
            "friends": [
                {
                    "timestamp": 1700000000000_i64,
                    "user": { "uri": "spotify:user:bob", "name": "Bob" },
                    "track": {
                        "uri": "spotify:track:abc",
                        "name": "Legacy Song",
                        "imageUrl": "https://i.scdn.co/image/legacy",
                        "artist": { "uri": "spotify:artist:1", "name": "Artist A" },
                        "context": { "uri": "spotify:user:bob:collection", "name": "Liked Songs" }
                    }
                }
            ]
        })
        .to_string();

        match parse_push_payload("hm://presence-view/v1/buddylist", payload.as_bytes()) {
            Some(ParsedPushPayload::LegacyFriends(friends)) => {
                assert_eq!(friends.len(), 1);
                let f = &friends[0];
                assert_eq!(f.track.artists.len(), 1);
                assert_eq!(f.track.artists[0].name, "Artist A");
                assert_eq!(
                    f.track.cover_url.as_deref(),
                    Some("https://i.scdn.co/image/legacy")
                );
                assert_eq!(f.context_type(), Some("liked_songs"));
                assert!(f.is_private_context());
            }
            other => panic!("expected LegacyFriends, got {other:?}"),
        }
    }

    #[test]
    fn parse_push_payload_single_friend_legacy_and_flat_presence2_json() {
        // Legacy nested single-friend object
        let legacy_single = serde_json::json!({
            "timestamp": 1700000000,
            "playing": false,
            "user": { "uri": "spotify:user:carol" },
            "track": {
                "uri": "spotify:track:999",
                "context": { "uri": "spotify:album:888" }
            }
        })
        .to_string();

        match parse_push_payload("hm://presence2/user/carol", legacy_single.as_bytes()) {
            Some(ParsedPushPayload::Entries(entries)) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].user_uri, "spotify:user:carol");
                assert_eq!(entries[0].track_uri, "spotify:track:999");
                assert_eq!(entries[0].context_uri.as_deref(), Some("spotify:album:888"));
                assert_eq!(entries[0].timestamp_ms, 1_700_000_000_000);
                assert!(!entries[0].playing);
            }
            other => panic!("expected Entries, got {other:?}"),
        }

        // Flat presence2 JSON with missing user_uri inferred from topic URI
        let flat_presence2 = serde_json::json!({
            "trackUri": "spotify:episode:ep123",
            "contextUri": "spotify:show:sh456",
            "timestamp": "2026-11-24T20:15:00Z",
            "is_playing": true
        })
        .to_string();

        match parse_push_payload("hm://presence2/user/alice", flat_presence2.as_bytes()) {
            Some(ParsedPushPayload::Entries(entries)) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].user_uri, "spotify:user:alice");
                assert_eq!(entries[0].track_uri, "spotify:episode:ep123");
                assert_eq!(entries[0].context_uri.as_deref(), Some("spotify:show:sh456"));
                assert_eq!(entries[0].timestamp_ms, 1_795_551_300_000);
                assert!(entries[0].playing);
            }
            other => panic!("expected Entries, got {other:?}"),
        }
    }

    fn encode_varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let mut byte = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if v == 0 {
                break;
            }
        }
    }

    fn encode_len_delimited(field_number: u32, data: &[u8], out: &mut Vec<u8>) {
        encode_varint(((field_number as u64) << 3) | 2, out);
        encode_varint(data.len() as u64, out);
        out.extend_from_slice(data);
    }

    fn encode_varint_field(field_number: u32, value: u64, out: &mut Vec<u8>) {
        encode_varint((field_number as u64) << 3, out);
        encode_varint(value, out);
    }

    #[test]
    fn parse_push_payload_nested_protobuf_binary() {
        // Inner submessage: track URI + artist list context URI
        let mut inner = Vec::new();
        encode_len_delimited(1, b"spotify:track:7777777777777777777777", &mut inner);
        encode_len_delimited(
            2,
            b"spotify:list:popular-release-segments-main-roles:artist_5555555555555555555555",
            &mut inner,
        );

        // Outer message: nested submessage + timestamp in Unix seconds
        let mut outer = Vec::new();
        encode_len_delimited(1, &inner, &mut outer);
        encode_varint_field(2, 1_732_500_000, &mut outer);

        match parse_push_payload("hm://presence2/user/dave", &outer) {
            Some(ParsedPushPayload::Entries(entries)) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].user_uri, "spotify:user:dave");
                assert_eq!(entries[0].track_uri, "spotify:track:7777777777777777777777");
                assert_eq!(
                    entries[0].context_uri.as_deref(),
                    Some("spotify:artist:5555555555555555555555")
                );
                assert_eq!(entries[0].timestamp_ms, 1_732_500_000_000);
                assert!(entries[0].playing);
            }
            other => panic!("expected Entries from protobuf, got {other:?}"),
        }
    }

    #[test]
    fn parse_push_payload_invalidation_hints() {
        assert!(matches!(
            parse_push_payload("hm://presence-view/v1/buddylist", b""),
            Some(ParsedPushPayload::InvalidateHint)
        ));
        assert!(matches!(
            parse_push_payload("hm://listening-activity/v1/update", b"{}"),
            Some(ParsedPushPayload::InvalidateHint)
        ));
        assert!(matches!(
            parse_push_payload("hm://presence-view/v2/events", b"  { }  "),
            Some(ParsedPushPayload::InvalidateHint)
        ));
        assert!(matches!(
            parse_push_payload("hm://listening-activity/v1/events", br#"{"event":"update"}"#),
            Some(ParsedPushPayload::InvalidateHint)
        ));
        assert!(matches!(
            parse_push_payload("hm://presence-view/v2/events", b"\x08\x2a"),
            Some(ParsedPushPayload::InvalidateHint)
        ));
        assert!(parse_push_payload("hm://presence2/user/alice", b"").is_none());
        assert!(parse_push_payload("hm://presence2/user/alice", b"{}").is_none());
    }

    #[test]
    fn parse_push_payload_decodes_percent_encoded_topic_and_ignores_binary_blob_noise() {
        let json = br#"{"track_uri":"spotify:track:9999999999999999999999","timestamp":1700000000000}"#;
        let parsed = parse_push_payload("hm://presence2/user/john%20doe", json)
            .expect("should infer decoded user URI");
        match parsed {
            ParsedPushPayload::Entries(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].user_uri, "spotify:user:john doe");
                assert_eq!(entries[0].timestamp_ms, 1_700_000_000_000);
            }
            other => panic!("expected Entries, got {other:?}"),
        }

        // Protobuf with a preceding garbage/non-protobuf binary blob (field 1) followed by
        // real track_uri (field 2) and real timestamp (field 3): the binary blob must NOT
        // corrupt the extracted timestamp.
        let mut payload = Vec::new();
        {
            let mut os = protobuf::CodedOutputStream::vec(&mut payload);
            // Field 1: raw bytes starting with tag=1/varint(1_500_000_000) followed by invalid wire type 7 (0x0f)
            let mut noisy_blob = Vec::new();
            {
                let mut nos = protobuf::CodedOutputStream::vec(&mut noisy_blob);
                nos.write_uint64(1, 1_500_000_000).unwrap();
                nos.flush().unwrap();
            }
            noisy_blob.push(0x0f); // invalid protobuf wire type 7 -> not a valid submessage
            os.write_bytes(1, &noisy_blob).unwrap();
            os.write_string(2, "spotify:track:7777777777777777777777")
                .unwrap();
            os.write_uint64(3, 1_750_000_000_000).unwrap();
            os.flush().unwrap();
        }
        let parsed = parse_push_payload("hm://presence2/user/alice", &payload)
            .expect("should parse protobuf");
        match parsed {
            ParsedPushPayload::Entries(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].timestamp_ms, 1_750_000_000_000);
                assert_eq!(entries[0].track_uri, "spotify:track:7777777777777777777777");
            }
            other => panic!("expected Entries, got {other:?}"),
        }
    }

    #[test]
    fn name_cache_track_hit_miss_cap_and_context_isolation() {
        let mut cache = NameCache::default();
        assert!(cache.get_track("spotify:track:missing").is_none());

        cache.profiles.insert(
            "alice".to_string(),
            (Some("Alice".to_string()), Some("https://img/alice".to_string())),
        );
        cache
            .profiles
            .insert("bob".to_string(), (Some("Bob".to_string()), None));
        cache.playlists.insert(
            "pl123".to_string(),
            PlaylistInfo {
                name: "Chill Mix".to_string(),
                owner: Some("bob".to_string()),
                is_private: false,
                total_tracks: Some(25),
                image_url: None,
            },
        );

        let track_uri = "spotify:track:shared123".to_string();
        cache.insert_track(
            track_uri.clone(),
            FriendTrack {
                uri: track_uri.clone(),
                name: "Shared Anthem".to_string(),
                artist: Some(NamedUri {
                    uri: "spotify:artist:a1".to_string(),
                    name: "Daft Punk".to_string(),
                }),
                artists: vec![NamedUri {
                    uri: "spotify:artist:a1".to_string(),
                    name: "Daft Punk".to_string(),
                }],
                context: Some(TrackContext {
                    uri: "spotify:playlist:stale".to_string(),
                    name: "Should Be Stripped".to_string(),
                    ..Default::default()
                }),
                duration_ms: Some(240_000),
                ..Default::default()
            },
        );

        // Cached track must never retain stale context.
        let cached = cache.get_track(&track_uri).expect("track cache hit");
        assert_eq!(cached.name, "Shared Anthem");
        assert!(cached.context.is_none());

        // Two friends listening to the SAME cached track in DIFFERENT contexts.
        let activities = materialize_live_entries(
            vec![
                LiveEntry {
                    user_uri: "spotify:user:alice".to_string(),
                    track_uri: track_uri.clone(),
                    context_uri: Some("spotify:user:alice:collection".to_string()),
                    playing: true,
                    timestamp_ms: 1_700_000_001_000,
                },
                LiveEntry {
                    user_uri: "spotify:user:bob".to_string(),
                    track_uri: track_uri.clone(),
                    context_uri: Some("spotify:playlist:pl123".to_string()),
                    playing: true,
                    timestamp_ms: 1_700_000_002_000,
                },
                LiveEntry {
                    user_uri: "spotify:user:carol".to_string(),
                    track_uri: "spotify:episode:uncached_ep".to_string(),
                    context_uri: None,
                    playing: false,
                    timestamp_ms: 1_700_000_003_000,
                },
            ],
            &cache,
        );

        assert_eq!(activities.len(), 3);
        assert_eq!(activities[0].user.name, "Alice");
        assert_eq!(activities[0].track.name, "Shared Anthem");
        assert_eq!(activities[0].context_type(), Some("liked_songs"));
        assert!(activities[0].is_private_context());

        assert_eq!(activities[1].user.name, "Bob");
        assert_eq!(activities[1].track.name, "Shared Anthem");
        assert_eq!(activities[1].context_type(), Some("playlist"));
        assert_eq!(activities[1].context_name(), Some("Chill Mix"));
        assert_eq!(activities[1].context_owner(), Some("bob"));
        assert!(!activities[1].is_private_context());

        // Uncached miss falls back cleanly with episode item_type.
        assert_eq!(activities[2].track.name, "spotify:episode:uncached_ep");
        assert_eq!(activities[2].item_type(), "episode");
        assert!(activities[2].track.context.is_none());

        // Cache entry remains context-free.
        assert!(cache.tracks.get(&track_uri).unwrap().context.is_none());

        // Verify bounded cap eviction at TRACK_CACHE_CAP (2048).
        for i in 0..(TRACK_CACHE_CAP + 25) {
            let uri = format!("spotify:track:cap_{i}");
            cache.insert_track(
                uri.clone(),
                FriendTrack {
                    uri,
                    name: format!("Track {i}"),
                    ..Default::default()
                },
            );
        }
        assert_eq!(cache.tracks.len(), TRACK_CACHE_CAP);
        assert!(cache.get_track(&track_uri).is_none());
        assert!(
            cache
                .get_track(&format!("spotify:track:cap_{}", TRACK_CACHE_CAP + 24))
                .is_some()
        );
    }

    #[tokio::test]
    async fn spawn_activity_stream_exits_cleanly_without_credentials() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bside-no-creds-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let refresh = std::sync::Arc::new(tokio::sync::Notify::new());
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let (mut rx, handle) = spawn_activity_stream(
            temp_dir.clone(),
            StreamConfig::default(),
            refresh,
            shutdown_rx,
        );

        let ev = rx.recv().await.expect("expected missing credentials error event");
        match ev {
            ActivityEvent::Error {
                message,
                consecutive_errors,
            } => {
                assert!(
                    message.contains("No cached Spotify credentials"),
                    "unexpected message: {message}"
                );
                assert_eq!(consecutive_errors, 1);
            }
            other => panic!("expected Error event, got {other:?}"),
        }

        // Stream must terminate cleanly without spamming retries.
        assert!(rx.recv().await.is_none());
        handle.await.unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
