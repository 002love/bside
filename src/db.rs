use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::spotify::{FriendActivity, NamedUri, PublicPlaylist, UserProfileSnapshot};

const LISTENS_MIGRATIONS: &[(&str, &str)] = &[
    ("item_type", "TEXT NOT NULL DEFAULT 'track'"),
    ("all_artists", "TEXT"),
    ("duration_ms", "INTEGER"),
    ("popularity", "INTEGER"),
    ("explicit", "INTEGER"),
    ("track_number", "INTEGER"),
    ("disc_number", "INTEGER"),
    ("isrc", "TEXT"),
    ("album_type", "TEXT"),
    ("release_date", "TEXT"),
    ("album_label", "TEXT"),
    ("cover_url", "TEXT"),
    ("context_type", "TEXT"),
    ("context_owner", "TEXT"),
    ("context_is_private", "INTEGER NOT NULL DEFAULT 0"),
    ("is_playing_seen", "INTEGER NOT NULL DEFAULT 0"),
    ("ended_at_ms", "INTEGER"),
    ("listened_ms", "INTEGER"),
    ("was_skipped", "INTEGER"),
    ("inferred_repeat", "INTEGER NOT NULL DEFAULT 0"),
];

#[derive(Debug, Clone, Serialize)]
pub struct ListenRecord {
    pub id: i64,
    pub played_at_ms: i64,
    pub played_at_local: String,
    pub logged_at_ms: i64,
    pub user_uri: String,
    pub user_name: String,
    pub track_uri: String,
    pub track_name: String,
    pub artist_name: String,
    pub artist_uri: Option<String>,
    pub album_name: Option<String>,
    pub album_uri: Option<String>,
    pub context_name: Option<String>,
    pub context_uri: Option<String>,
    pub item_type: String,
    pub all_artists: Option<String>,
    pub duration_ms: Option<i64>,
    pub popularity: Option<i32>,
    pub explicit: Option<bool>,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    pub album_label: Option<String>,
    pub cover_url: Option<String>,
    pub isrc: Option<String>,
    pub context_type: Option<String>,
    pub context_owner: Option<String>,
    pub context_is_private: bool,
    pub ended_at_ms: Option<i64>,
    pub listened_ms: Option<i64>,
    pub was_skipped: Option<bool>,
    pub inferred_repeat: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StatsSummary {
    pub total_plays: i64,
    pub unique_tracks: i64,
    pub unique_artists: i64,
    pub first_played_ms: Option<i64>,
    pub last_played_ms: Option<i64>,
    pub top_tracks: Vec<(String, String, i64)>,
    pub top_artists: Vec<(String, i64)>,
    pub hourly: [i64; 24],
    pub total_listened_ms: i64,
    pub known_duration_plays: i64,
    pub skipped_plays: i64,
    pub completed_plays: i64,
    pub most_skipped_tracks: Vec<(String, String, i64)>,
    pub avg_popularity: Option<f64>,
    pub most_obscure_track: Option<(String, String, i32)>,
    pub most_mainstream_track: Option<(String, String, i32)>,
    pub explicit_plays: i64,
    pub release_decades: Vec<(String, i64)>,
    pub context_breakdown: Vec<(String, i64)>,
    pub active_days: i64,
    pub longest_streak_days: i64,
    pub current_streak_days: i64,
    pub weekday: [i64; 7],
    pub heatmap_grid: [[i64; 24]; 7],
    pub session_count: i64,
    pub avg_session_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[allow(dead_code)]
pub struct ProfileDiff {
    pub renamed_from: Option<String>,
    pub new_playlists: Vec<String>,
    pub removed_playlists: Vec<String>,
    pub new_recent_artists: Vec<String>,
    pub followers_delta: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct ProfileEvent {
    pub id: i64,
    pub detected_at_ms: i64,
    pub detected_at_local: String,
    pub user_uri: String,
    pub user_name: String,
    pub event_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct StoredProfile {
    pub user_uri: String,
    pub user_name: String,
    pub image_url: Option<String>,
    pub color: Option<i64>,
    pub followers_count: Option<i64>,
    pub following_count: Option<i64>,
    pub recently_played_artists: Vec<NamedUri>,
    pub public_playlists: Vec<PublicPlaylist>,
    pub first_seen_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct ContextSummary {
    pub user_uri: String,
    pub user_name: String,
    pub context_uri: String,
    pub context_name: String,
    pub context_type: String,
    pub context_owner: Option<String>,
    pub is_private: bool,
    pub unique_tracks: i64,
    pub total_plays: i64,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct RebuiltTrack {
    pub track_uri: String,
    pub track_name: String,
    pub all_artists: String,
    pub album_name: Option<String>,
    pub duration_ms: Option<i64>,
    pub release_date: Option<String>,
    pub popularity: Option<i32>,
    pub play_count: i64,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub spotify_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct FriendPairOverlap {
    pub friend_a: String,
    pub friend_b: String,
    pub shared_artists: usize,
    pub total_union_artists: usize,
    pub artist_jaccard: f64,
    pub shared_tracks: usize,
    pub total_union_tracks: usize,
    pub track_jaccard: f64,
    pub compatibility_score: f64,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct SharedItem {
    pub name: String,
    pub friends: Vec<String>,
    pub total_plays: i64,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct TrendsetterTrack {
    pub artist_name: String,
    pub track_name: String,
    pub track_uri: String,
    pub friends_who_played: Vec<String>,
    pub first_player_name: String,
    pub first_played_ms: i64,
    pub total_plays: i64,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct OverlapReport {
    pub friend_pairs: Vec<FriendPairOverlap>,
    pub shared_artists: Vec<(String, Vec<String>, i64)>,
    pub shared_tracks: Vec<TrendsetterTrack>,
    pub trendsetter_leaderboard: Vec<(String, i64)>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite database at {}", path.display()))?;

        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS listens (
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
                item_type TEXT NOT NULL DEFAULT 'track',
                all_artists TEXT,
                duration_ms INTEGER,
                popularity INTEGER,
                explicit INTEGER,
                track_number INTEGER,
                disc_number INTEGER,
                isrc TEXT,
                album_type TEXT,
                release_date TEXT,
                album_label TEXT,
                cover_url TEXT,
                context_type TEXT,
                context_owner TEXT,
                context_is_private INTEGER NOT NULL DEFAULT 0,
                is_playing_seen INTEGER NOT NULL DEFAULT 0,
                ended_at_ms INTEGER,
                listened_ms INTEGER,
                was_skipped INTEGER,
                inferred_repeat INTEGER NOT NULL DEFAULT 0,
                UNIQUE(user_uri, track_uri, played_at_ms)
            );

            CREATE TABLE IF NOT EXISTS presence_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_uri TEXT NOT NULL,
                user_name TEXT NOT NULL,
                started_at_ms INTEGER NOT NULL,
                last_seen_ms INTEGER NOT NULL,
                ended_at_ms INTEGER,
                tracks_played INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS friend_profiles (
                user_uri TEXT PRIMARY KEY,
                user_name TEXT NOT NULL,
                image_url TEXT,
                color INTEGER,
                followers_count INTEGER,
                following_count INTEGER,
                recently_played_artists_json TEXT,
                first_seen_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS friend_public_playlists (
                user_uri TEXT NOT NULL,
                playlist_uri TEXT NOT NULL,
                name TEXT NOT NULL,
                owner_name TEXT,
                owner_uri TEXT,
                followers_count INTEGER,
                image_url TEXT,
                first_seen_ms INTEGER NOT NULL,
                last_seen_ms INTEGER NOT NULL,
                PRIMARY KEY (user_uri, playlist_uri)
            );

            CREATE TABLE IF NOT EXISTS profile_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                detected_at_ms INTEGER NOT NULL,
                user_uri TEXT NOT NULL,
                user_name TEXT NOT NULL,
                event_type TEXT NOT NULL,
                summary TEXT NOT NULL
            );
            ",
        )?;

        migrate_listens_table(&conn)?;

        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_listens_user_time
                ON listens(user_uri, played_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_listens_time
                ON listens(played_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_listens_context
                ON listens(context_uri, context_type);
            CREATE INDEX IF NOT EXISTS idx_listens_track
                ON listens(track_uri);
            CREATE INDEX IF NOT EXISTS idx_sessions_user
                ON presence_sessions(user_uri, started_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_public_playlists_user
                ON friend_public_playlists(user_uri);
            CREATE INDEX IF NOT EXISTS idx_profile_events_time
                ON profile_events(detected_at_ms DESC);
            ",
        )?;

        Ok(Self { conn })
    }

    /// Inserts a listen event if it isn't already recorded.
    /// Returns `Ok(true)` if a new row was inserted, `Ok(false)` if it was a duplicate.
    /// If the row already exists and `activity.playing` is true, updates `is_playing_seen = 1`.
    pub fn insert_listen(&self, activity: &FriendActivity) -> Result<bool> {
        self.insert_listen_internal(activity, activity.timestamp, activity.playing, false)
    }

    /// Inserts an inferred repeat cycle for a track playing continuously beyond its duration.
    #[allow(dead_code)]
    pub fn insert_inferred_repeat(
        &self,
        activity: &FriendActivity,
        cycle_played_at_ms: i64,
    ) -> Result<bool> {
        self.insert_listen_internal(activity, cycle_played_at_ms, true, true)
    }

    fn insert_listen_internal(
        &self,
        activity: &FriendActivity,
        played_at_ms: i64,
        is_playing: bool,
        inferred_repeat: bool,
    ) -> Result<bool> {
        let now_ms = Utc::now().timestamp_millis();
        let all_artists = activity.all_artists_display();
        let explicit_val = activity.track.explicit.map(|b| if b { 1i64 } else { 0i64 });
        let cover_url = activity
            .track
            .cover_url
            .as_deref()
            .or(activity.track.image_url.as_deref());
        let context_is_private = if activity.is_private_context() { 1i64 } else { 0i64 };
        let is_playing_seen = if is_playing { 1i64 } else { 0i64 };
        let inferred_repeat_val = if inferred_repeat { 1i64 } else { 0i64 };

        let rows = self.conn.execute(
            "
            INSERT OR IGNORE INTO listens (
                played_at_ms, logged_at_ms,
                user_uri, user_name,
                track_uri, track_name,
                artist_name, artist_uri,
                album_name, album_uri,
                context_name, context_uri,
                item_type, all_artists,
                duration_ms, popularity, explicit,
                track_number, disc_number, isrc,
                album_type, release_date, album_label, cover_url,
                context_type, context_owner, context_is_private,
                is_playing_seen, inferred_repeat
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                ?25, ?26, ?27, ?28, ?29
            )
            ",
            params![
                played_at_ms,
                now_ms,
                activity.user.uri,
                activity.user.name,
                activity.track.uri,
                activity.track.name,
                activity.artist_name(),
                activity.artist_uri(),
                activity.album_name(),
                activity.album_uri(),
                activity.context_name(),
                activity.context_uri(),
                activity.item_type(),
                all_artists,
                activity.track.duration_ms,
                activity.track.popularity,
                explicit_val,
                activity.track.track_number,
                activity.track.disc_number,
                activity.track.isrc.as_deref(),
                activity.track.album_type.as_deref(),
                activity.track.release_date.as_deref(),
                activity.track.album_label.as_deref(),
                cover_url,
                activity.context_type(),
                activity.context_owner(),
                context_is_private,
                is_playing_seen,
                inferred_repeat_val,
            ],
        )?;

        if rows == 0 && is_playing {
            self.conn.execute(
                "
                UPDATE listens
                SET is_playing_seen = 1
                WHERE user_uri = ?1
                  AND track_uri = ?2
                  AND played_at_ms = ?3
                  AND is_playing_seen = 0
                ",
                params![activity.user.uri, activity.track.uri, played_at_ms],
            )?;
        }

        Ok(rows > 0)
    }

    #[allow(dead_code)]
    pub fn finalize_play(
        &self,
        user_uri: &str,
        track_uri: &str,
        played_at_ms: i64,
        ended_at_ms: i64,
        duration_ms: Option<i64>,
        switched_to_next_track: bool,
    ) -> Result<()> {
        let elapsed_ms = (ended_at_ms - played_at_ms).max(0);
        let listened_ms = match duration_ms {
            Some(d) if d > 0 => Some(elapsed_ms.min(d)),
            _ => Some(elapsed_ms),
        };

        let was_skipped: Option<i64> = if switched_to_next_track {
            match duration_ms {
                Some(d) if d >= 30_000 => {
                    let threshold = (d / 2).min(d - 15_000);
                    if elapsed_ms < threshold {
                        Some(1)
                    } else {
                        Some(0)
                    }
                }
                _ => None,
            }
        } else {
            match duration_ms {
                Some(d) if d > 0 && elapsed_ms >= (d * 80 / 100) => Some(0),
                _ => None,
            }
        };

        self.conn.execute(
            "
            UPDATE listens
            SET ended_at_ms = ?1,
                listened_ms = ?2,
                was_skipped = ?3
            WHERE user_uri = ?4
              AND track_uri = ?5
              AND played_at_ms = ?6
            ",
            params![
                ended_at_ms,
                listened_ms,
                was_skipped,
                user_uri,
                track_uri,
                played_at_ms
            ],
        )?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn start_session(&self, user_uri: &str, user_name: &str, started_at_ms: i64) -> Result<i64> {
        self.conn.execute(
            "
            INSERT INTO presence_sessions (
                user_uri, user_name, started_at_ms, last_seen_ms, tracks_played
            ) VALUES (?1, ?2, ?3, ?3, 0)
            ",
            params![user_uri, user_name, started_at_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    #[allow(dead_code)]
    pub fn touch_session(
        &self,
        session_id: i64,
        last_seen_ms: i64,
        increment_tracks: bool,
    ) -> Result<()> {
        let inc = if increment_tracks { 1i64 } else { 0i64 };
        self.conn.execute(
            "
            UPDATE presence_sessions
            SET last_seen_ms = MAX(last_seen_ms, ?1),
                tracks_played = tracks_played + ?2
            WHERE id = ?3
            ",
            params![last_seen_ms, inc, session_id],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn end_session(&self, session_id: i64, ended_at_ms: i64) -> Result<()> {
        self.conn.execute(
            "
            UPDATE presence_sessions
            SET ended_at_ms = ?1,
                last_seen_ms = MAX(last_seen_ms, ?1)
            WHERE id = ?2
            ",
            params![ended_at_ms, session_id],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn upsert_profile(&self, snapshot: &UserProfileSnapshot) -> Result<ProfileDiff> {
        let now_ms = Utc::now().timestamp_millis();
        let existing: Option<(String, Option<i64>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT user_name, followers_count, recently_played_artists_json FROM friend_profiles WHERE user_uri = ?1",
                params![snapshot.user_uri],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let mut diff = ProfileDiff::default();
        if let Some((old_name, old_followers, old_artists_json)) = &existing {
            if !old_name.is_empty() && old_name != &snapshot.user_name {
                diff.renamed_from = Some(old_name.clone());
            }
            if let (Some(old_f), Some(new_f)) = (*old_followers, snapshot.followers_count)
                && old_f != new_f
            {
                diff.followers_delta = Some(new_f - old_f);
            }

            let old_artists: Vec<NamedUri> = old_artists_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Vec<NamedUri>>(s).ok())
                .unwrap_or_default();
            let old_artist_keys: HashSet<String> = old_artists
                .iter()
                .map(|a| {
                    if !a.uri.is_empty() {
                        a.uri.clone()
                    } else {
                        a.name.to_lowercase()
                    }
                })
                .collect();
            let mut seen_new_artists: HashSet<String> = HashSet::new();
            for artist in &snapshot.recently_played_artists {
                let key = if !artist.uri.is_empty() {
                    artist.uri.clone()
                } else {
                    artist.name.to_lowercase()
                };
                if !key.is_empty()
                    && !old_artist_keys.contains(&key)
                    && seen_new_artists.insert(key)
                {
                    let label = if !artist.name.is_empty() {
                        artist.name.clone()
                    } else {
                        artist.uri.clone()
                    };
                    diff.new_recent_artists.push(label);
                }
            }
        }

        let artists_json = serde_json::to_string(&snapshot.recently_played_artists)?;

        self.conn.execute(
            "
            INSERT INTO friend_profiles (
                user_uri, user_name, image_url, color,
                followers_count, following_count,
                recently_played_artists_json, first_seen_ms, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
            ON CONFLICT(user_uri) DO UPDATE SET
                user_name = excluded.user_name,
                image_url = COALESCE(excluded.image_url, friend_profiles.image_url),
                color = COALESCE(excluded.color, friend_profiles.color),
                followers_count = COALESCE(excluded.followers_count, friend_profiles.followers_count),
                following_count = COALESCE(excluded.following_count, friend_profiles.following_count),
                recently_played_artists_json = excluded.recently_played_artists_json,
                updated_at_ms = excluded.updated_at_ms
            ",
            params![
                snapshot.user_uri,
                snapshot.user_name,
                snapshot.image_url,
                snapshot.color,
                snapshot.followers_count,
                snapshot.following_count,
                artists_json,
                now_ms,
            ],
        )?;

        let mut existing_playlists: HashMap<String, String> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT playlist_uri, name FROM friend_public_playlists WHERE user_uri = ?1",
            )?;
            let rows = stmt.query_map(params![snapshot.user_uri], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for r in rows {
                let (uri, name) = r?;
                existing_playlists.insert(uri, name);
            }
        }

        let mut incoming_playlist_uris: HashSet<String> = HashSet::new();
        for pl in &snapshot.public_playlists {
            if pl.uri.is_empty() {
                continue;
            }
            incoming_playlist_uris.insert(pl.uri.clone());
            if existing.is_some() && !existing_playlists.contains_key(&pl.uri) {
                diff.new_playlists.push(pl.name.clone());
            }

            self.conn.execute(
                "
                INSERT INTO friend_public_playlists (
                    user_uri, playlist_uri, name, owner_name, owner_uri,
                    followers_count, image_url, first_seen_ms, last_seen_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                ON CONFLICT(user_uri, playlist_uri) DO UPDATE SET
                    name = excluded.name,
                    owner_name = COALESCE(excluded.owner_name, friend_public_playlists.owner_name),
                    owner_uri = COALESCE(excluded.owner_uri, friend_public_playlists.owner_uri),
                    followers_count = COALESCE(excluded.followers_count, friend_public_playlists.followers_count),
                    image_url = COALESCE(excluded.image_url, friend_public_playlists.image_url),
                    last_seen_ms = excluded.last_seen_ms
                ",
                params![
                    snapshot.user_uri,
                    pl.uri,
                    pl.name,
                    pl.owner_name,
                    pl.owner_uri,
                    pl.followers_count,
                    pl.image_url,
                    now_ms,
                ],
            )?;
        }

        if existing.is_some() {
            let mut removed: Vec<(String, String)> = existing_playlists
                .into_iter()
                .filter(|(uri, _)| !incoming_playlist_uris.contains(uri))
                .collect();
            removed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            for (uri, name) in removed {
                let label = if !name.is_empty() { name } else { uri.clone() };
                diff.removed_playlists.push(label);
                self.conn.execute(
                    "DELETE FROM friend_public_playlists WHERE user_uri = ?1 AND playlist_uri = ?2",
                    params![snapshot.user_uri, uri],
                )?;
            }

            // Record profile changelog events for already-seen users.
            if let Some(old_name) = &diff.renamed_from {
                self.insert_profile_event(
                    now_ms,
                    &snapshot.user_uri,
                    &snapshot.user_name,
                    "rename",
                    &format!("Renamed from '{}' to '{}'", old_name, snapshot.user_name),
                )?;
            }
            if let Some(delta) = diff.followers_delta
                && delta != 0
            {
                let summary = match snapshot.followers_count {
                    Some(total) => format!("Followers {delta:+} (now {total})"),
                    None => format!("Followers {delta:+}"),
                };
                self.insert_profile_event(
                    now_ms,
                    &snapshot.user_uri,
                    &snapshot.user_name,
                    "followers",
                    &summary,
                )?;
            }
            for pl_name in &diff.new_playlists {
                self.insert_profile_event(
                    now_ms,
                    &snapshot.user_uri,
                    &snapshot.user_name,
                    "playlist_added",
                    &format!("Added public playlist '{pl_name}'"),
                )?;
            }
            for pl_name in &diff.removed_playlists {
                self.insert_profile_event(
                    now_ms,
                    &snapshot.user_uri,
                    &snapshot.user_name,
                    "playlist_removed",
                    &format!("Removed public playlist '{pl_name}'"),
                )?;
            }
            for artist in &diff.new_recent_artists {
                self.insert_profile_event(
                    now_ms,
                    &snapshot.user_uri,
                    &snapshot.user_name,
                    "recent_artist",
                    &format!("New recently played artist: {artist}"),
                )?;
            }
        }

        Ok(diff)
    }

    fn insert_profile_event(
        &self,
        detected_at_ms: i64,
        user_uri: &str,
        user_name: &str,
        event_type: &str,
        summary: &str,
    ) -> Result<()> {
        self.conn.execute(
            "
            INSERT INTO profile_events (
                detected_at_ms, user_uri, user_name, event_type, summary
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![detected_at_ms, user_uri, user_name, event_type, summary],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn query_profile_events(
        &self,
        friend_filter: Option<&str>,
        days: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<ProfileEvent>> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let filter_pattern = friend_filter
            .map(|f| format!("%{}%", f.trim().to_lowercase()))
            .unwrap_or_else(|| "%".to_string());
        let limit_val = limit.map(|l| l as i64).unwrap_or(-1);

        let mut stmt = self.conn.prepare(
            "
            SELECT id, detected_at_ms, user_uri, user_name, event_type, summary
            FROM profile_events
            WHERE detected_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            ORDER BY detected_at_ms DESC, id DESC
            LIMIT ?3
            ",
        )?;

        let rows = stmt
            .query_map(params![since_ms, filter_pattern, limit_val], |row| {
                let detected_at_ms: i64 = row.get(1)?;
                Ok(ProfileEvent {
                    id: row.get(0)?,
                    detected_at_ms,
                    detected_at_local: format_timestamp_local(detected_at_ms),
                    user_uri: row.get(2)?,
                    user_name: row.get(3)?,
                    event_type: row.get(4)?,
                    summary: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    #[allow(dead_code)]
    pub fn query_profiles(&self, friend_filter: Option<&str>) -> Result<Vec<StoredProfile>> {
        let filter_pattern = friend_filter
            .map(|f| format!("%{}%", f.trim().to_lowercase()))
            .unwrap_or_else(|| "%".to_string());

        let mut stmt = self.conn.prepare(
            "
            SELECT
                user_uri, user_name, image_url, color,
                followers_count, following_count,
                recently_played_artists_json, first_seen_ms, updated_at_ms
            FROM friend_profiles
            WHERE LOWER(user_name) LIKE ?1 OR LOWER(user_uri) LIKE ?1
            ORDER BY user_name COLLATE NOCASE ASC
            ",
        )?;

        let raw_profiles = stmt
            .query_map(params![filter_pattern], |row| {
                let artists_json: Option<String> = row.get(6)?;
                let recently_played_artists = artists_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<NamedUri>>(s).ok())
                    .unwrap_or_default();
                Ok(StoredProfile {
                    user_uri: row.get(0)?,
                    user_name: row.get(1)?,
                    image_url: row.get(2)?,
                    color: row.get(3)?,
                    followers_count: row.get(4)?,
                    following_count: row.get(5)?,
                    recently_played_artists,
                    public_playlists: Vec::new(),
                    first_seen_ms: row.get(7)?,
                    updated_at_ms: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut pl_stmt = self.conn.prepare(
            "
            SELECT playlist_uri, name, owner_name, owner_uri, followers_count, image_url
            FROM friend_public_playlists
            WHERE user_uri = ?1
            ORDER BY last_seen_ms DESC, name COLLATE NOCASE ASC
            ",
        )?;

        let mut profiles = Vec::with_capacity(raw_profiles.len());
        for mut profile in raw_profiles {
            let playlists = pl_stmt
                .query_map(params![profile.user_uri], |row| {
                    Ok(PublicPlaylist {
                        uri: row.get(0)?,
                        name: row.get(1)?,
                        owner_name: row.get(2)?,
                        owner_uri: row.get(3)?,
                        followers_count: row.get(4)?,
                        image_url: row.get(5)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            profile.public_playlists = playlists;
            profiles.push(profile);
        }

        Ok(profiles)
    }

    #[allow(dead_code)]
    pub fn query_contexts(
        &self,
        friend_filter: Option<&str>,
        private_only: bool,
    ) -> Result<Vec<ContextSummary>> {
        let filter_pattern = friend_filter
            .map(|f| format!("%{}%", f.trim().to_lowercase()))
            .unwrap_or_else(|| "%".to_string());
        let private_flag = if private_only { 1i64 } else { 0i64 };

        let mut stmt = self.conn.prepare(
            "
            SELECT
                user_uri,
                MAX(user_name) AS user_name,
                COALESCE(context_uri, '') AS ctx_uri,
                COALESCE(MAX(NULLIF(context_name, '')), '') AS ctx_name,
                COALESCE(MAX(NULLIF(context_type, '')), 'other') AS ctx_type,
                MAX(NULLIF(context_owner, '')) AS ctx_owner,
                MAX(context_is_private) AS is_priv,
                COUNT(DISTINCT track_uri) AS unique_tracks,
                COUNT(*) AS total_plays,
                MIN(played_at_ms) AS first_seen_ms,
                MAX(played_at_ms) AS last_seen_ms
            FROM listens
            WHERE (LOWER(user_name) LIKE ?1 OR LOWER(user_uri) LIKE ?1)
              AND (
                  (context_uri IS NOT NULL AND context_uri != '')
                  OR (context_name IS NOT NULL AND context_name != '')
              )
              AND (?2 = 0 OR context_is_private = 1)
            GROUP BY user_uri, COALESCE(NULLIF(context_uri, ''), context_name)
            ORDER BY total_plays DESC, last_seen_ms DESC
            ",
        )?;

        let rows = stmt
            .query_map(params![filter_pattern, private_flag], |row| {
                let is_priv: i64 = row.get(6)?;
                Ok(ContextSummary {
                    user_uri: row.get(0)?,
                    user_name: row.get(1)?,
                    context_uri: row.get(2)?,
                    context_name: row.get(3)?,
                    context_type: row.get(4)?,
                    context_owner: row.get(5)?,
                    is_private: is_priv != 0,
                    unique_tracks: row.get(7)?,
                    total_plays: row.get(8)?,
                    first_seen_ms: row.get(9)?,
                    last_seen_ms: row.get(10)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    #[allow(dead_code)]
    pub fn rebuild_playlist(
        &self,
        friend_filter: Option<&str>,
        context_query: &str,
    ) -> Result<Vec<RebuiltTrack>> {
        let filter_pattern = friend_filter
            .map(|f| format!("%{}%", f.trim().to_lowercase()))
            .unwrap_or_else(|| "%".to_string());
        let ctx_pattern = format!("%{}%", context_query.trim().to_lowercase());

        let mut stmt = self.conn.prepare(
            "
            SELECT
                track_uri,
                track_name,
                COALESCE(NULLIF(all_artists, ''), artist_name) AS all_artists_display,
                album_name,
                duration_ms,
                release_date,
                popularity,
                COUNT(*) AS play_count,
                MIN(played_at_ms) AS first_seen_ms,
                MAX(played_at_ms) AS last_seen_ms
            FROM listens
            WHERE (LOWER(user_name) LIKE ?1 OR LOWER(user_uri) LIKE ?1)
              AND (
                  LOWER(COALESCE(context_uri, '')) LIKE ?2
                  OR LOWER(COALESCE(context_name, '')) LIKE ?2
                  OR LOWER(COALESCE(context_type, '')) LIKE ?2
              )
            GROUP BY track_uri
            ORDER BY play_count DESC, last_seen_ms DESC
            ",
        )?;

        let rows = stmt
            .query_map(params![filter_pattern, ctx_pattern], |row| {
                let track_uri: String = row.get(0)?;
                let spotify_url = spotify_uri_to_url(&track_uri);
                Ok(RebuiltTrack {
                    track_uri,
                    track_name: row.get(1)?,
                    all_artists: row.get(2)?,
                    album_name: row.get(3)?,
                    duration_ms: row.get(4)?,
                    release_date: row.get(5)?,
                    popularity: row.get(6)?,
                    play_count: row.get(7)?,
                    first_seen_ms: row.get(8)?,
                    last_seen_ms: row.get(9)?,
                    spotify_url,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn query_history(
        &self,
        friend_filter: Option<&str>,
        days: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<ListenRecord>> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let filter_pattern = friend_filter
            .map(|f| format!("%{}%", f.trim().to_lowercase()))
            .unwrap_or_else(|| "%".to_string());
        let limit_val = limit.map(|l| l as i64).unwrap_or(-1);

        let mut stmt = self.conn.prepare(
            "
            SELECT
                id, played_at_ms, logged_at_ms,
                user_uri, user_name,
                track_uri, track_name,
                artist_name, artist_uri,
                album_name, album_uri,
                context_name, context_uri,
                item_type, all_artists,
                duration_ms, popularity, explicit,
                album_type, release_date, album_label, cover_url, isrc,
                context_type, context_owner, context_is_private,
                ended_at_ms, listened_ms, was_skipped, inferred_repeat
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            ORDER BY played_at_ms DESC
            LIMIT ?3
            ",
        )?;

        let rows = stmt.query_map(params![since_ms, filter_pattern, limit_val], |row| {
            let played_at_ms: i64 = row.get(1)?;
            let explicit_int: Option<i64> = row.get(17)?;
            let ctx_priv_int: i64 = row.get(25)?;
            let skipped_int: Option<i64> = row.get(28)?;
            let inferred_repeat_int: i64 = row.get(29)?;
            Ok(ListenRecord {
                id: row.get(0)?,
                played_at_ms,
                played_at_local: format_timestamp_local(played_at_ms),
                logged_at_ms: row.get(2)?,
                user_uri: row.get(3)?,
                user_name: row.get(4)?,
                track_uri: row.get(5)?,
                track_name: row.get(6)?,
                artist_name: row.get(7)?,
                artist_uri: row.get(8)?,
                album_name: row.get(9)?,
                album_uri: row.get(10)?,
                context_name: row.get(11)?,
                context_uri: row.get(12)?,
                item_type: row.get(13)?,
                all_artists: row.get(14)?,
                duration_ms: row.get(15)?,
                popularity: row.get(16)?,
                explicit: explicit_int.map(|v| v != 0),
                album_type: row.get(18)?,
                release_date: row.get(19)?,
                album_label: row.get(20)?,
                cover_url: row.get(21)?,
                isrc: row.get(22)?,
                context_type: row.get(23)?,
                context_owner: row.get(24)?,
                context_is_private: ctx_priv_int != 0,
                ended_at_ms: row.get(26)?,
                listened_ms: row.get(27)?,
                was_skipped: skipped_int.map(|v| v != 0),
                inferred_repeat: inferred_repeat_int != 0,
            })
        })?;

        let mut records = Vec::new();
        for r in rows {
            records.push(r?);
        }
        Ok(records)
    }

    pub fn query_stats(
        &self,
        friend_filter: Option<&str>,
        days: Option<i64>,
    ) -> Result<StatsSummary> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let filter_pattern = friend_filter
            .map(|f| format!("%{}%", f.trim().to_lowercase()))
            .unwrap_or_else(|| "%".to_string());

        let (
            total_plays,
            unique_tracks,
            unique_artists,
            first_played_ms,
            last_played_ms,
            total_listened_ms,
            known_duration_plays,
            skipped_plays,
            completed_plays,
            avg_popularity,
            explicit_plays,
        ): (
            i64,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
            i64,
            i64,
            i64,
            i64,
            Option<f64>,
            i64,
        ) = self.conn.query_row(
            "
            SELECT
                COUNT(*),
                COUNT(DISTINCT track_uri),
                COUNT(DISTINCT artist_name),
                MIN(played_at_ms),
                MAX(played_at_ms),
                COALESCE(SUM(COALESCE(listened_ms, duration_ms, 0)), 0),
                COALESCE(SUM(CASE WHEN listened_ms IS NOT NULL OR duration_ms IS NOT NULL THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN was_skipped = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN was_skipped = 0 THEN 1 ELSE 0 END), 0),
                AVG(CAST(popularity AS REAL)),
                COALESCE(SUM(CASE WHEN explicit = 1 THEN 1 ELSE 0 END), 0)
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            ",
            params![since_ms, filter_pattern],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ))
            },
        )?;

        let mut top_tracks_stmt = self.conn.prepare(
            "
            SELECT artist_name, track_name, COUNT(*) as cnt
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            GROUP BY track_uri
            ORDER BY cnt DESC, MAX(played_at_ms) DESC
            LIMIT 10
            ",
        )?;
        let top_tracks = top_tracks_stmt
            .query_map(params![since_ms, filter_pattern], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut top_artists_stmt = self.conn.prepare(
            "
            SELECT artist_name, COUNT(*) as cnt
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            GROUP BY artist_name
            ORDER BY cnt DESC, MAX(played_at_ms) DESC
            LIMIT 10
            ",
        )?;
        let top_artists = top_artists_stmt
            .query_map(params![since_ms, filter_pattern], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut most_skipped_stmt = self.conn.prepare(
            "
            SELECT artist_name, track_name, COUNT(*) as cnt
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
              AND was_skipped = 1
            GROUP BY track_uri
            ORDER BY cnt DESC, MAX(played_at_ms) DESC
            LIMIT 10
            ",
        )?;
        let most_skipped_tracks = most_skipped_stmt
            .query_map(params![since_ms, filter_pattern], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let most_obscure_track: Option<(String, String, i32)> = self
            .conn
            .query_row(
                "
                SELECT artist_name, track_name, popularity
                FROM listens
                WHERE played_at_ms >= ?1
                  AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
                  AND popularity IS NOT NULL
                ORDER BY popularity ASC, played_at_ms DESC
                LIMIT 1
                ",
                params![since_ms, filter_pattern],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let most_mainstream_track: Option<(String, String, i32)> = self
            .conn
            .query_row(
                "
                SELECT artist_name, track_name, popularity
                FROM listens
                WHERE played_at_ms >= ?1
                  AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
                  AND popularity IS NOT NULL
                ORDER BY popularity DESC, played_at_ms DESC
                LIMIT 1
                ",
                params![since_ms, filter_pattern],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let mut decade_counts: HashMap<i32, i64> = HashMap::new();
        {
            let mut dec_stmt = self.conn.prepare(
                "
                SELECT release_date
                FROM listens
                WHERE played_at_ms >= ?1
                  AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
                  AND release_date IS NOT NULL
                  AND release_date != ''
                ",
            )?;
            let rows = dec_stmt
                .query_map(params![since_ms, filter_pattern], |row| row.get::<_, String>(0))?;
            for r in rows {
                let date_str = r?;
                if let Some(year) = date_str.get(0..4).and_then(|y| y.parse::<i32>().ok())
                    && (1900..=2100).contains(&year)
                {
                    let decade = (year / 10) * 10;
                    *decade_counts.entry(decade).or_insert(0) += 1;
                }
            }
        }
        let mut release_decades: Vec<(i32, i64)> = decade_counts.into_iter().collect();
        release_decades.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        let release_decades: Vec<(String, i64)> = release_decades
            .into_iter()
            .map(|(dec, cnt)| (format!("{dec}s"), cnt))
            .collect();

        let mut ctx_stmt = self.conn.prepare(
            "
            SELECT COALESCE(NULLIF(context_type, ''), 'unknown') AS ct, COUNT(*) as cnt
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            GROUP BY ct
            ORDER BY cnt DESC, ct ASC
            ",
        )?;
        let context_breakdown = ctx_stmt
            .query_map(params![since_ms, filter_pattern], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut hourly = [0i64; 24];
        let mut weekday = [0i64; 7];
        let mut heatmap_grid = [[0i64; 24]; 7];
        let mut active_date_set: BTreeSet<NaiveDate> = BTreeSet::new();
        {
            let mut ts_stmt = self.conn.prepare(
                "
                SELECT played_at_ms
                FROM listens
                WHERE played_at_ms >= ?1
                  AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
                ",
            )?;
            let ts_rows =
                ts_stmt.query_map(params![since_ms, filter_pattern], |row| row.get::<_, i64>(0))?;
            for ts in ts_rows {
                let ms = ts?;
                if let Some(dt) = millis_to_local(ms) {
                    let hour = dt.hour() as usize;
                    let wd = dt.weekday().num_days_from_monday() as usize;
                    if hour < 24 {
                        hourly[hour] += 1;
                    }
                    if wd < 7 {
                        weekday[wd] += 1;
                    }
                    if wd < 7 && hour < 24 {
                        heatmap_grid[wd][hour] += 1;
                    }
                    active_date_set.insert(dt.date_naive());
                }
            }
        }

        let active_days = active_date_set.len() as i64;
        let (longest_streak_days, current_streak_days) =
            compute_streaks(&active_date_set, Local::now().date_naive());

        let (session_count, avg_session_f64): (i64, Option<f64>) = self.conn.query_row(
            "
            SELECT
                COUNT(*),
                AVG(CAST(
                    CASE
                        WHEN COALESCE(ended_at_ms, last_seen_ms) > started_at_ms
                        THEN COALESCE(ended_at_ms, last_seen_ms) - started_at_ms
                        ELSE 0
                    END
                AS REAL))
            FROM presence_sessions
            WHERE started_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            ",
            params![since_ms, filter_pattern],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let avg_session_ms = avg_session_f64.map(|v| v.round() as i64);

        Ok(StatsSummary {
            total_plays,
            unique_tracks,
            unique_artists,
            first_played_ms,
            last_played_ms,
            top_tracks,
            top_artists,
            hourly,
            total_listened_ms,
            known_duration_plays,
            skipped_plays,
            completed_plays,
            most_skipped_tracks,
            avg_popularity,
            most_obscure_track,
            most_mainstream_track,
            explicit_plays,
            release_decades,
            context_breakdown,
            active_days,
            longest_streak_days,
            current_streak_days,
            weekday,
            heatmap_grid,
            session_count,
            avg_session_ms,
        })
    }

    #[allow(dead_code)]
    pub fn query_overlap(
        &self,
        days: Option<i64>,
        friends_filter: &[String],
    ) -> Result<OverlapReport> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);

        let normalized_filters: Vec<String> = friends_filter
            .iter()
            .map(|f| f.trim().to_lowercase())
            .filter(|f| !f.is_empty())
            .collect();

        let mut stmt = self.conn.prepare(
            "
            SELECT user_uri, user_name, artist_name, track_uri, track_name, played_at_ms
            FROM listens
            WHERE played_at_ms >= ?1
            ORDER BY played_at_ms ASC
            ",
        )?;

        let mut friend_names: HashMap<String, String> = HashMap::new();
        let mut friend_artists: HashMap<String, HashSet<String>> = HashMap::new();
        let mut friend_tracks: HashMap<String, HashSet<String>> = HashMap::new();

        // artist_name -> (set of user_uris, total plays)
        let mut artist_agg: HashMap<String, (BTreeSet<String>, i64)> = HashMap::new();

        // track_uri -> (artist_name, track_name, set of user_uris, first_user_uri, first_played_ms, total_plays)
        struct TrackAgg {
            artist_name: String,
            track_name: String,
            users: BTreeSet<String>,
            first_user_uri: String,
            first_played_ms: i64,
            total_plays: i64,
        }
        let mut track_agg: HashMap<String, TrackAgg> = HashMap::new();

        let rows = stmt.query_map(params![since_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        for row in rows {
            let (user_uri, user_name, artist_name, track_uri, track_name, played_at_ms) = row?;

            if !normalized_filters.is_empty() {
                let uri_lc = user_uri.to_lowercase();
                let name_lc = user_name.to_lowercase();
                let matches = normalized_filters
                    .iter()
                    .any(|f| uri_lc.contains(f) || name_lc.contains(f));
                if !matches {
                    continue;
                }
            }

            // Rows are ordered by played_at_ms ASC, so inserting keeps the latest known display name.
            friend_names.insert(user_uri.clone(), user_name);
            friend_artists
                .entry(user_uri.clone())
                .or_default()
                .insert(artist_name.clone());
            friend_tracks
                .entry(user_uri.clone())
                .or_default()
                .insert(track_uri.clone());

            let artist_entry = artist_agg
                .entry(artist_name.clone())
                .or_insert_with(|| (BTreeSet::new(), 0));
            artist_entry.0.insert(user_uri.clone());
            artist_entry.1 += 1;

            let track_entry = track_agg.entry(track_uri).or_insert_with(|| TrackAgg {
                artist_name,
                track_name,
                users: BTreeSet::new(),
                first_user_uri: user_uri.clone(),
                first_played_ms: played_at_ms,
                total_plays: 0,
            });
            track_entry.users.insert(user_uri.clone());
            if played_at_ms < track_entry.first_played_ms {
                track_entry.first_played_ms = played_at_ms;
                track_entry.first_user_uri = user_uri;
            }
            track_entry.total_plays += 1;
        }

        let mut user_uris: Vec<String> = friend_names.keys().cloned().collect();
        user_uris.sort_by(|a, b| {
            let na = friend_names.get(a).map(String::as_str).unwrap_or("");
            let nb = friend_names.get(b).map(String::as_str).unwrap_or("");
            na.to_lowercase()
                .cmp(&nb.to_lowercase())
                .then_with(|| a.cmp(b))
        });

        let mut friend_pairs = Vec::new();
        for i in 0..user_uris.len() {
            for j in (i + 1)..user_uris.len() {
                let ua = &user_uris[i];
                let ub = &user_uris[j];
                let artists_a = &friend_artists[ua];
                let artists_b = &friend_artists[ub];
                let shared_a = artists_a.intersection(artists_b).count();
                let union_a = artists_a.union(artists_b).count();
                let artist_jaccard = if union_a > 0 {
                    shared_a as f64 / union_a as f64
                } else {
                    0.0
                };

                let tracks_a = &friend_tracks[ua];
                let tracks_b = &friend_tracks[ub];
                let shared_t = tracks_a.intersection(tracks_b).count();
                let union_t = tracks_a.union(tracks_b).count();
                let track_jaccard = if union_t > 0 {
                    shared_t as f64 / union_t as f64
                } else {
                    0.0
                };

                let compatibility_score = if union_t > 0 {
                    0.6 * artist_jaccard + 0.4 * track_jaccard
                } else {
                    artist_jaccard
                };

                friend_pairs.push(FriendPairOverlap {
                    friend_a: friend_names[ua].clone(),
                    friend_b: friend_names[ub].clone(),
                    shared_artists: shared_a,
                    total_union_artists: union_a,
                    artist_jaccard,
                    shared_tracks: shared_t,
                    total_union_tracks: union_t,
                    track_jaccard,
                    compatibility_score,
                });
            }
        }
        friend_pairs.sort_by(|a, b| {
            b.compatibility_score
                .total_cmp(&a.compatibility_score)
                .then_with(|| b.shared_tracks.cmp(&a.shared_tracks))
                .then_with(|| b.shared_artists.cmp(&a.shared_artists))
                .then_with(|| a.friend_a.cmp(&b.friend_a))
                .then_with(|| a.friend_b.cmp(&b.friend_b))
        });

        let mut shared_artists: Vec<(String, Vec<String>, i64)> = artist_agg
            .into_iter()
            .filter(|(_, (users, _))| users.len() >= 2)
            .map(|(artist, (users, total_plays))| {
                let mut names: Vec<String> = users
                    .into_iter()
                    .filter_map(|u| friend_names.get(&u).cloned())
                    .collect();
                names.sort();
                (artist, names, total_plays)
            })
            .collect();
        shared_artists.sort_by(|a, b| {
            b.1.len()
                .cmp(&a.1.len())
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut shared_tracks: Vec<TrendsetterTrack> = Vec::new();
        let mut trendsetter_counts: BTreeMap<String, i64> = BTreeMap::new();
        for (track_uri, agg) in track_agg {
            if agg.users.len() < 2 {
                continue;
            }
            let mut friends_who_played: Vec<String> = agg
                .users
                .into_iter()
                .filter_map(|u| friend_names.get(&u).cloned())
                .collect();
            friends_who_played.sort();

            let first_player_name = friend_names
                .get(&agg.first_user_uri)
                .cloned()
                .unwrap_or(agg.first_user_uri);
            *trendsetter_counts
                .entry(first_player_name.clone())
                .or_insert(0) += 1;

            shared_tracks.push(TrendsetterTrack {
                artist_name: agg.artist_name,
                track_name: agg.track_name,
                track_uri,
                friends_who_played,
                first_player_name,
                first_played_ms: agg.first_played_ms,
                total_plays: agg.total_plays,
            });
        }
        shared_tracks.sort_by(|a, b| {
            b.friends_who_played
                .len()
                .cmp(&a.friends_who_played.len())
                .then_with(|| b.total_plays.cmp(&a.total_plays))
                .then_with(|| a.first_played_ms.cmp(&b.first_played_ms))
        });

        let mut trendsetter_leaderboard: Vec<(String, i64)> =
            trendsetter_counts.into_iter().collect();
        trendsetter_leaderboard.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        Ok(OverlapReport {
            friend_pairs,
            shared_artists,
            shared_tracks,
            trendsetter_leaderboard,
        })
    }
}

fn migrate_listens_table(conn: &Connection) -> Result<()> {
    let mut existing_cols: HashSet<String> = HashSet::new();
    {
        let mut stmt = conn.prepare("PRAGMA table_info(listens)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for col in rows {
            existing_cols.insert(col?);
        }
    }

    for (col_name, col_def) in LISTENS_MIGRATIONS {
        if !existing_cols.contains(*col_name) {
            let sql = format!("ALTER TABLE listens ADD COLUMN {col_name} {col_def}");
            conn.execute(&sql, [])
                .with_context(|| format!("Failed to migrate listens column `{col_name}`"))?;
        }
    }

    Ok(())
}

fn compute_streaks(dates: &BTreeSet<NaiveDate>, today: NaiveDate) -> (i64, i64) {
    if dates.is_empty() {
        return (0, 0);
    }

    let mut longest = 0i64;
    let mut current_run = 0i64;
    let mut prev: Option<NaiveDate> = None;
    for &d in dates {
        if let Some(p) = prev
            && d == p + Duration::days(1)
        {
            current_run += 1;
        } else {
            current_run = 1;
        }
        if current_run > longest {
            longest = current_run;
        }
        prev = Some(d);
    }

    let start_day = if dates.contains(&today) {
        Some(today)
    } else {
        let yesterday = today - Duration::days(1);
        if dates.contains(&yesterday) {
            Some(yesterday)
        } else {
            None
        }
    };

    let mut current_streak = 0i64;
    if let Some(mut day) = start_day {
        while dates.contains(&day) {
            current_streak += 1;
            day -= Duration::days(1);
        }
    }

    (longest, current_streak)
}

pub fn spotify_uri_to_url(uri: &str) -> String {
    let parts: Vec<&str> = uri.split(':').collect();
    if parts.len() == 3 && parts[0] == "spotify" && !parts[2].is_empty() {
        match parts[1] {
            "track" | "episode" | "album" | "artist" | "playlist" | "show" => {
                return format!("https://open.spotify.com/{}/{}", parts[1], parts[2]);
            }
            _ => {}
        }
    }
    uri.to_string()
}

pub fn millis_to_local(ms: i64) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(ms).single()
}

pub fn format_timestamp_local(ms: i64) -> String {
    millis_to_local(ms)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ms.to_string())
}

pub fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::spotify::{FriendTrack, FriendUser, NamedUri, TrackContext};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path(label: &str) -> (PathBuf, PathBuf) {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "spotify_friend_logger_test_{}_{}_{}",
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
}
