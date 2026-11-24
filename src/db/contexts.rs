use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use super::{Database, spotify_uri_to_url};

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

impl Database {
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
}
