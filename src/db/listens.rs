use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{Connection, Row, params, types::Value as SqlValue};
use serde::Serialize;

use crate::spotify::{
    FriendActivity, SPOTIFY_DJ_PLAYLIST_ID, canonical_spotify_dj_uri, is_spotify_dj_context,
};

use super::{Database, format_timestamp_local, spotify_uri_to_url};

#[derive(Debug, Clone)]
pub struct LatestUserListen {
    pub played_at_ms: i64,
    pub track_uri: String,
    pub duration_ms: Option<i64>,
    pub is_playing_seen: bool,
    pub ended_at_ms: Option<i64>,
    pub listened_ms: Option<i64>,
    #[allow(dead_code)]
    pub was_skipped: Option<bool>,
    #[allow(dead_code)]
    pub inferred_repeat: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RepairReport {
    pub merged_duplicates: usize,
    pub fixed_skips: usize,
    pub remaining_listens: usize,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct PaginatedHistory {
    pub items: Vec<ListenRecord>,
    pub total: i64,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FriendSummary {
    pub user_uri: String,
    pub user_name: String,
    pub image_url: Option<String>,
    pub total_plays: i64,
    pub last_active_ms: i64,
    pub last_active_local: String,
    pub latest_track_uri: String,
    pub latest_track_name: String,
    pub latest_artist_name: String,
    pub latest_cover_url: Option<String>,
    pub latest_spotify_url: String,
    pub is_live: bool,
}

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
    pub context_image_url: Option<String>,
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

        let raw_ctx_uri = activity.context_uri();
        let raw_ctx_type = activity.context_type();
        let raw_ctx_name = activity.context_name();
        let is_liked_songs = is_liked_songs_context(raw_ctx_uri, raw_ctx_type, raw_ctx_name);
        let is_dj = !is_liked_songs && is_spotify_dj_context(raw_ctx_uri, raw_ctx_type);
        let canonical_personal_uri = if is_liked_songs {
            let uid = activity
                .user
                .uri
                .strip_prefix("spotify:user:")
                .unwrap_or(&activity.user.uri)
                .trim();
            Some(format!("spotify:user:{uid}:collection"))
        } else if is_dj {
            Some(canonical_spotify_dj_uri(&activity.user.uri))
        } else {
            None
        };
        let context_name = if is_liked_songs {
            Some("Liked Songs")
        } else if is_dj {
            Some("DJ")
        } else {
            raw_ctx_name
        };
        let context_uri = if is_liked_songs || is_dj {
            canonical_personal_uri.as_deref()
        } else {
            raw_ctx_uri
        };
        let context_type = if is_liked_songs {
            Some("liked_songs")
        } else if is_dj {
            Some("dj")
        } else {
            raw_ctx_type
        };
        let context_owner = if is_liked_songs || is_dj {
            Some(activity.user.name.as_str())
        } else {
            activity.context_owner()
        };
        let context_is_private = if is_liked_songs || is_dj || activity.is_private_context() {
            1i64
        } else {
            0i64
        };
        let context_image_url = if is_liked_songs || is_dj {
            None
        } else {
            activity.context_image_url()
        };
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
                context_type, context_owner, context_is_private, context_image_url,
                is_playing_seen, inferred_repeat
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                ?25, ?26, ?27, ?28, ?29, ?30
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
                context_name,
                context_uri,
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
                context_type,
                context_owner,
                context_is_private,
                context_image_url,
                is_playing_seen,
                inferred_repeat_val,
            ],
        )?;

        if rows == 0 && is_playing {
            self.conn.execute(
                "
                UPDATE listens
                SET is_playing_seen = 1,
                    context_image_url = COALESCE(context_image_url, ?4)
                WHERE user_uri = ?1
                  AND track_uri = ?2
                  AND played_at_ms = ?3
                  AND (is_playing_seen = 0 OR (context_image_url IS NULL AND ?4 IS NOT NULL))
                ",
                params![
                    activity.user.uri,
                    activity.track.uri,
                    played_at_ms,
                    context_image_url
                ],
            )?;
        }

        Ok(rows > 0)
    }

    #[allow(dead_code)]
    pub fn mark_playing_seen(
        &self,
        user_uri: &str,
        track_uri: &str,
        played_at_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "
            UPDATE listens
            SET is_playing_seen = 1
            WHERE user_uri = ?1
              AND track_uri = ?2
              AND played_at_ms = ?3
              AND is_playing_seen = 0
            ",
            params![user_uri, track_uri, played_at_ms],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn has_recent_listen_near(
        &self,
        user_uri: &str,
        track_uri: &str,
        played_at_ms: i64,
        window_ms: i64,
    ) -> Result<bool> {
        let found: bool = self
            .conn
            .query_row(
                "
                SELECT 1
                FROM listens
                WHERE user_uri = ?1
                  AND track_uri = ?2
                  AND ABS(played_at_ms - ?3) <= ?4
                LIMIT 1
                ",
                params![user_uri, track_uri, played_at_ms, window_ms.abs()],
                |_| Ok(true),
            )
            .unwrap_or(false);
        Ok(found)
    }

    #[allow(dead_code)]
    pub fn get_latest_listen_for_user(&self, user_uri: &str) -> Result<Option<LatestUserListen>> {
        use rusqlite::OptionalExtension;
        let row = self
            .conn
            .query_row(
                "
                SELECT
                    played_at_ms, track_uri, duration_ms, is_playing_seen,
                    ended_at_ms, listened_ms, was_skipped, inferred_repeat
                FROM listens
                WHERE user_uri = ?1
                ORDER BY played_at_ms DESC, id DESC
                LIMIT 1
                ",
                params![user_uri],
                |row| {
                    let is_playing_seen: i64 = row.get(3)?;
                    let skipped_int: Option<i64> = row.get(6)?;
                    let inferred_int: i64 = row.get(7)?;
                    Ok(LatestUserListen {
                        played_at_ms: row.get(0)?,
                        track_uri: row.get(1)?,
                        duration_ms: row.get(2)?,
                        is_playing_seen: is_playing_seen != 0,
                        ended_at_ms: row.get(4)?,
                        listened_ms: row.get(5)?,
                        was_skipped: skipped_int.map(|v| v != 0),
                        inferred_repeat: inferred_int != 0,
                    })
                },
            )
            .optional()?;
        Ok(row)
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
        self.finalize_play_with_listened(
            user_uri,
            track_uri,
            played_at_ms,
            ended_at_ms,
            elapsed_ms,
            duration_ms,
            switched_to_next_track,
        )
    }

    #[allow(dead_code, clippy::too_many_arguments)]
    pub fn finalize_play_with_listened(
        &self,
        user_uri: &str,
        track_uri: &str,
        played_at_ms: i64,
        ended_at_ms: i64,
        active_listened_ms: i64,
        duration_ms: Option<i64>,
        switched_to_next_track: bool,
    ) -> Result<()> {
        let active_ms = active_listened_ms.max(0);
        let listened_ms = match duration_ms {
            Some(d) if d > 0 => Some(active_ms.min(d)),
            _ => Some(active_ms),
        };

        let was_skipped: Option<i64> = if switched_to_next_track {
            match duration_ms {
                Some(d) if d >= 30_000 => {
                    let threshold = (d / 2).min(d - 15_000);
                    if active_ms < threshold {
                        Some(1)
                    } else {
                        Some(0)
                    }
                }
                _ => None,
            }
        } else {
            match duration_ms {
                Some(d) if d > 0 && active_ms >= (d * 80 / 100) => Some(0),
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
        self.query_history_search(friend_filter, days, limit, None)
    }

    pub fn query_history_search(
        &self,
        friend_filter: Option<&str>,
        days: Option<i64>,
        limit: Option<usize>,
        search_query: Option<&str>,
    ) -> Result<Vec<ListenRecord>> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let filter_pattern = friend_filter
            .map(|f| format!("%{}%", f.trim().to_lowercase()))
            .unwrap_or_else(|| "%".to_string());
        let search_pattern = search_query
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| format!("%{}%", q.to_lowercase()))
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
                context_type, context_owner, context_is_private, context_image_url,
                ended_at_ms, listened_ms, was_skipped, inferred_repeat
            FROM listens
            WHERE played_at_ms >= ?1
              AND (LOWER(user_name) LIKE ?2 OR LOWER(user_uri) LIKE ?2)
              AND (
                  ?3 = '%'
                  OR LOWER(track_name) LIKE ?3
                  OR LOWER(artist_name) LIKE ?3
                  OR LOWER(COALESCE(all_artists, '')) LIKE ?3
                  OR LOWER(COALESCE(album_name, '')) LIKE ?3
                  OR LOWER(COALESCE(context_name, '')) LIKE ?3
                  OR LOWER(user_name) LIKE ?3
              )
            ORDER BY played_at_ms DESC
            LIMIT ?4
            ",
        )?;

        let rows = stmt.query_map(
            params![since_ms, filter_pattern, search_pattern, limit_val],
            row_to_listen_record,
        )?;

        let mut records = Vec::new();
        for r in rows {
            records.push(r?);
        }
        Ok(records)
    }

    #[allow(dead_code)]
    pub fn query_history_paginated(
        &self,
        users: &[String],
        days: Option<i64>,
        search: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedHistory> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let search_pattern = search
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| format!("%{}%", q.to_lowercase()))
            .unwrap_or_else(|| "%".to_string());

        let clean_users: Vec<String> = users
            .iter()
            .map(|u| u.trim().to_lowercase())
            .filter(|u| !u.is_empty())
            .collect();

        let mut where_clauses = vec![
            "played_at_ms >= ?".to_string(),
            "(? = '%' OR LOWER(track_name) LIKE ? OR LOWER(artist_name) LIKE ? OR LOWER(COALESCE(all_artists, '')) LIKE ? OR LOWER(COALESCE(album_name, '')) LIKE ? OR LOWER(COALESCE(context_name, '')) LIKE ? OR LOWER(user_name) LIKE ?)".to_string(),
        ];

        let mut bind_values: Vec<SqlValue> = vec![
            SqlValue::Integer(since_ms),
            SqlValue::Text(search_pattern.clone()),
            SqlValue::Text(search_pattern.clone()),
            SqlValue::Text(search_pattern.clone()),
            SqlValue::Text(search_pattern.clone()),
            SqlValue::Text(search_pattern.clone()),
            SqlValue::Text(search_pattern.clone()),
            SqlValue::Text(search_pattern),
        ];

        if !clean_users.is_empty() {
            let mut user_or_parts = Vec::with_capacity(clean_users.len());
            for u in &clean_users {
                user_or_parts.push(
                    "(LOWER(user_uri) = ? OR LOWER(user_name) LIKE ? OR LOWER(user_uri) LIKE ?)"
                        .to_string(),
                );
                let like = format!("%{u}%");
                bind_values.push(SqlValue::Text(u.clone()));
                bind_values.push(SqlValue::Text(like.clone()));
                bind_values.push(SqlValue::Text(like));
            }
            where_clauses.push(format!("({})", user_or_parts.join(" OR ")));
        }

        let where_sql = where_clauses.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM listens WHERE {where_sql}");
        let total: i64 = self.conn.query_row(
            &count_sql,
            rusqlite::params_from_iter(bind_values.iter()),
            |row| row.get(0),
        )?;

        let select_sql = format!(
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
                context_type, context_owner, context_is_private, context_image_url,
                ended_at_ms, listened_ms, was_skipped, inferred_repeat
            FROM listens
            WHERE {where_sql}
            ORDER BY played_at_ms DESC, id DESC
            LIMIT ? OFFSET ?
            "
        );

        let mut page_values = bind_values;
        page_values.push(SqlValue::Integer(limit as i64));
        page_values.push(SqlValue::Integer(offset as i64));

        let mut stmt = self.conn.prepare(&select_sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(page_values.iter()),
            row_to_listen_record,
        )?;

        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }

        let has_more = (offset as i64 + items.len() as i64) < total;
        Ok(PaginatedHistory {
            items,
            total,
            limit,
            offset,
            has_more,
        })
    }

    pub fn repair_duplicate_listens_and_skips(&self) -> Result<RepairReport> {
        let report = repair_duplicate_listens_and_skips_conn(&self.conn)?;
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('repair_v1_skip_dedupe', '1')",
            [],
        );
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('repair_v2_end_of_track_dedupe', '1')",
            [],
        );
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('repair_v5_stuck_and_pause_resume', '1')",
            [],
        );
        Ok(report)
    }

    pub fn query_friends_summary(&self) -> Result<Vec<FriendSummary>> {
        let now_ms = Utc::now().timestamp_millis();
        let mut stmt = self.conn.prepare(
            "
            WITH ranked AS (
                SELECT
                    l.user_uri,
                    l.user_name,
                    l.played_at_ms,
                    l.logged_at_ms,
                    l.track_uri,
                    l.track_name,
                    COALESCE(NULLIF(l.all_artists, ''), l.artist_name) AS artist_display,
                    l.cover_url,
                    l.is_playing_seen,
                    l.ended_at_ms,
                    COUNT(*) OVER (PARTITION BY l.user_uri) AS total_plays,
                    ROW_NUMBER() OVER (PARTITION BY l.user_uri ORDER BY l.played_at_ms DESC, l.id DESC) AS rn
                FROM listens l
            )
            SELECT
                r.user_uri,
                r.user_name,
                p.image_url,
                r.total_plays,
                r.played_at_ms,
                r.logged_at_ms,
                r.track_uri,
                r.track_name,
                r.artist_display,
                r.cover_url,
                r.is_playing_seen,
                r.ended_at_ms
            FROM ranked r
            LEFT JOIN friend_profiles p ON p.user_uri = r.user_uri
            WHERE r.rn = 1
            ORDER BY r.played_at_ms DESC, r.user_name COLLATE NOCASE ASC
            ",
        )?;

        let rows = stmt
            .query_map([], |row| {
                let played_at_ms: i64 = row.get(4)?;
                let logged_at_ms: i64 = row.get(5)?;
                let track_uri: String = row.get(6)?;
                let is_playing_seen: i64 = row.get(10)?;
                let ended_at_ms: Option<i64> = row.get(11)?;
                let recent_activity_ms = played_at_ms.max(logged_at_ms);
                let is_live = ended_at_ms.is_none()
                    && is_playing_seen != 0
                    && (now_ms - recent_activity_ms).abs() <= 10 * 60 * 1000;
                Ok(FriendSummary {
                    user_uri: row.get(0)?,
                    user_name: row.get(1)?,
                    image_url: row.get(2)?,
                    total_plays: row.get(3)?,
                    last_active_ms: played_at_ms,
                    last_active_local: format_timestamp_local(played_at_ms),
                    latest_spotify_url: spotify_uri_to_url(&track_uri),
                    latest_track_uri: track_uri,
                    latest_track_name: row.get(7)?,
                    latest_artist_name: row.get(8)?,
                    latest_cover_url: row.get(9)?,
                    is_live,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}

fn is_liked_songs_context(uri: Option<&str>, ctx_type: Option<&str>, name: Option<&str>) -> bool {
    if let Some(u) = uri {
        let u_lc = u.trim().to_ascii_lowercase();
        if u_lc == "spotify:collection"
            || u_lc.ends_with(":collection")
            || u_lc.contains(":collection:")
        {
            return true;
        }
    }
    if let Some(t) = ctx_type {
        let t_lc = t.trim().to_ascii_lowercase();
        if t_lc == "collection" || t_lc == "liked_songs" {
            return true;
        }
    }
    if let Some(n) = name
        && n.trim().eq_ignore_ascii_case("liked songs")
    {
        return true;
    }
    false
}

pub(super) fn repair_per_user_liked_songs_conn(conn: &Connection) -> Result<usize> {
    let updated = conn.execute(
        "
        UPDATE listens
        SET context_uri = 'spotify:user:' || REPLACE(user_uri, 'spotify:user:', '') || ':collection',
            context_type = 'liked_songs',
            context_name = 'Liked Songs',
            context_owner = user_name,
            context_is_private = 1
        WHERE LOWER(COALESCE(context_type, '')) IN ('collection', 'liked_songs')
           OR LOWER(COALESCE(context_uri, '')) = 'spotify:collection'
           OR LOWER(COALESCE(context_uri, '')) LIKE '%:collection'
           OR LOWER(COALESCE(context_uri, '')) LIKE '%:collection:%'
           OR LOWER(COALESCE(context_name, '')) = 'liked songs'
        ",
        [],
    )?;
    Ok(updated)
}

pub(super) fn repair_per_user_dj_conn(conn: &Connection) -> Result<usize> {
    let dj_id_lc = SPOTIFY_DJ_PLAYLIST_ID.to_ascii_lowercase();
    let exact_playlist_uri = format!("spotify:playlist:{dj_id_lc}");
    let suffixed_playlist_uri = format!("%:playlist:{dj_id_lc}");
    let updated = conn.execute(
        "
        UPDATE listens
        SET context_uri = 'spotify:user:' || REPLACE(user_uri, 'spotify:user:', '') || ':dj',
            context_type = 'dj',
            context_name = 'DJ',
            context_owner = user_name,
            context_is_private = 1,
            context_image_url = NULL
        WHERE LOWER(COALESCE(context_type, '')) = 'dj'
           OR LOWER(COALESCE(context_uri, '')) = 'spotify:dj'
           OR LOWER(COALESCE(context_uri, '')) LIKE '%:dj'
           OR LOWER(COALESCE(context_uri, '')) LIKE '%:dj:%'
           OR LOWER(COALESCE(context_uri, '')) = ?1
           OR LOWER(COALESCE(context_uri, '')) LIKE ?2
        ",
        params![exact_playlist_uri, suffixed_playlist_uri],
    )?;
    Ok(updated)
}

fn row_to_listen_record(row: &Row<'_>) -> rusqlite::Result<ListenRecord> {
    let played_at_ms: i64 = row.get(1)?;
    let explicit_int: Option<i64> = row.get(17)?;
    let ctx_priv_int: i64 = row.get(25)?;
    let skipped_int: Option<i64> = row.get(29)?;
    let inferred_repeat_int: i64 = row.get(30)?;
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
        context_image_url: row.get(26)?,
        ended_at_ms: row.get(27)?,
        listened_ms: row.get(28)?,
        was_skipped: skipped_int.map(|v| v != 0),
        inferred_repeat: inferred_repeat_int != 0,
    })
}

struct RawListenRow {
    id: i64,
    played_at_ms: i64,
    track_uri: String,
    duration_ms: Option<i64>,
    is_playing_seen: i64,
    ended_at_ms: Option<i64>,
    listened_ms: Option<i64>,
    was_skipped: Option<i64>,
    inferred_repeat: i64,
    context_image_url: Option<String>,
}

fn row_segment_listened_ms(row: &RawListenRow, after_row: Option<&RawListenRow>) -> i64 {
    let raw = if let Some(lst) = row.listened_ms {
        lst.max(0)
    } else if let Some(end) = row.ended_at_ms {
        end.saturating_sub(row.played_at_ms).max(0)
    } else if let Some(an) = after_row
        && an.track_uri != row.track_uri
    {
        let diff = an.played_at_ms.saturating_sub(row.played_at_ms);
        let cap = row
            .duration_ms
            .filter(|&d| d > 0)
            .map(|d| d + 30_000)
            .unwrap_or(300_000);
        if diff >= 0 && diff <= cap { diff } else { 0 }
    } else {
        0
    };
    match row.duration_ms {
        Some(d) if d > 0 => raw.min(d),
        _ => raw,
    }
}

fn is_phantom_inferred_repeat_row(
    row: &RawListenRow,
    run_dur: Option<i64>,
    after_row: Option<&RawListenRow>,
) -> bool {
    if row.inferred_repeat == 0 {
        return false;
    }
    let phantom_by_next = after_row.is_some_and(|an| {
        an.track_uri != row.track_uri
            && an.played_at_ms.saturating_sub(row.played_at_ms) <= 25_000
    });
    let raw_elapsed = row
        .ended_at_ms
        .map(|e| e.saturating_sub(row.played_at_ms).max(0));
    let incomplete_or_off_duration = match (run_dur.or(row.duration_ms.filter(|&d| d > 0)), raw_elapsed) {
        (Some(d), Some(el)) => el < (d * 85) / 100 || el > (d * 115) / 100,
        (_, None) => true,
        (None, Some(el)) => el <= 25_000,
    };
    phantom_by_next || row.was_skipped == Some(1) || incomplete_or_off_duration
}

pub(super) fn repair_duplicate_listens_and_skips_conn(conn: &Connection) -> Result<RepairReport> {
    let mut total_merged = 0usize;
    let mut total_fixed_skips = 0usize;

    // Run until fixed point (at most 3 passes) so deleting orphaned phantom rows immediately
    // reconciles adjacent rows in a single repair call.
    for _ in 0..3 {
        let (merged, fixed) = repair_duplicate_listens_and_skips_pass(conn)?;
        total_merged += merged;
        total_fixed_skips += fixed;
        if merged == 0 && fixed == 0 {
            break;
        }
    }

    let remaining_listens: i64 =
        conn.query_row("SELECT COUNT(*) FROM listens", [], |row| row.get(0))?;

    Ok(RepairReport {
        merged_duplicates: total_merged,
        fixed_skips: total_fixed_skips,
        remaining_listens: remaining_listens.max(0) as usize,
    })
}

fn repair_duplicate_listens_and_skips_pass(conn: &Connection) -> Result<(usize, usize)> {
    const PAUSE_RESUME_MAX_GAP_MS: i64 = 12 * 60 * 60 * 1000;

    let mut by_user: BTreeMap<String, Vec<RawListenRow>> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(
            "
            SELECT
                id, user_uri, played_at_ms, track_uri,
                duration_ms, is_playing_seen, ended_at_ms,
                listened_ms, was_skipped, inferred_repeat,
                context_image_url
            FROM listens
            ORDER BY user_uri ASC, played_at_ms ASC, id ASC
            ",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                RawListenRow {
                    id: row.get(0)?,
                    played_at_ms: row.get(2)?,
                    track_uri: row.get(3)?,
                    duration_ms: row.get(4)?,
                    is_playing_seen: row.get(5)?,
                    ended_at_ms: row.get(6)?,
                    listened_ms: row.get(7)?,
                    was_skipped: row.get(8)?,
                    inferred_repeat: row.get(9)?,
                    context_image_url: row.get(10)?,
                },
            ))
        })?;
        for item in rows {
            let (user_uri, raw) = item?;
            by_user.entry(user_uri).or_default().push(raw);
        }
    }

    let mut merged_duplicates = 0usize;
    let mut fixed_skips = 0usize;
    let mut ids_to_delete: Vec<i64> = Vec::new();
    type RepairUpdate = (
        i64,
        i64,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    );
    let mut updates: Vec<RepairUpdate> = Vec::new();

    for rows in by_user.values() {
        let mut i = 0;
        while i < rows.len() {
            let run_start = i;
            let mut run_end = i;
            let base_track = &rows[run_start].track_uri;
            let base_played = rows[run_start].played_at_ms;
            let base_dur = rows[run_start].duration_ms;
            let mut run_dur = base_dur.filter(|&d| d > 0);
            let mut run_accumulated_ms =
                row_segment_listened_ms(&rows[run_start], rows.get(run_start + 1));
            let mut merged_pause_gap = false;
            let mut inferred_in_run = usize::from(rows[run_start].inferred_repeat != 0);

            // An orphaned phantom `inferred_repeat = 1` row (e.g., separated from its parent
            // by an out-of-order track timestamp) should be deleted outright.
            if rows[run_start].inferred_repeat != 0
                && (is_phantom_inferred_repeat_row(
                    &rows[run_start],
                    run_dur,
                    rows.get(run_start + 1),
                ) || rows
                    .get(run_start + 1)
                    .is_some_and(|nxt| nxt.track_uri == *base_track && nxt.inferred_repeat != 0))
            {
                while run_end + 1 < rows.len()
                    && rows[run_end + 1].track_uri == *base_track
                    && rows[run_end + 1].inferred_repeat != 0
                {
                    run_end += 1;
                }
                for row in &rows[run_start..=run_end] {
                    ids_to_delete.push(row.id);
                    merged_duplicates += 1;
                }
                i = run_end + 1;
                continue;
            }

            while run_end + 1 < rows.len() {
                let next_idx = run_end + 1;
                let prev = &rows[run_end];
                let next = &rows[next_idx];
                if next.track_uri != *base_track {
                    break;
                }
                if run_dur.is_none() {
                    run_dur = next.duration_ms.filter(|&d| d > 0);
                }
                let after_next = rows.get(next_idx + 1);
                let delta_from_base = next.played_at_ms.saturating_sub(base_played);
                let delta_from_prev = next.played_at_ms.saturating_sub(prev.played_at_ms);

                if next.inferred_repeat == 0 {
                    let next_seg_ms = row_segment_listened_ms(next, after_next);
                    let (should_merge, is_pause_resume) = match run_dur {
                        Some(d) => {
                            let mid_track_cutoff = (d * 85) / 100;
                            let end_of_track_window = d + 25_000;
                            let short_stub_limit = (d / 2).min(30_000);
                            let tail_stub_limit = (d / 2).min(45_000);
                            let next_is_short_before_distinct = after_next.is_some_and(|an| {
                                an.track_uri != next.track_uri
                                    && an.played_at_ms.saturating_sub(next.played_at_ms)
                                        < short_stub_limit
                            });
                            let next_ended_short = next.ended_at_ms.is_some_and(|e| {
                                e.saturating_sub(next.played_at_ms) < short_stub_limit
                            }) && after_next
                                .is_none_or(|an| an.track_uri != next.track_uri);

                            if delta_from_base < mid_track_cutoff
                                || delta_from_prev < mid_track_cutoff
                            {
                                // Mid-track heartbeat, restart jitter, or [LAST] duplicate.
                                (true, false)
                            } else if delta_from_base <= end_of_track_window {
                                // End-of-track transition zone (85%..100%+25s):
                                // Merge if `next` is a [LAST] snapshot, a short stub right
                                // before switching/ending, or was falsely marked skipped.
                                // Preserve genuine back-to-back full repeats!
                                let m = next.is_playing_seen == 0
                                    || next_is_short_before_distinct
                                    || next_ended_short
                                    || next.was_skipped == Some(1);
                                (m, false)
                            } else if delta_from_prev <= PAUSE_RESUME_MAX_GAP_MS {
                                // Pause-and-resume split within 12 hours:
                                // 1) Incomplete play (<85% listened) resumed where combined
                                //    active playback fits in ~1 playthrough (<=125% dur).
                                // 2) Short tail stub (<min(50% dur, 45s)) finishing a track
                                //    that was paused near the end.
                                // 3) Unplayed [LAST] snapshot duplicate.
                                let combined_fits_one_play = run_accumulated_ms < mid_track_cutoff
                                    && run_accumulated_ms + next_seg_ms <= (d * 125) / 100;
                                let is_tail_stub = next_seg_ms > 0
                                    && next_seg_ms < tail_stub_limit
                                    && (next.was_skipped == Some(1)
                                        || next_is_short_before_distinct
                                        || next_ended_short);
                                let m = combined_fits_one_play
                                    || is_tail_stub
                                    || next.is_playing_seen == 0;
                                (m, m)
                            } else {
                                (false, false)
                            }
                        }
                        None => (delta_from_base < 180_000, false),
                    };

                    if should_merge {
                        if is_pause_resume {
                            merged_pause_gap = true;
                        }
                        if next.is_playing_seen != 0 {
                            run_accumulated_ms = match run_dur {
                                Some(d) => (run_accumulated_ms + next_seg_ms).min(d),
                                None => run_accumulated_ms + next_seg_ms,
                            };
                        }
                        run_end = next_idx;
                        continue;
                    }
                } else {
                    // `inferred_repeat == 1`: check for stuck-presence chains, overlapping
                    // post-restart repeats, or partial/phantom repeats caused by pauses/delays.
                    let chain_ahead = after_next.is_some_and(|an| {
                        an.track_uri == *base_track && an.inferred_repeat != 0
                    });
                    let part_of_inferred_chain = inferred_in_run >= 1 || chain_ahead;
                    let overlaps_prev = run_dur.is_some_and(|d| delta_from_prev < (d * 95) / 100);

                    if part_of_inferred_chain
                        || overlaps_prev
                        || is_phantom_inferred_repeat_row(next, run_dur, after_next)
                    {
                        inferred_in_run += 1;
                        run_end = next_idx;
                        continue;
                    }
                }
                break;
            }

            let run = &rows[run_start..=run_end];
            let keeper = &run[0];
            let merged_count = run.len() - 1;
            if merged_count > 0 {
                merged_duplicates += merged_count;
                for dup in &run[1..] {
                    ids_to_delete.push(dup.id);
                }
            }

            let merged_is_playing = run.iter().map(|r| r.is_playing_seen).max().unwrap_or(0);
            let merged_ctx_img = run
                .iter()
                .find_map(|r| r.context_image_url.clone().filter(|s| !s.is_empty()));
            let merged_dur = run
                .iter()
                .find_map(|r| r.duration_ms.filter(|&d| d > 0))
                .or(keeper.duration_ms);
            let last_non_inferred_ts = run
                .iter()
                .filter(|r| r.inferred_repeat == 0)
                .map(|r| r.ended_at_ms.unwrap_or(r.played_at_ms).max(r.played_at_ms))
                .max()
                .unwrap_or(base_played);
            let last_ts_in_run = if inferred_in_run > 0 {
                // Don't let deleted phantom inferred repeats stretch ended_at_ms by an hour.
                match merged_dur {
                    Some(d) if d > 0 => last_non_inferred_ts.min(base_played + d),
                    _ => last_non_inferred_ts,
                }
            } else {
                run.iter()
                    .map(|r| r.ended_at_ms.unwrap_or(r.played_at_ms).max(r.played_at_ms))
                    .max()
                    .unwrap_or(base_played)
            };

            let next_after_run = rows.get(run_end + 1);
            let session_window = merged_dur
                .filter(|&d| d > 0)
                .map(|d| ((d as f64) * 1.15) as i64)
                .unwrap_or(0)
                .max(300_000);

            let followed_by_switch = next_after_run.is_some_and(|nxt| {
                nxt.track_uri != *base_track
                    && nxt.played_at_ms >= last_ts_in_run
                    && nxt.played_at_ms.saturating_sub(last_ts_in_run) <= session_window
                    && (merged_pause_gap
                        || nxt.played_at_ms.saturating_sub(base_played)
                            <= session_window.saturating_mul(2))
            });

            let (new_ended_at, new_listened_ms, new_was_skipped) = if merged_pause_gap {
                // Merged pause-resume segments: use accumulated active playback time, not
                // wall-clock elapsed time across the pause gap.
                let end = if followed_by_switch {
                    next_after_run.unwrap().played_at_ms
                } else {
                    last_ts_in_run
                };
                let active_ms = match merged_dur {
                    Some(d) if d > 0 => run_accumulated_ms.min(d),
                    _ => run_accumulated_ms,
                };
                let skipped = if followed_by_switch && merged_is_playing != 0 {
                    match merged_dur {
                        Some(d) if d >= 30_000 => {
                            let threshold = (d / 2).min(d - 15_000);
                            Some(if active_ms < threshold { 1 } else { 0 })
                        }
                        _ => None,
                    }
                } else {
                    match merged_dur {
                        Some(d) if d > 0 && active_ms >= (d * 80 / 100) => Some(0),
                        _ => None,
                    }
                };
                (Some(end), Some(active_ms), skipped)
            } else if followed_by_switch && inferred_in_run == 0 {
                let nxt = next_after_run.unwrap();
                if merged_count > 0 || keeper.ended_at_ms.is_none() {
                    let end = nxt.played_at_ms;
                    let elapsed = (end - base_played).max(0);
                    let listened = match merged_dur {
                        Some(d) if d > 0 => Some(elapsed.min(d)),
                        _ => Some(elapsed),
                    };
                    let skipped = if merged_is_playing != 0 {
                        match merged_dur {
                            Some(d) if d >= 30_000 => {
                                let threshold = (d / 2).min(d - 15_000);
                                Some(if elapsed < threshold { 1 } else { 0 })
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    (Some(end), listened, skipped)
                } else {
                    (keeper.ended_at_ms, keeper.listened_ms, keeper.was_skipped)
                }
            } else if merged_count > 0 {
                let end = last_ts_in_run;
                let elapsed = (end - base_played).max(0);
                let listened = match merged_dur {
                    Some(d) if d > 0 => Some(elapsed.min(d)),
                    _ => Some(elapsed),
                };
                let skipped = match merged_dur {
                    Some(d) if d > 0 && elapsed >= (d * 80 / 100) => Some(0),
                    _ => None,
                };
                (Some(end), listened, skipped)
            } else {
                (keeper.ended_at_ms, keeper.listened_ms, keeper.was_skipped)
            };

            if new_was_skipped != keeper.was_skipped {
                fixed_skips += 1;
            }

            if merged_is_playing != keeper.is_playing_seen
                || merged_ctx_img != keeper.context_image_url
                || new_ended_at != keeper.ended_at_ms
                || new_listened_ms != keeper.listened_ms
                || new_was_skipped != keeper.was_skipped
            {
                updates.push((
                    keeper.id,
                    merged_is_playing,
                    merged_ctx_img,
                    new_ended_at,
                    new_listened_ms,
                    new_was_skipped,
                ));
            }

            i = run_end + 1;
        }
    }

    if !ids_to_delete.is_empty() || !updates.is_empty() {
        let tx = conn.unchecked_transaction()?;
        {
            let mut del_stmt = tx.prepare("DELETE FROM listens WHERE id = ?1")?;
            for id in &ids_to_delete {
                del_stmt.execute([id])?;
            }

            let mut upd_stmt = tx.prepare(
                "
                UPDATE listens
                SET is_playing_seen = ?1,
                    context_image_url = ?2,
                    ended_at_ms = ?3,
                    listened_ms = ?4,
                    was_skipped = ?5
                WHERE id = ?6
                ",
            )?;
            for (id, is_playing, ctx_img, ended_at, listened_ms, was_skipped) in &updates {
                upd_stmt.execute(params![
                    is_playing,
                    ctx_img.as_deref(),
                    ended_at,
                    listened_ms,
                    was_skipped,
                    id,
                ])?;
            }
        }
        tx.commit()?;
    }

    Ok((merged_duplicates, fixed_skips))
}

#[cfg(test)]
mod repair_tests {
    use std::path::PathBuf;

    use crate::db::Database;
    use crate::spotify::{FriendActivity, FriendTrack, FriendUser, NamedUri, TrackContext};

    fn temp_db(tag: &str) -> (PathBuf, Database) {
        let dir = std::env::temp_dir().join(format!(
            "bside_repair_test_{tag}_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("test.db")).unwrap();
        (dir, db)
    }

    fn make_activity(
        user_uri: &str,
        user_name: &str,
        track_uri: &str,
        track_name: &str,
        ts_ms: i64,
        duration_ms: Option<i64>,
        playing: bool,
    ) -> FriendActivity {
        FriendActivity {
            timestamp: ts_ms,
            playing,
            user: FriendUser {
                uri: user_uri.to_string(),
                name: user_name.to_string(),
                image_url: None,
            },
            track: FriendTrack {
                uri: track_uri.to_string(),
                name: track_name.to_string(),
                artist: Some(NamedUri {
                    uri: "spotify:artist:1".to_string(),
                    name: "Don Toliver".to_string(),
                }),
                artists: vec![NamedUri {
                    uri: "spotify:artist:1".to_string(),
                    name: "Don Toliver".to_string(),
                }],
                duration_ms,
                context: Some(TrackContext {
                    uri: "spotify:playlist:pl1".to_string(),
                    name: "Gym".to_string(),
                    context_type: "playlist".to_string(),
                    image_url: Some("https://i.scdn.co/image/pl1".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        }
    }

    #[test]
    fn bandit_multi_heartbeat_collapses_into_one_completed_play() {
        let (dir, db) = temp_db("bandit");
        let t0 = 1_700_000_000_000i64;
        let dur = 147_000i64;

        // 6s + 70s + 15s + 46s + 9s = 146s total across 5 segments, followed by next track
        let offsets = [0, 6_000, 76_000, 91_000, 137_000];
        let ends = [6_000, 76_000, 91_000, 137_000, 146_000];
        for (start, end) in offsets.into_iter().zip(ends) {
            let act = make_activity(
                "spotify:user:u1",
                "Alice",
                "spotify:track:bandit",
                "BANDIT",
                t0 + start,
                Some(dur),
                true,
            );
            assert!(db.insert_listen(&act).unwrap());
            db.finalize_play(
                "spotify:user:u1",
                "spotify:track:bandit",
                t0 + start,
                t0 + end,
                Some(dur),
                true,
            )
            .unwrap();
        }

        // Next distinct track at t0 + 146_000
        let next = make_activity(
            "spotify:user:u1",
            "Alice",
            "spotify:track:next",
            "NEXT",
            t0 + 146_000,
            Some(180_000),
            true,
        );
        assert!(db.insert_listen(&next).unwrap());

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 4);
        assert_eq!(report.fixed_skips, 1);
        assert_eq!(report.remaining_listens, 2);

        let history = db.query_history(None, None, None).unwrap();
        assert_eq!(history.len(), 2);
        let bandit = history.iter().find(|r| r.track_uri == "spotify:track:bandit").unwrap();
        assert_eq!(bandit.played_at_ms, t0);
        assert_eq!(bandit.ended_at_ms, Some(t0 + 146_000));
        assert_eq!(bandit.listened_ms, Some(146_000));
        assert_eq!(bandit.was_skipped, Some(false));
        assert_eq!(
            bandit.context_image_url.as_deref(),
            Some("https://i.scdn.co/image/pl1")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pause_live_to_last_duplicate_collapses_without_skip() {
        let (dir, db) = temp_db("pause_collapse");
        let t0 = 1_700_000_000_000i64;
        let dur = 200_000i64;

        let live = make_activity(
            "spotify:user:u1",
            "Alice",
            "spotify:track:t1",
            "Song A",
            t0,
            Some(dur),
            true,
        );
        db.insert_listen(&live).unwrap();
        db.finalize_play("spotify:user:u1", "spotify:track:t1", t0, t0 + 45_000, Some(dur), true)
            .unwrap();

        let paused = make_activity(
            "spotify:user:u1",
            "Alice",
            "spotify:track:t1",
            "Song A",
            t0 + 45_000,
            Some(dur),
            false,
        );
        db.insert_listen(&paused).unwrap();

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 1);
        assert_eq!(report.fixed_skips, 1);
        assert_eq!(report.remaining_listens, 1);

        let history = db.query_history(None, None, None).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].played_at_ms, t0);
        assert_eq!(history[0].ended_at_ms, Some(t0 + 45_000));
        assert_eq!(history[0].listened_ms, Some(45_000));
        assert_eq!(history[0].was_skipped, None);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn genuine_short_skip_is_preserved() {
        let (dir, db) = temp_db("genuine_skip");
        let t0 = 1_700_000_000_000i64;
        let dur = 200_000i64;

        let a = make_activity(
            "spotify:user:u1",
            "Alice",
            "spotify:track:short",
            "Skipped Track",
            t0,
            Some(dur),
            true,
        );
        db.insert_listen(&a).unwrap();
        db.finalize_play("spotify:user:u1", "spotify:track:short", t0, t0 + 12_000, Some(dur), true)
            .unwrap();

        let b = make_activity(
            "spotify:user:u1",
            "Alice",
            "spotify:track:other",
            "Next Track",
            t0 + 12_000,
            Some(210_000),
            true,
        );
        db.insert_listen(&b).unwrap();

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 0);
        assert_eq!(report.fixed_skips, 0);
        assert_eq!(report.remaining_listens, 2);

        let history = db.query_history(None, None, None).unwrap();
        let skipped = history.iter().find(|r| r.track_uri == "spotify:track:short").unwrap();
        assert_eq!(skipped.was_skipped, Some(true));
        assert_eq!(skipped.listened_ms, Some(12_000));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn repair_is_idempotent() {
        let (dir, db) = temp_db("idempotent");
        let t0 = 1_700_000_000_000i64;

        let a1 = make_activity(
            "spotify:user:u1",
            "Alice",
            "spotify:track:t1",
            "Track 1",
            t0,
            Some(180_000),
            true,
        );
        let a2 = make_activity(
            "spotify:user:u1",
            "Alice",
            "spotify:track:t1",
            "Track 1",
            t0 + 30_000,
            Some(180_000),
            true,
        );
        db.insert_listen(&a1).unwrap();
        db.finalize_play("spotify:user:u1", "spotify:track:t1", t0, t0 + 30_000, Some(180_000), true)
            .unwrap();
        db.insert_listen(&a2).unwrap();

        let first = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(first.merged_duplicates, 1);
        assert_eq!(first.fixed_skips, 1);

        let second = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(second.merged_duplicates, 0);
        assert_eq!(second.fixed_skips, 0);
        assert_eq!(second.remaining_listens, 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn paginated_history_supports_multi_user_and_offsets() {
        let (dir, db) = temp_db("paginated");
        let now = chrono::Utc::now().timestamp_millis();

        for (i, (uri, name, track)) in [
            ("spotify:user:alice", "Alice", "Track 1"),
            ("spotify:user:bob", "Bob", "Track 2"),
            ("spotify:user:carol", "Carol", "Track 3"),
            ("spotify:user:alice", "Alice", "Track 4"),
        ]
        .into_iter()
        .enumerate()
        {
            let act = make_activity(
                uri,
                name,
                &format!("spotify:track:t{i}"),
                track,
                now - ((4 - i as i64) * 60_000),
                Some(180_000),
                true,
            );
            db.insert_listen(&act).unwrap();
        }

        let page1 = db
            .query_history_paginated(
                &["spotify:user:alice".to_string(), "bob".to_string()],
                None,
                None,
                2,
                0,
            )
            .unwrap();
        assert_eq!(page1.total, 3);
        assert_eq!(page1.items.len(), 2);
        assert!(page1.has_more);

        let page2 = db
            .query_history_paginated(
                &["spotify:user:alice".to_string(), "bob".to_string()],
                None,
                None,
                2,
                2,
            )
            .unwrap();
        assert_eq!(page2.total, 3);
        assert_eq!(page2.items.len(), 1);
        assert!(!page2.has_more);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn end_of_track_heartbeat_collapses_into_one_completed_play() {
        let (dir, db) = temp_db("brain_power_eot");
        let t0 = 1_795_619_146_000i64; // 16:05:46
        let dur = 124_000i64; // 02:04

        // Play #1: 16:05:46 -> ended by heartbeat at +121_204 ms
        let p1 = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:brain_power",
            "Brain Power",
            t0,
            Some(dur),
            true,
        );
        db.insert_listen(&p1).unwrap();
        db.finalize_play(
            "spotify:user:wrobelek",
            "spotify:track:brain_power",
            t0,
            t0 + 121_204,
            Some(dur),
            false,
        )
        .unwrap();

        // Phantom end-of-track heartbeat at +121_204 ms -> falsely skipped 3.436s later
        let p2 = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:brain_power",
            "Brain Power",
            t0 + 121_204,
            Some(dur),
            true,
        );
        db.insert_listen(&p2).unwrap();
        db.finalize_play(
            "spotify:user:wrobelek",
            "spotify:track:brain_power",
            t0 + 121_204,
            t0 + 124_640,
            Some(dur),
            true,
        )
        .unwrap();

        // Next track: Recharge at +124_640 ms (16:07:50)
        let recharge = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:recharge",
            "Recharge",
            t0 + 124_640,
            Some(128_000),
            true,
        );
        db.insert_listen(&recharge).unwrap();

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 1);
        assert_eq!(report.remaining_listens, 2);

        let history = db.query_history(None, None, None).unwrap();
        assert_eq!(history.len(), 2);
        let bp = history
            .iter()
            .find(|r| r.track_uri == "spotify:track:brain_power")
            .unwrap();
        assert_eq!(bp.played_at_ms, t0);
        assert_eq!(bp.ended_at_ms, Some(t0 + 124_640));
        assert_eq!(bp.listened_ms, Some(124_000));
        assert_eq!(bp.was_skipped, Some(false));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn genuine_back_to_back_full_repeat_is_preserved() {
        let (dir, db) = temp_db("genuine_repeat");
        let t0 = 1_795_619_146_000i64;
        let dur = 124_000i64;

        // Play #1: full 124s
        let p1 = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:brain_power",
            "Brain Power",
            t0,
            Some(dur),
            true,
        );
        db.insert_listen(&p1).unwrap();
        db.finalize_play(
            "spotify:user:wrobelek",
            "spotify:track:brain_power",
            t0,
            t0 + dur,
            Some(dur),
            false,
        )
        .unwrap();

        // Play #2: full 124s repeat
        let p2 = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:brain_power",
            "Brain Power",
            t0 + dur,
            Some(dur),
            true,
        );
        db.insert_listen(&p2).unwrap();
        db.finalize_play(
            "spotify:user:wrobelek",
            "spotify:track:brain_power",
            t0 + dur,
            t0 + dur * 2,
            Some(dur),
            true,
        )
        .unwrap();

        // Next track: Recharge at +248s
        let recharge = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:recharge",
            "Recharge",
            t0 + dur * 2,
            Some(128_000),
            true,
        );
        db.insert_listen(&recharge).unwrap();

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 0);
        assert_eq!(report.fixed_skips, 0);
        assert_eq!(report.remaining_listens, 3);

        let history = db.query_history(None, None, None).unwrap();
        let bp_plays: Vec<_> = history
            .iter()
            .filter(|r| r.track_uri == "spotify:track:brain_power")
            .collect();
        assert_eq!(bp_plays.len(), 2);
        for play in bp_plays {
            assert_eq!(play.was_skipped, Some(false));
            assert_eq!(play.listened_ms, Some(dur));
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stuck_inferred_repeat_chains_and_interleaved_restart_collapse_to_one() {
        let (dir, db) = temp_db("stuck_inferred_chain");
        let t0 = 1_795_607_888_000i64;
        let dur = 157_720i64;

        // Reproduces wrobelek's Tak to lecialo: 1 real play + 10 inferred repeats +
        // 1 restart duplicate (+129s) + 10 MORE interleaved inferred repeats = 22 rows!
        let p1 = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:tak",
            "Tak to lecialo",
            t0,
            Some(dur),
            true,
        );
        db.insert_listen(&p1).unwrap();
        db.finalize_play("spotify:user:wrobelek", "spotify:track:tak", t0, t0 + dur, Some(dur), false)
            .unwrap();
        for k in 1..=10i64 {
            let cyc = t0 + dur * k;
            db.insert_inferred_repeat(&p1, cyc).unwrap();
            db.finalize_play("spotify:user:wrobelek", "spotify:track:tak", cyc, cyc + dur, Some(dur), false)
                .unwrap();
        }

        // Restart duplicate 129.2s after t0 + 10 more interleaved inferred repeats
        let t_restart = t0 + 129_152;
        let p_restart = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:tak",
            "Tak to lecialo",
            t_restart,
            Some(dur),
            true,
        );
        db.insert_listen(&p_restart).unwrap();
        db.finalize_play(
            "spotify:user:wrobelek",
            "spotify:track:tak",
            t_restart,
            t_restart + dur,
            Some(dur),
            false,
        )
        .unwrap();
        for k in 1..=10i64 {
            let cyc = t_restart + dur * k;
            db.insert_inferred_repeat(&p_restart, cyc).unwrap();
            db.finalize_play(
                "spotify:user:wrobelek",
                "spotify:track:tak",
                cyc,
                cyc + dur,
                Some(dur),
                false,
            )
            .unwrap();
        }

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 21);
        assert_eq!(report.remaining_listens, 1);

        let history = db.query_history(None, None, None).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].played_at_ms, t0);
        assert_eq!(history[0].listened_ms, Some(dur));
        assert_eq!(history[0].was_skipped, Some(false));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pause_resume_splits_collapse_preserving_accumulated_listened_ms() {
        let (dir, db) = temp_db("pause_resume_repair");
        let t0 = 1_795_600_000_000i64;
        let dur = 220_000i64;

        // 3-part pause-resume split across 20 minutes: 34s + 46s + 100s = 180s
        let segs = [(0i64, 34_000i64, false), (713_000, 46_000, false), (1_178_000, 100_000, true)];
        for (offset, len, switched) in segs {
            let act = make_activity(
                "spotify:user:ksiksen",
                "Ksiksen",
                "spotify:track:lvl",
                "LVL",
                t0 + offset,
                Some(dur),
                true,
            );
            db.insert_listen(&act).unwrap();
            db.finalize_play(
                "spotify:user:ksiksen",
                "spotify:track:lvl",
                t0 + offset,
                t0 + offset + len,
                Some(dur),
                switched,
            )
            .unwrap();
        }
        let next = make_activity(
            "spotify:user:ksiksen",
            "Ksiksen",
            "spotify:track:landline",
            "Landline",
            t0 + 1_278_000,
            Some(160_000),
            true,
        );
        db.insert_listen(&next).unwrap();

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 2);
        assert_eq!(report.remaining_listens, 2);

        let history = db.query_history(None, None, None).unwrap();
        let lvl = history.iter().find(|r| r.track_uri == "spotify:track:lvl").unwrap();
        assert_eq!(lvl.played_at_ms, t0);
        assert_eq!(lvl.listened_ms, Some(180_000));
        assert_eq!(lvl.was_skipped, Some(false));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn near_end_pause_and_short_tail_resume_collapses_into_one() {
        let (dir, db) = temp_db("tail_stub_repair");
        let t0 = 1_795_600_000_000i64;
        let dur = 247_000i64;

        // Play 1: 236s (95.5%), paused. Play 2: 5 min later, 12s tail marked skipped.
        let p1 = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:patrz",
            "patrz mi w oczy",
            t0,
            Some(dur),
            true,
        );
        db.insert_listen(&p1).unwrap();
        db.finalize_play("spotify:user:wrobelek", "spotify:track:patrz", t0, t0 + 236_000, Some(dur), false)
            .unwrap();

        let p2 = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:patrz",
            "patrz mi w oczy",
            t0 + 316_000,
            Some(dur),
            true,
        );
        db.insert_listen(&p2).unwrap();
        db.finalize_play(
            "spotify:user:wrobelek",
            "spotify:track:patrz",
            t0 + 316_000,
            t0 + 328_000,
            Some(dur),
            true,
        )
        .unwrap();

        let next = make_activity(
            "spotify:user:wrobelek",
            "wrobelek",
            "spotify:track:deli",
            "DELI SUPREME",
            t0 + 328_000,
            Some(151_000),
            true,
        );
        db.insert_listen(&next).unwrap();

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 1);
        assert_eq!(report.remaining_listens, 2);

        let history = db.query_history(None, None, None).unwrap();
        let patrz = history.iter().find(|r| r.track_uri == "spotify:track:patrz").unwrap();
        assert_eq!(patrz.played_at_ms, t0);
        assert_eq!(patrz.listened_ms, Some(dur));
        assert_eq!(patrz.was_skipped, Some(false));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn orphaned_phantom_inferred_repeat_separated_by_out_of_order_track_is_deleted() {
        let (dir, db) = temp_db("orphaned_inferred_repeat");
        let t0 = 1_795_600_000_000i64;
        let dur = 232_000i64;

        let p1 = make_activity(
            "spotify:user:maks1m",
            "maks1m-c",
            "spotify:track:stars",
            "All The Stars",
            t0,
            Some(dur),
            true,
        );
        db.insert_listen(&p1).unwrap();
        db.finalize_play("spotify:user:maks1m", "spotify:track:stars", t0, t0 + dur, Some(dur), false)
            .unwrap();

        // Out-of-order next track timestamp lands BEFORE the phantom inferred repeat.
        let mid = make_activity(
            "spotify:user:maks1m",
            "maks1m-c",
            "spotify:track:sigala",
            "Melody",
            t0 + 204_000,
            Some(167_000),
            true,
        );
        db.insert_listen(&mid).unwrap();

        // Orphaned phantom inferred repeat at t0 + 232s with 0s duration before skip.
        db.insert_inferred_repeat(&p1, t0 + dur).unwrap();
        db.finalize_play(
            "spotify:user:maks1m",
            "spotify:track:stars",
            t0 + dur,
            t0 + dur,
            Some(dur),
            true,
        )
        .unwrap();

        let report = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(report.merged_duplicates, 1);
        assert_eq!(report.remaining_listens, 2);

        // Second repair pass must be strictly idempotent.
        let second = db.repair_duplicate_listens_and_skips().unwrap();
        assert_eq!(second.merged_duplicates, 0);
        assert_eq!(second.fixed_skips, 0);
        assert_eq!(second.remaining_listens, 2);

        let _ = std::fs::remove_dir_all(dir);
    }
}
