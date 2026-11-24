mod contexts;
mod listens;
mod overlap;
mod profiles;
mod schema;
mod stats;

#[cfg(test)]
mod tests;

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone};
use rusqlite::Connection;

#[allow(unused_imports)]
pub use contexts::{ContextSummary, RebuiltTrack};
#[allow(unused_imports)]
pub use listens::ListenRecord;
#[allow(unused_imports)]
pub use overlap::{FriendPairOverlap, OverlapReport, SharedItem, TrendsetterTrack};
#[allow(unused_imports)]
pub use profiles::{ProfileDiff, ProfileEvent, StoredProfile};
#[allow(unused_imports)]
pub use stats::StatsSummary;

pub struct Database {
    pub(super) conn: Connection,
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

        schema::initialize_schema(&conn)?;

        Ok(Self { conn })
    }
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
