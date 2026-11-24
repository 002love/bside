use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::params;
use serde::Serialize;

use crate::spotify::FriendActivity;

use super::{Database, format_timestamp_local};

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

impl Database {
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
}
