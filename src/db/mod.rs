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
use rusqlite::{Connection, types::Value as SqlValue};

#[allow(unused_imports)]
pub use contexts::{ContextListener, ContextSummary, RebuiltTrack};
#[allow(unused_imports)]
pub use listens::{FriendSummary, LatestUserListen, ListenRecord, PaginatedHistory, RepairReport};
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

pub(super) fn single_user_vec(friend_filter: Option<&str>) -> Vec<String> {
    friend_filter
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .map(|f| vec![f])
        .unwrap_or_default()
}

/// Builds a parameterized SQL predicate matching any of `users` against `uri_col` or `name_col`,
/// appending bound values to `bind_values`. Returns `"1=1"` when `users` has no non-empty entries.
pub(super) fn build_users_filter_sql(
    users: &[String],
    uri_col: &str,
    name_col: &str,
    bind_values: &mut Vec<SqlValue>,
) -> String {
    let clean_users: Vec<String> = users
        .iter()
        .map(|u| u.trim().to_lowercase())
        .filter(|u| !u.is_empty())
        .collect();

    if clean_users.is_empty() {
        return "1=1".to_string();
    }

    let mut parts = Vec::with_capacity(clean_users.len());
    for u in clean_users {
        parts.push(format!(
            "(LOWER({uri_col}) = ? OR LOWER({name_col}) LIKE ? OR LOWER({uri_col}) LIKE ?)"
        ));
        let like = format!("%{u}%");
        bind_values.push(SqlValue::Text(u));
        bind_values.push(SqlValue::Text(like.clone()));
        bind_values.push(SqlValue::Text(like));
    }

    format!("({})", parts.join(" OR "))
}
