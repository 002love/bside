use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike, Utc};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use super::{Database, millis_to_local};

#[derive(Debug, Clone, Serialize)]
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

impl Database {
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

        struct AggregateStatsRow {
            total_plays: i64,
            unique_tracks: i64,
            unique_artists: i64,
            first_played_ms: Option<i64>,
            last_played_ms: Option<i64>,
            total_listened_ms: i64,
            known_duration_plays: i64,
            skipped_plays: i64,
            completed_plays: i64,
            avg_popularity: Option<f64>,
            explicit_plays: i64,
        }

        let agg = self.conn.query_row(
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
                Ok(AggregateStatsRow {
                    total_plays: row.get(0)?,
                    unique_tracks: row.get(1)?,
                    unique_artists: row.get(2)?,
                    first_played_ms: row.get(3)?,
                    last_played_ms: row.get(4)?,
                    total_listened_ms: row.get(5)?,
                    known_duration_plays: row.get(6)?,
                    skipped_plays: row.get(7)?,
                    completed_plays: row.get(8)?,
                    avg_popularity: row.get(9)?,
                    explicit_plays: row.get(10)?,
                })
            },
        )?;
        let AggregateStatsRow {
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
        } = agg;

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
