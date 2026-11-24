use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::spotify::FriendActivity;

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
}

#[derive(Debug, Clone)]
pub struct StatsSummary {
    pub total_plays: i64,
    pub unique_tracks: i64,
    pub unique_artists: i64,
    pub first_played_ms: Option<i64>,
    pub last_played_ms: Option<i64>,
    pub top_tracks: Vec<(String, String, i64)>,
    pub top_artists: Vec<(String, i64)>,
    pub hourly: [i64; 24],
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
                UNIQUE(user_uri, track_uri, played_at_ms)
            );

            CREATE INDEX IF NOT EXISTS idx_listens_user_time
                ON listens(user_uri, played_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_listens_time
                ON listens(played_at_ms DESC);
            ",
        )?;

        Ok(Self { conn })
    }

    /// Inserts a listen event if it isn't already recorded.
    /// Returns `Ok(true)` if a new row was inserted, `Ok(false)` if it was a duplicate.
    pub fn insert_listen(&self, activity: &FriendActivity) -> Result<bool> {
        let now_ms = Utc::now().timestamp_millis();
        let rows = self.conn.execute(
            "
            INSERT OR IGNORE INTO listens (
                played_at_ms, logged_at_ms,
                user_uri, user_name,
                track_uri, track_name,
                artist_name, artist_uri,
                album_name, album_uri,
                context_name, context_uri
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ",
            params![
                activity.timestamp,
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
            ],
        )?;
        Ok(rows > 0)
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
                context_name, context_uri
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            ORDER BY played_at_ms DESC
            LIMIT ?3
            ",
        )?;

        let rows = stmt.query_map(params![since_ms, filter_pattern, limit_val], |row| {
            let played_at_ms: i64 = row.get(1)?;
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

        let (total_plays, unique_tracks, unique_artists, first_played_ms, last_played_ms): (
            i64,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
        ) = self.conn.query_row(
            "
            SELECT
                COUNT(*),
                COUNT(DISTINCT track_uri),
                COUNT(DISTINCT artist_name),
                MIN(played_at_ms),
                MAX(played_at_ms)
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            ",
            params![since_ms, filter_pattern],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
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

        let mut hourly = [0i64; 24];
        let mut ts_stmt = self.conn.prepare(
            "
            SELECT played_at_ms
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
            ",
        )?;
        let ts_rows = ts_stmt.query_map(params![since_ms, filter_pattern], |row| row.get::<_, i64>(0))?;
        for ts in ts_rows {
            let ms = ts?;
            if let Some(dt) = millis_to_local(ms) {
                let hour = dt.format("%H").to_string().parse::<usize>().unwrap_or(0);
                if hour < 24 {
                    hourly[hour] += 1;
                }
            }
        }

        Ok(StatsSummary {
            total_plays,
            unique_tracks,
            unique_artists,
            first_played_ms,
            last_played_ms,
            top_tracks,
            top_artists,
            hourly,
        })
    }
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
    use super::*;
    use crate::spotify::{FriendTrack, FriendUser, NamedUri, TrackContext};

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
                }),
            },
        }
    }

    #[test]
    fn deduplicates_and_queries_stats() {
        let dir = std::env::temp_dir().join(format!(
            "spotify_friend_logger_test_{}",
            std::process::id()
        ));
        let db_path = dir.join("test.db");
        let _ = std::fs::remove_file(&db_path);

        let db = Database::open(&db_path).unwrap();
        let now_ms = Utc::now().timestamp_millis();

        let a1 = sample_activity(now_ms - 60_000, "spotify:track:1", "Song One");
        let a2 = sample_activity(now_ms - 30_000, "spotify:track:2", "Song Two");

        assert!(db.insert_listen(&a1).unwrap());
        assert!(!db.insert_listen(&a1).unwrap()); // duplicate ignored
        assert!(db.insert_listen(&a2).unwrap());

        let hist = db.query_history(Some("alice"), Some(1), Some(10)).unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].track_name, "Song Two");
        assert_eq!(hist[1].track_name, "Song One");

        let stats = db.query_stats(Some("alice"), Some(1)).unwrap();
        assert_eq!(stats.total_plays, 2);
        assert_eq!(stats.unique_tracks, 2);
        assert_eq!(stats.unique_artists, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
