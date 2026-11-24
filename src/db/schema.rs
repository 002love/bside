use std::collections::HashSet;

use anyhow::{Context, Result};
use rusqlite::Connection;

pub(super) const LISTENS_MIGRATIONS: &[(&str, &str)] = &[
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

pub(super) fn initialize_schema(conn: &Connection) -> Result<()> {
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

    migrate_listens_table(conn)?;

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

    Ok(())
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
