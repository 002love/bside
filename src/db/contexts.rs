use std::collections::HashMap;

use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde::Serialize;

use super::{Database, build_users_filter_sql, single_user_vec, spotify_uri_to_url};

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct ContextListener {
    pub user_uri: String,
    pub user_name: String,
    pub image_url: Option<String>,
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
    pub image_url: Option<String>,
    pub listeners: Vec<ContextListener>,
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
    pub cover_url: Option<String>,
    pub duration_ms: Option<i64>,
    pub release_date: Option<String>,
    pub popularity: Option<i32>,
    pub play_count: i64,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub spotify_url: String,
}

impl Database {
    #[allow(dead_code)]
    pub fn query_contexts(
        &self,
        friend_filter: Option<&str>,
        private_only: bool,
    ) -> Result<Vec<ContextSummary>> {
        let users = single_user_vec(friend_filter);
        self.query_contexts_for_users(&users, None, private_only)
    }

    #[allow(dead_code)]
    pub fn query_contexts_for_users(
        &self,
        users: &[String],
        days: Option<i64>,
        private_only: bool,
    ) -> Result<Vec<ContextSummary>> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let private_flag = if private_only { 1i64 } else { 0i64 };

        let mut bind_values: Vec<SqlValue> = vec![SqlValue::Integer(since_ms)];
        let user_clause =
            build_users_filter_sql(users, "l.user_uri", "l.user_name", &mut bind_values);
        bind_values.push(SqlValue::Integer(private_flag));

        let listeners_sql = format!(
            "
            SELECT
                COALESCE(NULLIF(l.context_uri, ''), l.context_name) AS ctx_key,
                l.user_uri,
                MAX(l.user_name) AS user_name,
                MAX(NULLIF(p.image_url, '')) AS image_url
            FROM listens l
            LEFT JOIN friend_profiles p ON p.user_uri = l.user_uri
            WHERE l.played_at_ms >= ?
              AND {user_clause}
              AND (
                  (l.context_uri IS NOT NULL AND l.context_uri != '')
                  OR (l.context_name IS NOT NULL AND l.context_name != '')
              )
              AND (
                  ? = 0
                  OR l.context_is_private = 1
                  OR LOWER(COALESCE(l.context_type, '')) IN ('liked_songs', 'collection', 'dj')
              )
            GROUP BY ctx_key, l.user_uri
            ORDER BY MAX(l.played_at_ms) DESC, user_name COLLATE NOCASE ASC
            "
        );
        let mut listeners_by_ctx: HashMap<String, Vec<ContextListener>> = HashMap::new();
        {
            let mut l_stmt = self.conn.prepare(&listeners_sql)?;
            let l_rows = l_stmt.query_map(params_from_iter(bind_values.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ContextListener {
                        user_uri: row.get(1)?,
                        user_name: row.get(2)?,
                        image_url: row.get(3)?,
                    },
                ))
            })?;
            for r in l_rows {
                let (ctx_key, listener) = r?;
                listeners_by_ctx.entry(ctx_key).or_default().push(listener);
            }
        }

        let sql = format!(
            "
            SELECT
                l.user_uri,
                MAX(l.user_name) AS user_name,
                COALESCE(l.context_uri, '') AS ctx_uri,
                CASE
                    WHEN LOWER(COALESCE(MAX(NULLIF(l.context_type, '')), '')) IN ('liked_songs', 'collection')
                    THEN 'Liked Songs'
                    WHEN LOWER(COALESCE(MAX(NULLIF(l.context_type, '')), '')) = 'dj'
                    THEN 'DJ'
                    ELSE COALESCE(MAX(NULLIF(l.context_name, '')), '')
                END AS ctx_name,
                COALESCE(MAX(NULLIF(l.context_type, '')), 'other') AS ctx_type,
                CASE
                    WHEN LOWER(COALESCE(MAX(NULLIF(l.context_type, '')), '')) IN ('liked_songs', 'collection', 'dj')
                    THEN MAX(l.user_name)
                    ELSE MAX(NULLIF(l.context_owner, ''))
                END AS ctx_owner,
                MAX(
                    CASE
                        WHEN LOWER(COALESCE(l.context_type, '')) IN ('liked_songs', 'collection', 'dj') THEN 1
                        ELSE l.context_is_private
                    END
                ) AS is_priv,
                COALESCE(
                    NULLIF(MAX(NULLIF(l.context_image_url, '')), ''),
                    MAX(fpp.image_url),
                    MAX(NULLIF(l.cover_url, ''))
                ) AS image_url,
                COUNT(DISTINCT l.track_uri) AS unique_tracks,
                COUNT(*) AS total_plays,
                MIN(l.played_at_ms) AS first_seen_ms,
                MAX(l.played_at_ms) AS last_seen_ms,
                COALESCE(NULLIF(l.context_uri, ''), l.context_name) AS ctx_key
            FROM listens l
            LEFT JOIN (
                SELECT playlist_uri, MAX(NULLIF(image_url, '')) AS image_url
                FROM friend_public_playlists
                GROUP BY playlist_uri
            ) fpp ON fpp.playlist_uri = l.context_uri
            WHERE l.played_at_ms >= ?
              AND {user_clause}
              AND (
                  (l.context_uri IS NOT NULL AND l.context_uri != '')
                  OR (l.context_name IS NOT NULL AND l.context_name != '')
              )
              AND (
                  ? = 0
                  OR l.context_is_private = 1
                  OR LOWER(COALESCE(l.context_type, '')) IN ('liked_songs', 'collection', 'dj')
              )
            GROUP BY l.user_uri, COALESCE(NULLIF(l.context_uri, ''), l.context_name)
            ORDER BY total_plays DESC, last_seen_ms DESC
            "
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(bind_values.iter()), |row| {
                let is_priv: i64 = row.get(6)?;
                let ctx_key: String = row.get(12)?;
                let listeners = listeners_by_ctx.get(&ctx_key).cloned().unwrap_or_default();
                Ok(ContextSummary {
                    user_uri: row.get(0)?,
                    user_name: row.get(1)?,
                    context_uri: row.get(2)?,
                    context_name: row.get(3)?,
                    context_type: row.get(4)?,
                    context_owner: row.get(5)?,
                    is_private: is_priv != 0,
                    image_url: row.get(7)?,
                    listeners,
                    unique_tracks: row.get(8)?,
                    total_plays: row.get(9)?,
                    first_seen_ms: row.get(10)?,
                    last_seen_ms: row.get(11)?,
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
        let users = single_user_vec(friend_filter);
        self.rebuild_playlist_for_users(&users, None, context_query)
    }

    #[allow(dead_code)]
    pub fn rebuild_playlist_for_users(
        &self,
        users: &[String],
        days: Option<i64>,
        context_query: &str,
    ) -> Result<Vec<RebuiltTrack>> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let trimmed_query = context_query.trim().to_lowercase();
        let is_exact_uri = trimmed_query.starts_with("spotify:");

        let mut bind_values: Vec<SqlValue> = vec![SqlValue::Integer(since_ms)];
        let user_clause =
            build_users_filter_sql(users, "user_uri", "user_name", &mut bind_values);
        let ctx_clause = if is_exact_uri {
            bind_values.push(SqlValue::Text(trimmed_query));
            "LOWER(COALESCE(context_uri, '')) = ?"
        } else {
            let ctx_pattern = format!("%{trimmed_query}%");
            bind_values.push(SqlValue::Text(ctx_pattern.clone()));
            bind_values.push(SqlValue::Text(ctx_pattern.clone()));
            bind_values.push(SqlValue::Text(ctx_pattern));
            "(LOWER(COALESCE(context_uri, '')) LIKE ? OR LOWER(COALESCE(context_name, '')) LIKE ? OR LOWER(COALESCE(context_type, '')) LIKE ?)"
        };

        let sql = format!(
            "
            SELECT
                track_uri,
                track_name,
                COALESCE(NULLIF(all_artists, ''), artist_name) AS all_artists_display,
                album_name,
                MAX(NULLIF(cover_url, '')) AS cover_url,
                duration_ms,
                release_date,
                popularity,
                COUNT(*) AS play_count,
                MIN(played_at_ms) AS first_seen_ms,
                MAX(played_at_ms) AS last_seen_ms
            FROM listens
            WHERE played_at_ms >= ?
              AND {user_clause}
              AND {ctx_clause}
            GROUP BY track_uri
            ORDER BY play_count DESC, last_seen_ms DESC
            "
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(bind_values.iter()), |row| {
                let track_uri: String = row.get(0)?;
                let spotify_url = spotify_uri_to_url(&track_uri);
                Ok(RebuiltTrack {
                    track_uri,
                    track_name: row.get(1)?,
                    all_artists: row.get(2)?,
                    album_name: row.get(3)?,
                    cover_url: row.get(4)?,
                    duration_ms: row.get(5)?,
                    release_date: row.get(6)?,
                    popularity: row.get(7)?,
                    play_count: row.get(8)?,
                    first_seen_ms: row.get(9)?,
                    last_seen_ms: row.get(10)?,
                    spotify_url,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}
