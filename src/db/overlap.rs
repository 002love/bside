use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde::Serialize;

use super::{Database, build_users_filter_sql};

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

impl Database {
    #[allow(dead_code)]
    pub fn query_overlap(
        &self,
        days: Option<i64>,
        friends_filter: &[String],
    ) -> Result<OverlapReport> {
        let since_ms = days
            .map(|d| (Utc::now() - Duration::days(d)).timestamp_millis())
            .unwrap_or(0);

        let mut bind_values: Vec<SqlValue> = vec![SqlValue::Integer(since_ms)];
        let user_clause =
            build_users_filter_sql(friends_filter, "user_uri", "user_name", &mut bind_values);

        let sql = format!(
            "
            SELECT user_uri, user_name, artist_name, track_uri, track_name, played_at_ms
            FROM listens
            WHERE played_at_ms >= ?
              AND {user_clause}
            ORDER BY played_at_ms ASC
            "
        );
        let mut stmt = self.conn.prepare(&sql)?;

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

        let rows = stmt.query_map(params_from_iter(bind_values.iter()), |row| {
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
