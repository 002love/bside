use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Datelike, Duration, Local, Timelike, Utc};
use rusqlite::{Connection, params};

use super::schema::LISTENS_MIGRATIONS;
use super::*;
use crate::spotify::{
    FriendActivity, FriendTrack, FriendUser, NamedUri, PublicPlaylist, TrackContext,
    UserProfileSnapshot,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_db_path(label: &str) -> (PathBuf, PathBuf) {
    let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "bside_test_{}_{}_{}",
        label,
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let db_path = dir.join("test.db");
    (dir, db_path)
}

fn sample_activity(ts: i64, track_uri: &str, track_name: &str) -> FriendActivity {
    FriendActivity {
        timestamp: ts,
        playing: true,
        user: FriendUser {
            uri: "spotify:user:alice123".to_string(),
            name: "Alice".to_string(),
            image_url: None,
        },
        track: FriendTrack {
            uri: track_uri.to_string(),
            name: track_name.to_string(),
            image_url: None,
            album: Some(NamedUri {
                uri: "spotify:album:1".to_string(),
                name: "Test Album".to_string(),
            }),
            artist: Some(NamedUri {
                uri: "spotify:artist:1".to_string(),
                name: "Test Artist".to_string(),
            }),
            context: Some(TrackContext {
                uri: "spotify:playlist:1".to_string(),
                name: "Chill Mix".to_string(),
                index: Some(0),
                context_type: "playlist".to_string(),
                owner: Some("spotify".to_string()),
                is_private: false,
            }),
            item_type: "track".to_string(),
            artists: vec![
                NamedUri {
                    uri: "spotify:artist:1".to_string(),
                    name: "Test Artist".to_string(),
                },
                NamedUri {
                    uri: "spotify:artist:2".to_string(),
                    name: "Guest Artist".to_string(),
                },
            ],
            duration_ms: Some(200_000),
            popularity: Some(72),
            explicit: Some(true),
            track_number: Some(3),
            disc_number: Some(1),
            isrc: Some("USRC17607839".to_string()),
            album_type: Some("ALBUM".to_string()),
            release_date: Some("2021-05-14".to_string()),
            album_label: Some("Indie Records".to_string()),
            cover_url: Some("https://i.scdn.co/image/abc123".to_string()),
        },
    }
}

#[test]
fn deduplicates_and_queries_stats() {
    let (dir, db_path) = temp_db_path("dedup");
    let db = Database::open(&db_path).unwrap();
    let now_ms = Utc::now().timestamp_millis();

    let mut a1 = sample_activity(now_ms - 60_000, "spotify:track:1", "Song One");
    a1.playing = false;
    let mut a1_live = a1.clone();
    a1_live.playing = true;
    let a2 = sample_activity(now_ms - 30_000, "spotify:track:2", "Song Two");

    assert!(db.insert_listen(&a1).unwrap());
    assert!(!db.insert_listen(&a1_live).unwrap()); // duplicate updates is_playing_seen
    assert!(db.insert_listen(&a2).unwrap());

    let playing_seen: i64 = db
        .conn
        .query_row(
            "SELECT is_playing_seen FROM listens WHERE track_uri = 'spotify:track:1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(playing_seen, 1);

    let hist = db.query_history(Some("alice"), Some(1), Some(10)).unwrap();
    assert_eq!(hist.len(), 2);
    assert_eq!(hist[0].track_name, "Song Two");
    assert_eq!(hist[1].track_name, "Song One");
    assert_eq!(
        hist[0].all_artists.as_deref(),
        Some("Test Artist, Guest Artist")
    );
    assert_eq!(hist[0].duration_ms, Some(200_000));
    assert_eq!(hist[0].popularity, Some(72));
    assert_eq!(hist[0].explicit, Some(true));
    assert_eq!(hist[0].isrc.as_deref(), Some("USRC17607839"));

    let stats = db.query_stats(Some("alice"), Some(1)).unwrap();
    assert_eq!(stats.total_plays, 2);
    assert_eq!(stats.unique_tracks, 2);
    assert_eq!(stats.unique_artists, 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn migrates_v1_schema_cleanly() {
    let (dir, db_path) = temp_db_path("migrate_v1");
    std::fs::create_dir_all(&dir).unwrap();

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE listens (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                played_at_ms INTEGER NOT NULL,
                logged_at_ms INTEGER NOT NULL,
                user_uri TEXT NOT NULL,
                user_name TEXT NOT NULL,
                track_uri TEXT NOT NULL,
                track_name TEXT NOT NULL,
                artist_name TEXT NOT NULL,
                artist_uri TEXT,
                album_name TEXT,
                album_uri TEXT,
                context_name TEXT,
                context_uri TEXT,
                UNIQUE(user_uri, track_uri, played_at_ms)
            );
            INSERT INTO listens (
                played_at_ms, logged_at_ms, user_uri, user_name,
                track_uri, track_name, artist_name
            ) VALUES (
                1700000000000, 1700000001000, 'spotify:user:legacy', 'Legacy User',
                'spotify:track:old1', 'Old Song', 'Old Artist'
            );
            ",
        )
        .unwrap();
    }

    let db = Database::open(&db_path).unwrap();

    let mut cols = HashSet::new();
    {
        let mut stmt = db.conn.prepare("PRAGMA table_info(listens)").unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
        for col in rows {
            cols.insert(col.unwrap());
        }
    }
    for (col_name, _) in LISTENS_MIGRATIONS {
        assert!(cols.contains(*col_name), "Missing migrated column {col_name}");
    }

    let now_ms = Utc::now().timestamp_millis();
    let new_act = sample_activity(now_ms, "spotify:track:new1", "New Song");
    assert!(db.insert_listen(&new_act).unwrap());

    let hist = db.query_history(None, None, None).unwrap();
    assert_eq!(hist.len(), 2);
    assert_eq!(hist[0].track_name, "New Song");
    assert_eq!(hist[1].track_name, "Old Song");
    assert_eq!(hist[1].item_type, "track");
    assert!(!hist[1].context_is_private);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tracks_skips_and_sessions() {
    let (dir, db_path) = temp_db_path("skips_sessions");
    let db = Database::open(&db_path).unwrap();
    let now_ms = Utc::now().timestamp_millis();

    // Track 1: 200s track, switched after 30s -> skipped
    let t1_start = now_ms - 600_000;
    let a1 = sample_activity(t1_start, "spotify:track:skip1", "Skipped Song");
    db.insert_listen(&a1).unwrap();
    db.finalize_play(
        &a1.user.uri,
        &a1.track.uri,
        t1_start,
        t1_start + 30_000,
        Some(200_000),
        true,
    )
    .unwrap();

    // Track 2: 200s track, switched after 190s -> completed
    let t2_start = now_ms - 400_000;
    let a2 = sample_activity(t2_start, "spotify:track:comp1", "Completed Song");
    db.insert_listen(&a2).unwrap();
    db.finalize_play(
        &a2.user.uri,
        &a2.track.uri,
        t2_start,
        t2_start + 190_000,
        Some(200_000),
        true,
    )
    .unwrap();

    // Track 3: 200s track, stopped (not switched) after 170s (85%) -> completed
    let t3_start = now_ms - 200_000;
    let a3 = sample_activity(t3_start, "spotify:track:comp2", "Stopped Late Song");
    db.insert_listen(&a3).unwrap();
    db.finalize_play(
        &a3.user.uri,
        &a3.track.uri,
        t3_start,
        t3_start + 170_000,
        Some(200_000),
        false,
    )
    .unwrap();

    // Track 4: 200s track, stopped after 40s -> unknown (None)
    let t4_start = now_ms - 100_000;
    let a4 = sample_activity(t4_start, "spotify:track:unk1", "Paused Early Song");
    db.insert_listen(&a4).unwrap();
    db.finalize_play(
        &a4.user.uri,
        &a4.track.uri,
        t4_start,
        t4_start + 40_000,
        Some(200_000),
        false,
    )
    .unwrap();

    let hist = db.query_history(Some("alice"), Some(1), None).unwrap();
    let by_uri: HashMap<_, _> = hist.iter().map(|r| (r.track_uri.as_str(), r)).collect();
    assert_eq!(by_uri["spotify:track:skip1"].was_skipped, Some(true));
    assert_eq!(by_uri["spotify:track:skip1"].listened_ms, Some(30_000));
    assert_eq!(by_uri["spotify:track:comp1"].was_skipped, Some(false));
    assert_eq!(by_uri["spotify:track:comp1"].listened_ms, Some(190_000));
    assert_eq!(by_uri["spotify:track:comp2"].was_skipped, Some(false));
    assert_eq!(by_uri["spotify:track:unk1"].was_skipped, None);

    // Sessions
    let sid = db
        .start_session("spotify:user:alice123", "Alice", now_ms - 600_000)
        .unwrap();
    db.touch_session(sid, now_ms - 400_000, true).unwrap();
    db.touch_session(sid, now_ms - 200_000, true).unwrap();
    db.end_session(sid, now_ms - 100_000).unwrap();

    let (tracks_played, ended_at): (i64, Option<i64>) = db
        .conn
        .query_row(
            "SELECT tracks_played, ended_at_ms FROM presence_sessions WHERE id = ?1",
            params![sid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(tracks_played, 2);
    assert_eq!(ended_at, Some(now_ms - 100_000));

    let stats = db.query_stats(Some("alice"), Some(1)).unwrap();
    assert_eq!(stats.skipped_plays, 1);
    assert_eq!(stats.completed_plays, 2);
    assert_eq!(stats.most_skipped_tracks.len(), 1);
    assert_eq!(stats.most_skipped_tracks[0].1, "Skipped Song");
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.avg_session_ms, Some(500_000));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rebuilds_hidden_playlists_and_lists_contexts() {
    let (dir, db_path) = temp_db_path("rebuild_contexts");
    let db = Database::open(&db_path).unwrap();
    let now_ms = Utc::now().timestamp_millis();

    let mut p1 = sample_activity(now_ms - 300_000, "spotify:track:AAA", "Secret Track A");
    p1.track.context = Some(TrackContext {
        uri: "spotify:playlist:secret123".to_string(),
        name: "Late Night Secret".to_string(),
        index: Some(0),
        context_type: "playlist".to_string(),
        owner: Some("alice123".to_string()),
        is_private: true,
    });
    let mut p2 = sample_activity(now_ms - 200_000, "spotify:track:AAA", "Secret Track A");
    p2.track.context = p1.track.context.clone();

    let mut p3 = sample_activity(now_ms - 100_000, "spotify:episode:EP99", "Secret Ep");
    p3.track.item_type = "episode".to_string();
    p3.track.context = p1.track.context.clone();

    let mut liked = sample_activity(now_ms - 50_000, "spotify:track:LIKED1", "Liked Track");
    liked.track.context = Some(TrackContext {
        uri: "spotify:user:alice123:collection".to_string(),
        name: "Liked Songs".to_string(),
        index: None,
        context_type: "liked_songs".to_string(),
        owner: Some("alice123".to_string()),
        is_private: true,
    });

    let public_pl = sample_activity(now_ms - 10_000, "spotify:track:PUB1", "Public Track");

    for a in [&p1, &p2, &p3, &liked, &public_pl] {
        db.insert_listen(a).unwrap();
    }

    let all_ctx = db.query_contexts(Some("alice"), false).unwrap();
    assert_eq!(all_ctx.len(), 3);

    let priv_ctx = db.query_contexts(Some("alice"), true).unwrap();
    assert_eq!(priv_ctx.len(), 2);
    assert_eq!(priv_ctx[0].context_name, "Late Night Secret");
    assert_eq!(priv_ctx[0].unique_tracks, 2);
    assert_eq!(priv_ctx[0].total_plays, 3);
    assert!(priv_ctx[0].is_private);

    let rebuilt = db.rebuild_playlist(Some("alice"), "secret123").unwrap();
    assert_eq!(rebuilt.len(), 2);
    assert_eq!(rebuilt[0].track_name, "Secret Track A");
    assert_eq!(rebuilt[0].play_count, 2);
    assert_eq!(rebuilt[0].spotify_url, "https://open.spotify.com/track/AAA");
    assert_eq!(rebuilt[1].track_name, "Secret Ep");
    assert_eq!(
        rebuilt[1].spotify_url,
        "https://open.spotify.com/episode/EP99"
    );

    let rebuilt_liked = db.rebuild_playlist(Some("alice"), "liked_songs").unwrap();
    assert_eq!(rebuilt_liked.len(), 1);
    assert_eq!(rebuilt_liked[0].track_name, "Liked Track");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tracks_profile_changes_and_public_playlists() {
    let (dir, db_path) = temp_db_path("profiles");
    let db = Database::open(&db_path).unwrap();

    let snap1 = UserProfileSnapshot {
        user_uri: "spotify:user:bob".to_string(),
        user_name: "Bob".to_string(),
        image_url: Some("https://img/bob1.jpg".to_string()),
        color: Some(12345),
        followers_count: Some(10),
        following_count: Some(25),
        public_playlists: vec![PublicPlaylist {
            uri: "spotify:playlist:pl1".to_string(),
            name: "Gym Mix".to_string(),
            owner_name: Some("Bob".to_string()),
            owner_uri: Some("spotify:user:bob".to_string()),
            followers_count: Some(5),
            image_url: None,
        }],
        recently_played_artists: vec![NamedUri {
            uri: "spotify:artist:a1".to_string(),
            name: "Daft Punk".to_string(),
        }],
    };

    let diff1 = db.upsert_profile(&snap1).unwrap();
    assert_eq!(diff1.renamed_from, None);
    assert!(diff1.new_playlists.is_empty());
    assert_eq!(diff1.followers_delta, None);

    let snap2 = UserProfileSnapshot {
        user_uri: "spotify:user:bob".to_string(),
        user_name: "Bobby".to_string(),
        image_url: Some("https://img/bob2.jpg".to_string()),
        color: Some(12345),
        followers_count: Some(14),
        following_count: Some(26),
        public_playlists: vec![
            PublicPlaylist {
                uri: "spotify:playlist:pl1".to_string(),
                name: "Gym Mix".to_string(),
                owner_name: Some("Bobby".to_string()),
                owner_uri: Some("spotify:user:bob".to_string()),
                followers_count: Some(7),
                image_url: None,
            },
            PublicPlaylist {
                uri: "spotify:playlist:pl2".to_string(),
                name: "Road Trip".to_string(),
                owner_name: Some("Bobby".to_string()),
                owner_uri: Some("spotify:user:bob".to_string()),
                followers_count: Some(1),
                image_url: None,
            },
        ],
        recently_played_artists: vec![
            NamedUri {
                uri: "spotify:artist:a1".to_string(),
                name: "Daft Punk".to_string(),
            },
            NamedUri {
                uri: "spotify:artist:a2".to_string(),
                name: "Justice".to_string(),
            },
        ],
    };

    let diff2 = db.upsert_profile(&snap2).unwrap();
    assert_eq!(diff2.renamed_from.as_deref(), Some("Bob"));
    assert_eq!(diff2.new_playlists, vec!["Road Trip".to_string()]);
    assert_eq!(diff2.followers_delta, Some(4));

    let profiles = db.query_profiles(Some("bobby")).unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].user_name, "Bobby");
    assert_eq!(profiles[0].followers_count, Some(14));
    assert_eq!(profiles[0].public_playlists.len(), 2);
    assert_eq!(profiles[0].recently_played_artists.len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn computes_enhanced_stats_and_streaks() {
    let (dir, db_path) = temp_db_path("enhanced_stats");
    let db = Database::open(&db_path).unwrap();

    let now = Local::now();
    let day0 = now.timestamp_millis();
    let day1 = (now - Duration::days(1)).timestamp_millis();
    let day2 = (now - Duration::days(2)).timestamp_millis();
    // gap on day 3
    let day4 = (now - Duration::days(4)).timestamp_millis();
    let day5 = (now - Duration::days(5)).timestamp_millis();

    let mut a0 = sample_activity(day0, "spotify:track:s0", "Mainstream Hit");
    a0.track.popularity = Some(95);
    a0.track.release_date = Some("2023-01-01".to_string());
    a0.track.explicit = Some(true);
    a0.track.duration_ms = Some(180_000);

    let mut a1 = sample_activity(day1, "spotify:track:s1", "Obscure Gem");
    a1.track.popularity = Some(12);
    a1.track.release_date = Some("1998-06-10".to_string());
    a1.track.explicit = Some(false);
    a1.track.duration_ms = Some(240_000);

    let mut a2 = sample_activity(day2, "spotify:track:s2", "Mid Track");
    a2.track.popularity = Some(50);
    a2.track.release_date = Some("2021-11-20".to_string());
    a2.track.explicit = Some(false);
    a2.track.duration_ms = Some(200_000);

    let mut a4 = sample_activity(day4, "spotify:track:s4", "Older Track 1");
    a4.track.popularity = Some(43);
    a4.track.release_date = Some("2015-03-01".to_string());
    a4.track.explicit = Some(false);
    a4.track.duration_ms = Some(210_000);

    let mut a5 = sample_activity(day5, "spotify:track:s5", "Older Track 2");
    a5.track.popularity = None;
    a5.track.release_date = None;
    a5.track.explicit = None;
    a5.track.duration_ms = None;

    for a in [&a0, &a1, &a2, &a4, &a5] {
        db.insert_listen(a).unwrap();
    }

    let stats = db.query_stats(Some("alice"), Some(30)).unwrap();
    assert_eq!(stats.total_plays, 5);
    assert_eq!(stats.total_listened_ms, 180_000 + 240_000 + 200_000 + 210_000);
    assert_eq!(stats.known_duration_plays, 4);
    assert_eq!(stats.explicit_plays, 1);
    assert_eq!(stats.avg_popularity, Some(50.0));
    assert_eq!(
        stats.most_obscure_track,
        Some(("Test Artist".to_string(), "Obscure Gem".to_string(), 12))
    );
    assert_eq!(
        stats.most_mainstream_track,
        Some(("Test Artist".to_string(), "Mainstream Hit".to_string(), 95))
    );
    assert_eq!(stats.release_decades[0], ("2020s".to_string(), 2));
    assert_eq!(stats.context_breakdown, vec![("playlist".to_string(), 5)]);
    assert_eq!(stats.active_days, 5);
    assert_eq!(stats.longest_streak_days, 3);
    assert_eq!(stats.current_streak_days, 3);
    assert_eq!(stats.weekday.iter().sum::<i64>(), 5);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn computes_friend_overlap_and_trendsetters() {
    let (dir, db_path) = temp_db_path("overlap");
    let db = Database::open(&db_path).unwrap();
    let now_ms = Utc::now().timestamp_millis();

    let make_play = |user_uri: &str,
                     user_name: &str,
                     artist: &str,
                     track_uri: &str,
                     track_name: &str,
                     ts: i64| {
        let mut a = sample_activity(ts, track_uri, track_name);
        a.user.uri = user_uri.to_string();
        a.user.name = user_name.to_string();
        a.track.artist = Some(NamedUri {
            uri: format!("spotify:artist:{artist}"),
            name: artist.to_string(),
        });
        a.track.artists = vec![NamedUri {
            uri: format!("spotify:artist:{artist}"),
            name: artist.to_string(),
        }];
        a
    };

    // Alice plays SharedTrack1 first, Bob plays it later, Carol plays it last
    // Bob plays SharedTrack2 first, Alice plays it later
    // Carol also shares ArtistB with Bob only
    let plays = vec![
        make_play(
            "spotify:user:alice",
            "Alice",
            "ArtistA",
            "spotify:track:shared1",
            "Shared One",
            now_ms - 500_000,
        ),
        make_play(
            "spotify:user:bob",
            "Bob",
            "ArtistA",
            "spotify:track:shared1",
            "Shared One",
            now_ms - 400_000,
        ),
        make_play(
            "spotify:user:carol",
            "Carol",
            "ArtistA",
            "spotify:track:shared1",
            "Shared One",
            now_ms - 300_000,
        ),
        make_play(
            "spotify:user:bob",
            "Bob",
            "ArtistB",
            "spotify:track:shared2",
            "Shared Two",
            now_ms - 250_000,
        ),
        make_play(
            "spotify:user:alice",
            "Alice",
            "ArtistB",
            "spotify:track:shared2",
            "Shared Two",
            now_ms - 200_000,
        ),
        make_play(
            "spotify:user:alice",
            "Alice",
            "ArtistA",
            "spotify:track:shared3",
            "Shared Three",
            now_ms - 150_000,
        ),
        make_play(
            "spotify:user:bob",
            "Bob",
            "ArtistA",
            "spotify:track:shared3",
            "Shared Three",
            now_ms - 100_000,
        ),
        make_play(
            "spotify:user:carol",
            "Carol",
            "ArtistC",
            "spotify:track:carol_only",
            "Carol Only",
            now_ms - 50_000,
        ),
    ];

    for p in &plays {
        db.insert_listen(p).unwrap();
    }

    let report = db.query_overlap(Some(7), &[]).unwrap();
    assert_eq!(report.friend_pairs.len(), 3);
    // Alice & Bob have identical artist & track sets -> compatibility 1.0
    assert_eq!(report.friend_pairs[0].friend_a, "Alice");
    assert_eq!(report.friend_pairs[0].friend_b, "Bob");
    assert!((report.friend_pairs[0].artist_jaccard - 1.0).abs() < 1e-9);
    assert!((report.friend_pairs[0].track_jaccard - 1.0).abs() < 1e-9);
    assert!((report.friend_pairs[0].compatibility_score - 1.0).abs() < 1e-9);

    assert_eq!(report.shared_artists.len(), 2);
    assert_eq!(report.shared_artists[0].0, "ArtistA");
    assert_eq!(report.shared_artists[0].1.len(), 3);

    assert_eq!(report.shared_tracks.len(), 3);
    assert_eq!(report.shared_tracks[0].track_name, "Shared One");
    assert_eq!(report.shared_tracks[0].first_player_name, "Alice");
    assert_eq!(report.shared_tracks[0].friends_who_played.len(), 3);

    // Alice played Shared One and Shared Three first (2); Bob played Shared Two first (1)
    assert_eq!(
        report.trendsetter_leaderboard,
        vec![("Alice".to_string(), 2), ("Bob".to_string(), 1)]
    );

    // Filtered overlap (only Alice & Carol)
    let filtered = db
        .query_overlap(Some(7), &["alice".to_string(), "carol".to_string()])
        .unwrap();
    assert_eq!(filtered.friend_pairs.len(), 1);
    assert_eq!(filtered.shared_tracks.len(), 1);
    assert_eq!(filtered.shared_tracks[0].track_name, "Shared One");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tracks_inferred_repeats() {
    let (dir, db_path) = temp_db_path("inferred_repeats");
    let db = Database::open(&db_path).unwrap();
    let now_ms = Utc::now().timestamp_millis();

    let base_ts = now_ms - 500_000;
    let a = sample_activity(base_ts, "spotify:track:loop1", "Looping Track");
    assert!(db.insert_listen(&a).unwrap());

    let cycle2_ts = base_ts + 200_000;
    assert!(db.insert_inferred_repeat(&a, cycle2_ts).unwrap());
    // Duplicate repeat cycle is ignored
    assert!(!db.insert_inferred_repeat(&a, cycle2_ts).unwrap());

    let hist = db.query_history(Some("alice"), Some(1), Some(10)).unwrap();
    assert_eq!(hist.len(), 2);
    assert_eq!(hist[0].played_at_ms, cycle2_ts);
    assert!(hist[0].inferred_repeat);
    assert_eq!(hist[1].played_at_ms, base_ts);
    assert!(!hist[1].inferred_repeat);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn populates_heatmap_grid_in_stats() {
    let (dir, db_path) = temp_db_path("heatmap_grid");
    let db = Database::open(&db_path).unwrap();
    let now_ms = Utc::now().timestamp_millis();

    let ts1 = now_ms - 60_000;
    let ts2 = now_ms - 30_000;
    let a1 = sample_activity(ts1, "spotify:track:h1", "Heat 1");
    let a2 = sample_activity(ts2, "spotify:track:h2", "Heat 2");
    db.insert_listen(&a1).unwrap();
    db.insert_listen(&a2).unwrap();

    let stats = db.query_stats(Some("alice"), Some(1)).unwrap();
    let total_cells: i64 = stats
        .heatmap_grid
        .iter()
        .flat_map(|row| row.iter())
        .sum();
    assert_eq!(total_cells, 2);

    for ts in [ts1, ts2] {
        let dt = millis_to_local(ts).unwrap();
        let wd = dt.weekday().num_days_from_monday() as usize;
        let hour = dt.hour() as usize;
        assert!(stats.heatmap_grid[wd][hour] >= 1);
        assert_eq!(stats.weekday[wd], stats.heatmap_grid[wd].iter().sum::<i64>());
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn records_profile_changelog_events() {
    let (dir, db_path) = temp_db_path("profile_events");
    let db = Database::open(&db_path).unwrap();

    let snap1 = UserProfileSnapshot {
        user_uri: "spotify:user:bob".to_string(),
        user_name: "Bob".to_string(),
        image_url: None,
        color: None,
        followers_count: Some(10),
        following_count: Some(20),
        public_playlists: vec![
            PublicPlaylist {
                uri: "spotify:playlist:pl1".to_string(),
                name: "Gym Mix".to_string(),
                owner_name: Some("Bob".to_string()),
                owner_uri: Some("spotify:user:bob".to_string()),
                followers_count: Some(5),
                image_url: None,
            },
            PublicPlaylist {
                uri: "spotify:playlist:pl_old".to_string(),
                name: "Old Mix".to_string(),
                owner_name: Some("Bob".to_string()),
                owner_uri: Some("spotify:user:bob".to_string()),
                followers_count: Some(1),
                image_url: None,
            },
        ],
        recently_played_artists: vec![NamedUri {
            uri: "spotify:artist:a1".to_string(),
            name: "Daft Punk".to_string(),
        }],
    };

    // First insert should not create changelog events.
    let diff1 = db.upsert_profile(&snap1).unwrap();
    assert!(diff1.renamed_from.is_none());
    assert!(diff1.new_playlists.is_empty());
    assert!(diff1.removed_playlists.is_empty());
    assert!(diff1.new_recent_artists.is_empty());
    assert!(db.query_profile_events(None, None, None).unwrap().is_empty());

    // Second snapshot: rename, follower delta -2, pl_old removed, pl2 added, new artist Justice.
    let snap2 = UserProfileSnapshot {
        user_uri: "spotify:user:bob".to_string(),
        user_name: "Bobby".to_string(),
        image_url: None,
        color: None,
        followers_count: Some(8),
        following_count: Some(20),
        public_playlists: vec![
            PublicPlaylist {
                uri: "spotify:playlist:pl1".to_string(),
                name: "Gym Mix".to_string(),
                owner_name: Some("Bobby".to_string()),
                owner_uri: Some("spotify:user:bob".to_string()),
                followers_count: Some(5),
                image_url: None,
            },
            PublicPlaylist {
                uri: "spotify:playlist:pl2".to_string(),
                name: "Road Trip".to_string(),
                owner_name: Some("Bobby".to_string()),
                owner_uri: Some("spotify:user:bob".to_string()),
                followers_count: Some(2),
                image_url: None,
            },
        ],
        recently_played_artists: vec![
            NamedUri {
                uri: "spotify:artist:a1".to_string(),
                name: "Daft Punk".to_string(),
            },
            NamedUri {
                uri: "spotify:artist:a2".to_string(),
                name: "Justice".to_string(),
            },
        ],
    };

    let diff2 = db.upsert_profile(&snap2).unwrap();
    assert_eq!(diff2.renamed_from.as_deref(), Some("Bob"));
    assert_eq!(diff2.followers_delta, Some(-2));
    assert_eq!(diff2.new_playlists, vec!["Road Trip".to_string()]);
    assert_eq!(diff2.removed_playlists, vec!["Old Mix".to_string()]);
    assert_eq!(diff2.new_recent_artists, vec!["Justice".to_string()]);

    let events = db.query_profile_events(Some("bobby"), Some(1), Some(20)).unwrap();
    assert_eq!(events.len(), 5);
    let types: HashSet<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(types.contains("rename"));
    assert!(types.contains("followers"));
    assert!(types.contains("playlist_added"));
    assert!(types.contains("playlist_removed"));
    assert!(types.contains("recent_artist"));

    // Re-upserting identical snapshot should produce no new diffs or events.
    let diff3 = db.upsert_profile(&snap2).unwrap();
    assert!(diff3.renamed_from.is_none());
    assert!(diff3.followers_delta.is_none());
    assert!(diff3.new_playlists.is_empty());
    assert!(diff3.removed_playlists.is_empty());
    assert!(diff3.new_recent_artists.is_empty());
    assert_eq!(db.query_profile_events(None, None, None).unwrap().len(), 5);

    let _ = std::fs::remove_dir_all(&dir);
}
