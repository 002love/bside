use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use serde::Serialize;

use crate::spotify::{NamedUri, PublicPlaylist, UserProfileSnapshot};

use super::{Database, build_users_filter_sql, format_timestamp_local, single_user_vec};

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

impl Database {
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
        let users = single_user_vec(friend_filter);
        self.query_profile_events_internal(&users, days, limit)
    }

    #[allow(dead_code)]
    pub fn query_profile_events_for_users(
        &self,
        users: &[String],
        days: Option<i64>,
        limit: usize,
    ) -> Result<Vec<ProfileEvent>> {
        self.query_profile_events_internal(users, days, Some(limit))
    }

    fn query_profile_events_internal(
        &self,
        users: &[String],
        days: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<ProfileEvent>> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);
        let limit_val = limit.map(|l| l as i64).unwrap_or(-1);

        let mut bind_values: Vec<SqlValue> = vec![SqlValue::Integer(since_ms)];
        let user_clause =
            build_users_filter_sql(users, "user_uri", "user_name", &mut bind_values);
        bind_values.push(SqlValue::Integer(limit_val));

        let sql = format!(
            "
            SELECT id, detected_at_ms, user_uri, user_name, event_type, summary
            FROM profile_events
            WHERE detected_at_ms >= ?
              AND {user_clause}
            ORDER BY detected_at_ms DESC, id DESC
            LIMIT ?
            "
        );
        let mut stmt = self.conn.prepare(&sql)?;

        let rows = stmt
            .query_map(params_from_iter(bind_values.iter()), |row| {
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
        let users = single_user_vec(friend_filter);
        self.query_profiles_for_users(&users)
    }

    #[allow(dead_code)]
    pub fn query_profiles_for_users(&self, users: &[String]) -> Result<Vec<StoredProfile>> {
        let mut bind_values: Vec<SqlValue> = Vec::new();
        let user_clause =
            build_users_filter_sql(users, "user_uri", "user_name", &mut bind_values);

        let sql = format!(
            "
            SELECT
                user_uri, user_name, image_url, color,
                followers_count, following_count,
                recently_played_artists_json, first_seen_ms, updated_at_ms
            FROM friend_profiles
            WHERE {user_clause}
            ORDER BY user_name COLLATE NOCASE ASC
            "
        );
        let mut stmt = self.conn.prepare(&sql)?;

        let raw_profiles = stmt
            .query_map(params_from_iter(bind_values.iter()), row_to_stored_profile)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        self.attach_public_playlists(raw_profiles)
    }

    #[allow(dead_code)]
    pub fn get_profile_by_uri(&self, user_uri: &str) -> Result<Option<StoredProfile>> {
        let profile = self
            .conn
            .query_row(
                "
                SELECT
                    user_uri, user_name, image_url, color,
                    followers_count, following_count,
                    recently_played_artists_json, first_seen_ms, updated_at_ms
                FROM friend_profiles
                WHERE LOWER(user_uri) = LOWER(?1)
                LIMIT 1
                ",
                params![user_uri],
                row_to_stored_profile,
            )
            .optional()?;

        match profile {
            Some(p) => Ok(self.attach_public_playlists(vec![p])?.into_iter().next()),
            None => Ok(None),
        }
    }

    fn attach_public_playlists(
        &self,
        raw_profiles: Vec<StoredProfile>,
    ) -> Result<Vec<StoredProfile>> {
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
}

fn row_to_stored_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredProfile> {
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
}
