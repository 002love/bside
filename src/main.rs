mod db;
mod format;
mod spotify;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use tokio::time::sleep;

use crate::db::{Database, ProfileDiff, escape_csv, format_timestamp_local};
use crate::format::{
    ExportTrackRow, format_duration_ms, format_m3u8, format_relative_age_secs,
    format_spotify_urls, format_total_duration_ms, render_activity_heatmap, render_bar,
    render_hourly_chart, render_weekday_chart, spotify_uri_to_url, truncate_ellipsis,
};
use crate::spotify::{
    FriendActivity, NameCache, connect_cached, fetch_buddylist, fetch_buddylist_raw,
    fetch_user_profile, login_oauth,
};

#[derive(Parser)]
#[command(
    name = "bside",
    version,
    about = "The flip side of Spotify — self-hosted listening analytics, skip/repeat tracking, and playlist reconstruction via librespot"
)]
struct Cli {
    /// Directory for cached Spotify session credentials
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,

    /// Path to SQLite database file
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authorize with Spotify via browser OAuth (saved locally for future runs)
    Login {
        /// Do not auto-open browser; only print the OAuth URL
        #[arg(long)]
        no_browser: bool,

        /// Custom OAuth redirect URI (default: http://127.0.0.1:8898/login)
        #[arg(long)]
        redirect_uri: Option<String>,
    },

    /// List all friends currently visible in your Spotify Friend Activity
    Friends {
        /// Optional filter by friend name or URI substring
        #[arg(short, long)]
        friend: Vec<String>,

        /// Print raw JSON response from Spotify spclient (for debugging)
        #[arg(long)]
        raw: bool,
    },

    /// Continuously poll Friend Activity and log listens, skips, repeats, sessions, and profile changes
    Watch {
        /// Filter to specific friend(s) by name or Spotify URI (repeatable; omit to log all)
        #[arg(short, long)]
        friend: Vec<String>,

        /// Polling interval in seconds (default 15, minimum 10)
        #[arg(short, long, default_value_t = 15)]
        interval: u64,

        /// Profile sync interval in seconds (default 1800, 0 to disable)
        #[arg(long, default_value_t = 1800)]
        profile_interval: u64,

        /// Maximum consecutive inferred repeat cycles per single continuous track play
        #[arg(long, default_value_t = 10)]
        max_inferred_repeats: u32,
    },

    /// Show stored friend profiles, public playlists, and recently played artists
    Profiles {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Fetch fresh profile snapshots from Spotify before displaying
        #[arg(long)]
        refresh: bool,
    },

    /// Show chronological changelog of friend profile and playlist changes
    Changes {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only show changes from the last N days
        #[arg(short, long)]
        days: Option<i64>,

        /// Maximum number of events to show
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// List all discovered listening contexts (Liked Songs, private/public playlists, albums, artists)
    Contexts {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only show private contexts (Liked Songs and inaccessible private playlists)
        #[arg(long)]
        private_only: bool,
    },

    /// Rebuild a hidden/private playlist or Liked Songs collection from logged listens
    Rebuild {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Context name, URI, or type substring (e.g. "liked_songs", playlist URI, or playlist name)
        #[arg(short, long)]
        context: String,

        /// Output format
        #[arg(long, value_enum, default_value_t = RebuildFormat::Table)]
        format: RebuildFormat,

        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Show rich listening statistics, streaks, charts, and 7x24 activity heatmap
    Stats {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only include listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,
    },

    /// Analyze friend taste overlap, compatibility scores, and trendsetters
    Overlap {
        /// Only include listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,

        /// Filter to specific friend(s) by name or Spotify URI (repeatable)
        #[arg(short, long)]
        friend: Vec<String>,
    },

    /// Show logged listening history from SQLite
    History {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only show listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,

        /// Maximum number of rows to show
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// Export listening history to CSV or JSON
    Export {
        /// Output format
        #[arg(short, long, value_enum, default_value_t = ExportFormat::Csv)]
        format: ExportFormat,

        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Filter by friend name or URI substring
        #[arg(short = 'F', long)]
        friend: Option<String>,

        /// Only include listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ExportFormat {
    Csv,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum RebuildFormat {
    Table,
    Urls,
    M3u8,
    Csv,
    Json,
}

struct ActivePlay {
    track_uri: String,
    played_at_ms: i64,
    cycle_started_at_ms: i64,
    duration_ms: Option<i64>,
    inferred_cycles: u32,
    is_playing: bool,
    session_id: Option<i64>,
    last_activity: FriendActivity,
    missed_polls: u32,
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bside")
}

fn resolve_cache_dir(arg: Option<PathBuf>) -> PathBuf {
    arg.unwrap_or_else(|| default_data_dir().join("cache"))
}

fn resolve_db_path(arg: Option<PathBuf>) -> PathBuf {
    arg.unwrap_or_else(|| default_data_dir().join("history.db"))
}

fn format_context_badge(item: &FriendActivity) -> String {
    let ctx_name = item.context_name();
    let ctx_uri = item.context_uri();
    let is_private = item.is_private_context();
    let owner = item.context_owner();

    let label = match (ctx_name, ctx_uri) {
        (Some(name), _) => name.to_string(),
        (None, Some(uri)) if is_private => format!("Private Playlist ({uri})"),
        (None, Some(uri)) => uri.to_string(),
        (None, None) => return String::new(),
    };

    let mut details = Vec::new();
    if is_private {
        details.push("PRIVATE".to_string());
    }
    if let Some(o) = owner {
        details.push(format!("by {o}"));
    }

    if details.is_empty() {
        format!(" [{label}]")
    } else {
        format!(" [{label} ({})]", details.join(", "))
    }
}

fn format_release_year(release_date: Option<&str>) -> Option<&str> {
    release_date.and_then(|d| d.get(0..4)).filter(|y| y.len() == 4)
}

fn is_likely_skipped(elapsed_ms: i64, duration_ms: Option<i64>) -> bool {
    match duration_ms {
        Some(d) if d >= 30_000 => {
            let threshold = (d / 2).min(d - 15_000);
            elapsed_ms < threshold
        }
        _ => false,
    }
}

fn print_profile_diff(user_name: &str, diff: &ProfileDiff) {
    if let Some(old_name) = &diff.renamed_from {
        println!("[PROFILE] {user_name} renamed from '{old_name}'");
    }
    if let Some(delta) = diff.followers_delta
        && delta != 0
    {
        println!("[PROFILE] {user_name} followers {delta:+}");
    }
    for pl in &diff.new_playlists {
        println!("[PROFILE] {user_name} added public playlist '{pl}'");
    }
    for pl in &diff.removed_playlists {
        println!("[PROFILE] {user_name} removed public playlist '{pl}'");
    }
    for artist in &diff.new_recent_artists {
        println!("[PROFILE] {user_name} new recently played artist: {artist}");
    }
}

fn print_live_listen_line(item: &FriendActivity) {
    let ts = format_timestamp_local(item.timestamp);
    let badge = if item.playing { "[LIVE]" } else { "[LAST]" };
    let type_badge = if item.item_type() == "episode" {
        " [PODCAST]"
    } else {
        ""
    };
    let explicit_badge = if item.track.explicit == Some(true) {
        " [E]"
    } else {
        ""
    };
    let dur = item
        .duration_ms()
        .map(|ms| format!(" ({})", format_duration_ms(ms)))
        .unwrap_or_default();
    let ctx = format_context_badge(item);

    println!(
        "[{ts}] {badge}{type_badge} {} | {} — {}{explicit_badge}{dur}{ctx}",
        item.user.name,
        item.all_artists_display(),
        item.track.name,
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cache_dir = resolve_cache_dir(cli.cache_dir);
    let db_path = resolve_db_path(cli.db);

    match cli.command {
        Commands::Login {
            no_browser,
            redirect_uri,
        } => {
            println!("Starting Spotify OAuth login...");
            let session = login_oauth(&cache_dir, !no_browser, redirect_uri.as_deref()).await?;
            println!(
                "Logged in as '{}' — credentials saved to {}",
                session.username(),
                cache_dir.display()
            );
            session.shutdown();
        }

        Commands::Friends { friend, raw } => {
            let session = connect_cached(&cache_dir).await?;
            if raw {
                let base_url = session.spclient().base_url().await.unwrap_or_default();
                eprintln!(
                    "Account: {} | Country: {} | SpClient: {}",
                    session.username(),
                    session.country(),
                    base_url
                );
                let body = fetch_buddylist_raw(&session).await?;
                session.shutdown();
                println!("{body}");
                return Ok(());
            }
            let mut name_cache = NameCache::default();
            let friends = fetch_buddylist(&session, &mut name_cache).await?;
            session.shutdown();

            let matched: Vec<_> = friends
                .into_iter()
                .filter(|f| f.matches_filter(&friend))
                .collect();

            if matched.is_empty() {
                println!("No matching friends found in Friend Activity.");
                return Ok(());
            }

            let now_secs = Utc::now().timestamp();
            println!("Found {} friend(s) sharing activity:\n", matched.len());
            for item in matched {
                let ts = format_timestamp_local(item.timestamp);
                let age_secs = (now_secs - (item.timestamp / 1_000)).max(0);
                let age_str = format_relative_age_secs(age_secs);

                let mut badges = Vec::new();
                if item.playing {
                    badges.push("[LIVE]");
                }
                if item.item_type() == "episode" {
                    badges.push("[PODCAST]");
                }
                let badge_str = if badges.is_empty() {
                    String::new()
                } else {
                    format!(" {}", badges.join(" "))
                };

                let mut meta_parts = Vec::new();
                if let Some(dur) = item.duration_ms() {
                    meta_parts.push(format_duration_ms(dur));
                }
                if item.track.explicit == Some(true) {
                    meta_parts.push("[E]".to_string());
                }
                if let Some(year) = format_release_year(item.track.release_date.as_deref()) {
                    meta_parts.push(year.to_string());
                }
                if let Some(pop) = item.track.popularity {
                    meta_parts.push(format!("pop: {pop}"));
                }
                let meta_str = if meta_parts.is_empty() {
                    String::new()
                } else {
                    format!("  ({})", meta_parts.join(" · "))
                };

                let ctx = format_context_badge(&item);
                println!(
                    "{} ({}){}\n  {} — {}{}{}\n  Last activity: {} ({})\n",
                    item.user.name,
                    item.user.uri,
                    badge_str,
                    item.all_artists_display(),
                    item.track.name,
                    meta_str,
                    ctx,
                    ts,
                    age_str,
                );
            }
        }

        Commands::Watch {
            friend,
            interval,
            profile_interval,
            max_inferred_repeats,
        } => {
            let interval_secs = interval.max(10);
            let db = Database::open(&db_path)?;
            let mut session = connect_cached(&cache_dir).await?;

            if friend.is_empty() {
                println!(
                    "Watching ALL friends every {}s (profile sync: {}) -> {}",
                    interval_secs,
                    if profile_interval == 0 {
                        "off".to_string()
                    } else {
                        format!("{profile_interval}s")
                    },
                    db_path.display()
                );
            } else {
                println!(
                    "Watching [{}] every {}s (profile sync: {}) -> {}",
                    friend.join(", "),
                    interval_secs,
                    if profile_interval == 0 {
                        "off".to_string()
                    } else {
                        format!("{profile_interval}s")
                    },
                    db_path.display()
                );
            }
            println!("Press Ctrl+C to stop.\n");

            let mut consecutive_errors: u32 = 0;
            let mut name_cache = NameCache::default();
            let mut active_plays: HashMap<String, ActivePlay> = HashMap::new();
            let mut known_friend_uris: BTreeSet<String> = BTreeSet::new();
            let mut last_profile_sync: Option<Instant> = None;

            loop {
                if session.is_invalid() {
                    eprintln!("[warn] Spotify session disconnected, reconnecting...");
                    match connect_cached(&cache_dir).await {
                        Ok(new_session) => {
                            session = new_session;
                            eprintln!("[info] Reconnected.");
                        }
                        Err(e) => {
                            eprintln!("[error] Reconnect failed: {e}");
                        }
                    }
                }

                match fetch_buddylist(&session, &mut name_cache).await {
                    Ok(list) => {
                        consecutive_errors = 0;
                        let now_ms = Utc::now().timestamp_millis();
                        let mut seen_users: HashSet<String> = HashSet::new();

                        for item in list.into_iter().filter(|f| f.matches_filter(&friend)) {
                            let user_uri = item.user.uri.clone();
                            seen_users.insert(user_uri.clone());
                            known_friend_uris.insert(user_uri.clone());

                            let is_same_play = active_plays.get(&user_uri).is_some_and(|prev| {
                                prev.track_uri == item.track.uri
                                    && prev.played_at_ms == item.timestamp
                            });

                            if !is_same_play {
                                let mut session_id = None;
                                if let Some(prev) = active_plays.remove(&user_uri) {
                                    session_id = prev.session_id;
                                    if prev.is_playing {
                                        let elapsed_ms = (now_ms - prev.cycle_started_at_ms).max(0);
                                        if let Err(e) = db.finalize_play(
                                            &user_uri,
                                            &prev.track_uri,
                                            prev.cycle_started_at_ms,
                                            now_ms,
                                            prev.duration_ms,
                                            true,
                                        ) {
                                            eprintln!("[error] finalize_play failed: {e}");
                                        } else if is_likely_skipped(elapsed_ms, prev.duration_ms) {
                                            let dur_str = prev
                                                .duration_ms
                                                .map(format_duration_ms)
                                                .unwrap_or_else(|| "?".to_string());
                                            println!(
                                                "[{}] [SKIP] {} skipped {} — {} after {} / {}",
                                                format_timestamp_local(now_ms),
                                                prev.last_activity.user.name,
                                                prev.last_activity.all_artists_display(),
                                                prev.last_activity.track.name,
                                                format_duration_ms(elapsed_ms),
                                                dur_str,
                                            );
                                        }
                                    }
                                    if !item.playing
                                        && let Some(sid) = session_id.take()
                                    {
                                        let _ = db.end_session(sid, now_ms);
                                    }
                                }

                                let inserted = match db.insert_listen(&item) {
                                    Ok(ins) => ins,
                                    Err(e) => {
                                        eprintln!("[error] DB insert failed: {e}");
                                        false
                                    }
                                };

                                if item.playing {
                                    if let Some(sid) = session_id {
                                        let _ = db.touch_session(sid, now_ms, true);
                                    } else {
                                        let start_ms = item.timestamp.min(now_ms);
                                        match db.start_session(&user_uri, &item.user.name, start_ms)
                                        {
                                            Ok(sid) => {
                                                let _ = db.touch_session(sid, now_ms, true);
                                                session_id = Some(sid);
                                            }
                                            Err(e) => {
                                                eprintln!("[error] start_session failed: {e}");
                                            }
                                        }
                                    }
                                }

                                if inserted {
                                    print_live_listen_line(&item);
                                }

                                active_plays.insert(
                                    user_uri,
                                    ActivePlay {
                                        track_uri: item.track.uri.clone(),
                                        played_at_ms: item.timestamp,
                                        cycle_started_at_ms: item.timestamp,
                                        duration_ms: item.duration_ms(),
                                        inferred_cycles: 0,
                                        is_playing: item.playing,
                                        session_id,
                                        last_activity: item,
                                        missed_polls: 0,
                                    },
                                );
                            } else if let Some(prev) = active_plays.get_mut(&user_uri) {
                                prev.missed_polls = 0;
                                prev.last_activity = item.clone();

                                if prev.is_playing && !item.playing {
                                    let elapsed_ms = (now_ms - prev.cycle_started_at_ms).max(0);
                                    if let Err(e) = db.finalize_play(
                                        &user_uri,
                                        &prev.track_uri,
                                        prev.cycle_started_at_ms,
                                        now_ms,
                                        prev.duration_ms,
                                        false,
                                    ) {
                                        eprintln!("[error] finalize_play failed: {e}");
                                    }
                                    if let Some(sid) = prev.session_id.take() {
                                        let _ = db.end_session(sid, now_ms);
                                    }
                                    prev.is_playing = false;
                                    println!(
                                        "[{}] [PAUSE/END] {} stopped {} — {} ({})",
                                        format_timestamp_local(now_ms),
                                        item.user.name,
                                        item.all_artists_display(),
                                        item.track.name,
                                        format_duration_ms(elapsed_ms),
                                    );
                                } else if !prev.is_playing && item.playing {
                                    let _ = db.insert_listen(&item);
                                    if prev.session_id.is_none()
                                        && let Ok(sid) =
                                            db.start_session(&user_uri, &item.user.name, now_ms)
                                    {
                                        prev.session_id = Some(sid);
                                    }
                                    prev.is_playing = true;
                                } else if prev.is_playing && item.playing {
                                    if let Some(sid) = prev.session_id {
                                        let _ = db.touch_session(sid, now_ms, false);
                                    }

                                    if let Some(dur) = prev.duration_ms
                                        && dur >= 30_000
                                        && prev.inferred_cycles < max_inferred_repeats
                                        && (now_ms - prev.cycle_started_at_ms) >= dur
                                    {
                                        let cycle_end = prev.cycle_started_at_ms + dur;
                                        let _ = db.finalize_play(
                                            &user_uri,
                                            &prev.track_uri,
                                            prev.cycle_started_at_ms,
                                            cycle_end,
                                            Some(dur),
                                            false,
                                        );
                                        prev.cycle_started_at_ms = cycle_end;
                                        prev.inferred_cycles += 1;

                                        match db.insert_inferred_repeat(&item, cycle_end) {
                                            Ok(true) => {
                                                if let Some(sid) = prev.session_id {
                                                    let _ = db.touch_session(sid, now_ms, true);
                                                }
                                                println!(
                                                    "[{}] [REPEAT #{}] {} | {} — {} ({})",
                                                    format_timestamp_local(cycle_end),
                                                    prev.inferred_cycles,
                                                    item.user.name,
                                                    item.all_artists_display(),
                                                    item.track.name,
                                                    format_duration_ms(dur),
                                                );
                                            }
                                            Ok(false) => {}
                                            Err(e) => {
                                                eprintln!(
                                                    "[error] insert_inferred_repeat failed: {e}"
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Finalize friends who disappeared from the live feed while playing.
                        for (user_uri, state) in active_plays.iter_mut() {
                            if !seen_users.contains(user_uri) && state.is_playing {
                                state.missed_polls += 1;
                                if state.missed_polls > 2 {
                                    let _ = db.finalize_play(
                                        user_uri,
                                        &state.track_uri,
                                        state.cycle_started_at_ms,
                                        now_ms,
                                        state.duration_ms,
                                        false,
                                    );
                                    if let Some(sid) = state.session_id.take() {
                                        let _ = db.end_session(sid, now_ms);
                                    }
                                    state.is_playing = false;
                                    println!(
                                        "[{}] [OFFLINE] {} left feed during {} — {}",
                                        format_timestamp_local(now_ms),
                                        state.last_activity.user.name,
                                        state.last_activity.all_artists_display(),
                                        state.last_activity.track.name,
                                    );
                                }
                            }
                        }

                        // Periodic profile sync.
                        if profile_interval > 0
                            && !known_friend_uris.is_empty()
                            && last_profile_sync.is_none_or(|t| {
                                t.elapsed() >= Duration::from_secs(profile_interval)
                            })
                        {
                            for uri in &known_friend_uris {
                                match fetch_user_profile(&session, uri).await {
                                    Ok(snap) => match db.upsert_profile(&snap) {
                                        Ok(diff) => print_profile_diff(&snap.user_name, &diff),
                                        Err(e) => {
                                            eprintln!("[error] upsert_profile failed: {e}");
                                        }
                                    },
                                    Err(e) => {
                                        eprintln!("[warn] Profile sync failed for {uri}: {e}");
                                    }
                                }
                            }
                            last_profile_sync = Some(Instant::now());
                        }
                    }
                    Err(e) => {
                        consecutive_errors = consecutive_errors.saturating_add(1);
                        eprintln!("[error] Poll failed (#{consecutive_errors}): {e}");
                        if consecutive_errors >= 3 {
                            session.shutdown();
                        }
                    }
                }

                let backoff_secs = if consecutive_errors > 0 {
                    (interval_secs * (1u64 << consecutive_errors.min(4))).min(600)
                } else {
                    interval_secs
                };

                tokio::select! {
                    _ = sleep(Duration::from_secs(backoff_secs)) => {}
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nStopping watcher...");
                        break;
                    }
                }
            }

            // Close all open listening sessions cleanly in SQLite before exiting.
            let end_now_ms = Utc::now().timestamp_millis();
            for state in active_plays.values_mut() {
                if let Some(sid) = state.session_id.take() {
                    let _ = db.end_session(sid, end_now_ms);
                }
            }
            session.shutdown();
        }

        Commands::Profiles { friend, refresh } => {
            let db = Database::open(&db_path)?;

            if refresh {
                let session = connect_cached(&cache_dir).await?;
                let mut name_cache = NameCache::default();
                let filters: Vec<String> = friend.iter().cloned().collect();
                let mut uris: BTreeSet<String> = BTreeSet::new();

                if let Ok(feed) = fetch_buddylist(&session, &mut name_cache).await {
                    for item in feed.into_iter().filter(|f| f.matches_filter(&filters)) {
                        uris.insert(item.user.uri);
                    }
                }
                if let Some(f) = friend.as_deref()
                    && f.starts_with("spotify:user:")
                {
                    uris.insert(f.to_string());
                }
                // Also refresh any already-stored profiles matching the filter.
                if let Ok(existing) = db.query_profiles(friend.as_deref()) {
                    for p in existing {
                        uris.insert(p.user_uri);
                    }
                }

                if uris.is_empty() {
                    println!("No matching friends found to refresh.");
                } else {
                    println!("Refreshing {} friend profile(s)...", uris.len());
                    for uri in &uris {
                        match fetch_user_profile(&session, uri).await {
                            Ok(snap) => match db.upsert_profile(&snap) {
                                Ok(diff) => print_profile_diff(&snap.user_name, &diff),
                                Err(e) => eprintln!("[error] upsert_profile failed: {e}"),
                            },
                            Err(e) => eprintln!("[warn] Failed to fetch profile for {uri}: {e}"),
                        }
                    }
                    println!();
                }
                session.shutdown();
            }

            let profiles = db.query_profiles(friend.as_deref())?;
            if profiles.is_empty() {
                println!(
                    "No stored profiles found. Run `bside profiles --refresh` or `watch` first."
                );
                return Ok(());
            }

            for p in profiles {
                let url = spotify_uri_to_url(&p.user_uri);
                let followers = p
                    .followers_count
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let following = p
                    .following_count
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".to_string());

                println!("=== {} ===", p.user_name);
                println!("  URI:       {} ({})", p.user_uri, url);
                println!("  Social:    {followers} followers · {following} following");
                println!(
                    "  Updated:   {} (first seen {})",
                    format_timestamp_local(p.updated_at_ms),
                    format_timestamp_local(p.first_seen_ms)
                );

                if !p.recently_played_artists.is_empty() {
                    let artists: Vec<&str> = p
                        .recently_played_artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !artists.is_empty() {
                        println!("  Recent artists: {}", artists.join(", "));
                    }
                }

                if !p.public_playlists.is_empty() {
                    println!("  Public playlists ({}):", p.public_playlists.len());
                    for pl in &p.public_playlists {
                        let pl_followers = pl
                            .followers_count
                            .map(|n| format!(" ({n} followers)"))
                            .unwrap_or_default();
                        println!(
                            "    • {}{} — {}",
                            pl.name,
                            pl_followers,
                            spotify_uri_to_url(&pl.uri)
                        );
                    }
                }
                println!();
            }
        }

        Commands::Changes {
            friend,
            days,
            limit,
        } => {
            let db = Database::open(&db_path)?;
            let events = db.query_profile_events(friend.as_deref(), days, Some(limit))?;
            if events.is_empty() {
                println!("No profile or playlist changes recorded yet.");
                return Ok(());
            }

            for ev in events {
                let badge = match ev.event_type.as_str() {
                    "rename" => "RENAME",
                    "followers" => "FOLLOWERS",
                    "playlist_added" => "+PLAYLIST",
                    "playlist_removed" => "-PLAYLIST",
                    "recent_artist" => "ARTIST",
                    other => other,
                };
                println!(
                    "[{}] [{:<9}] {} — {}",
                    ev.detected_at_local, badge, ev.user_name, ev.summary
                );
            }
        }

        Commands::Contexts {
            friend,
            private_only,
        } => {
            let db = Database::open(&db_path)?;
            let contexts = db.query_contexts(friend.as_deref(), private_only)?;
            if contexts.is_empty() {
                println!("No matching listening contexts found.");
                return Ok(());
            }

            println!("Discovered {} context(s):\n", contexts.len());
            for c in contexts {
                let title = if !c.context_name.is_empty() {
                    c.context_name.clone()
                } else {
                    c.context_uri.clone()
                };
                let priv_badge = if c.is_private { " [PRIVATE]" } else { "" };
                let owner_str = c
                    .context_owner
                    .as_deref()
                    .map(|o| format!(" (by {o})"))
                    .unwrap_or_default();
                let url_suffix = if !c.context_uri.is_empty() {
                    let u = spotify_uri_to_url(&c.context_uri);
                    if u != c.context_uri {
                        format!("  {u}")
                    } else {
                        format!("  {}", c.context_uri)
                    }
                } else {
                    String::new()
                };

                println!(
                    "{} | [{}]{} {}{}",
                    c.user_name,
                    c.context_type,
                    priv_badge,
                    truncate_ellipsis(&title, 48),
                    owner_str,
                );
                println!(
                    "  {} unique tracks · {} plays · last seen {}{}",
                    c.unique_tracks,
                    c.total_plays,
                    format_timestamp_local(c.last_seen_ms),
                    url_suffix,
                );
            }
        }

        Commands::Rebuild {
            friend,
            context,
            format,
            out,
        } => {
            let db = Database::open(&db_path)?;
            let tracks = db.rebuild_playlist(friend.as_deref(), &context)?;
            if tracks.is_empty() {
                println!("No tracks found matching context '{context}'.");
                return Ok(());
            }

            let export_rows: Vec<ExportTrackRow> = tracks
                .iter()
                .map(|t| ExportTrackRow {
                    track_uri: t.track_uri.clone(),
                    spotify_url: t.spotify_url.clone(),
                    artist: t.all_artists.clone(),
                    title: t.track_name.clone(),
                    album: t.album_name.clone().unwrap_or_default(),
                    duration_ms: t.duration_ms,
                    play_count: t.play_count,
                })
                .collect();

            let mut writer: Box<dyn Write> = match &out {
                Some(path) => Box::new(
                    File::create(path)
                        .with_context(|| format!("Failed to create {}", path.display()))?,
                ),
                None => Box::new(io::stdout().lock()),
            };

            match format {
                RebuildFormat::Table => {
                    writeln!(
                        writer,
                        "Rebuilt '{}' — {} unique track(s):\n",
                        context,
                        tracks.len()
                    )?;
                    for (i, (t, row)) in tracks.iter().zip(export_rows.iter()).enumerate() {
                        let dur = row
                            .duration_ms
                            .map(format_duration_ms)
                            .unwrap_or_else(|| "?:??".to_string());
                        let year = format_release_year(t.release_date.as_deref()).unwrap_or("----");
                        let album_part = if !row.album.is_empty() {
                            format!(" · {}", truncate_ellipsis(&row.album, 24))
                        } else {
                            String::new()
                        };
                        writeln!(
                            writer,
                            "{:>3}. [{:>2}x] {} — {} ({}, {}{})\n     {}",
                            i + 1,
                            row.play_count,
                            truncate_ellipsis(&row.artist, 32),
                            truncate_ellipsis(&row.title, 36),
                            dur,
                            year,
                            album_part,
                            row.spotify_url,
                        )?;
                    }
                }
                RebuildFormat::Urls => {
                    write!(writer, "{}", format_spotify_urls(&export_rows))?;
                }
                RebuildFormat::M3u8 => {
                    write!(writer, "{}", format_m3u8(&context, &export_rows))?;
                }
                RebuildFormat::Csv => {
                    writeln!(
                        writer,
                        "play_count,artist,track,album,duration_ms,release_date,popularity,track_uri,spotify_url"
                    )?;
                    for t in &tracks {
                        writeln!(
                            writer,
                            "{},{},{},{},{},{},{},{},{}",
                            t.play_count,
                            escape_csv(&t.all_artists),
                            escape_csv(&t.track_name),
                            escape_csv(t.album_name.as_deref().unwrap_or("")),
                            t.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
                            escape_csv(t.release_date.as_deref().unwrap_or("")),
                            t.popularity.map(|p| p.to_string()).unwrap_or_default(),
                            escape_csv(&t.track_uri),
                            escape_csv(&t.spotify_url),
                        )?;
                    }
                }
                RebuildFormat::Json => {
                    serde_json::to_writer_pretty(&mut writer, &tracks)?;
                    writeln!(writer)?;
                }
            }

            if let Some(path) = out {
                eprintln!(
                    "Wrote {} rebuilt tracks for '{}' to {}",
                    tracks.len(),
                    context,
                    path.display()
                );
            }
        }

        Commands::Stats { friend, days } => {
            let db = Database::open(&db_path)?;
            let stats = db.query_stats(friend.as_deref(), days)?;

            if stats.total_plays == 0 {
                println!("No listens recorded yet for this filter.");
                return Ok(());
            }

            println!("=== Listening Stats ===");
            println!("Total plays:     {}", stats.total_plays);
            println!(
                "Listening time:  {} (from {}/{} plays with known duration)",
                format_total_duration_ms(stats.total_listened_ms),
                stats.known_duration_plays,
                stats.total_plays
            );
            println!(
                "Catalog:         {} unique tracks · {} unique artists",
                stats.unique_tracks, stats.unique_artists
            );
            println!(
                "Activity:        {} active day(s) · current streak: {}d · longest streak: {}d",
                stats.active_days, stats.current_streak_days, stats.longest_streak_days
            );
            if stats.session_count > 0 {
                let avg_str = stats
                    .avg_session_ms
                    .map(format_total_duration_ms)
                    .unwrap_or_else(|| "n/a".to_string());
                println!(
                    "Sessions:        {} session(s) · avg length: {}",
                    stats.session_count, avg_str
                );
            }
            let skip_denom = stats.skipped_plays + stats.completed_plays;
            if skip_denom > 0 {
                let skip_pct = (stats.skipped_plays as f64 / skip_denom as f64) * 100.0;
                println!(
                    "Skip rate:       {:.1}% ({} skipped / {} completed)",
                    skip_pct, stats.skipped_plays, stats.completed_plays
                );
            }
            if let Some(avg_pop) = stats.avg_popularity {
                println!("Avg popularity:  {avg_pop:.1} / 100");
            }
            if let Some((artist, track, pop)) = &stats.most_obscure_track {
                println!("Most obscure:    {artist} — {track} (pop: {pop})");
            }
            if let Some((artist, track, pop)) = &stats.most_mainstream_track {
                println!("Most mainstream: {artist} — {track} (pop: {pop})");
            }
            if stats.explicit_plays > 0 {
                let exp_pct = (stats.explicit_plays as f64 / stats.total_plays as f64) * 100.0;
                println!(
                    "Explicit tracks: {} ({:.1}%)",
                    stats.explicit_plays, exp_pct
                );
            }
            if let (Some(first), Some(last)) = (stats.first_played_ms, stats.last_played_ms) {
                println!(
                    "Range:           {} -> {}",
                    format_timestamp_local(first),
                    format_timestamp_local(last)
                );
            }

            if !stats.top_tracks.is_empty() {
                println!("\nTop Tracks:");
                for (i, (artist, track, count)) in stats.top_tracks.iter().enumerate() {
                    println!(
                        "  {:2}. {} — {} ({})",
                        i + 1,
                        truncate_ellipsis(artist, 28),
                        truncate_ellipsis(track, 34),
                        count
                    );
                }
            }

            if !stats.top_artists.is_empty() {
                let max_cnt = stats.top_artists[0].1.max(1);
                println!("\nTop Artists:");
                for (i, (artist, count)) in stats.top_artists.iter().enumerate() {
                    let bar = render_bar(*count, max_cnt, 20);
                    println!(
                        "  {:2}. {:<24} {:<20} ({})",
                        i + 1,
                        truncate_ellipsis(artist, 24),
                        bar,
                        count
                    );
                }
            }

            if !stats.most_skipped_tracks.is_empty() {
                println!("\nMost Skipped Tracks:");
                for (i, (artist, track, count)) in stats.most_skipped_tracks.iter().enumerate() {
                    println!(
                        "  {:2}. {} — {} ({} skips)",
                        i + 1,
                        truncate_ellipsis(artist, 28),
                        truncate_ellipsis(track, 34),
                        count
                    );
                }
            }

            if !stats.context_breakdown.is_empty() {
                let max_cnt = stats.context_breakdown[0].1.max(1);
                println!("\nContext Breakdown:");
                for (ctx, count) in &stats.context_breakdown {
                    let bar = render_bar(*count, max_cnt, 20);
                    println!("  {:<14} {:<20} ({})", ctx, bar, count);
                }
            }

            if !stats.release_decades.is_empty() {
                let max_cnt = stats.release_decades[0].1.max(1);
                println!("\nRelease Decades:");
                for (decade, count) in &stats.release_decades {
                    let bar = render_bar(*count, max_cnt, 20);
                    println!("  {:<8} {:<20} ({})", decade, bar, count);
                }
            }

            println!("\nActivity by Weekday:");
            println!("{}", render_weekday_chart(&stats.weekday, 20));

            println!("\nActivity by Hour (local time):");
            println!("{}", render_hourly_chart(&stats.hourly, 24));

            println!("\n7x24 Activity Heatmap (local time):");
            println!("{}", render_activity_heatmap(&stats.heatmap_grid));
        }

        Commands::Overlap { days, friend } => {
            let db = Database::open(&db_path)?;
            let report = db.query_overlap(days, &friend)?;

            if report.friend_pairs.is_empty() {
                println!("Need at least 2 friends with logged listens to compute taste overlap.");
                return Ok(());
            }

            println!("=== Friend Taste Compatibility ===");
            for pair in &report.friend_pairs {
                let score_pct = (pair.compatibility_score * 100.0).round() as i64;
                let artist_pct = pair.artist_jaccard * 100.0;
                let track_pct = pair.track_jaccard * 100.0;
                let bar = render_bar(score_pct, 100, 20);
                println!(
                    "  {} <-> {}: {:>3}% {:<20} (artists: {:.1}% [{}/{}], tracks: {:.1}% [{}/{}])",
                    truncate_ellipsis(&pair.friend_a, 18),
                    truncate_ellipsis(&pair.friend_b, 18),
                    score_pct,
                    bar,
                    artist_pct,
                    pair.shared_artists,
                    pair.total_union_artists,
                    track_pct,
                    pair.shared_tracks,
                    pair.total_union_tracks,
                );
            }

            if !report.trendsetter_leaderboard.is_empty() {
                println!("\nTrendsetter Leaderboard (first to play shared tracks):");
                let max_firsts = report.trendsetter_leaderboard[0].1.max(1);
                for (i, (name, count)) in report.trendsetter_leaderboard.iter().enumerate() {
                    let bar = render_bar(*count, max_firsts, 18);
                    println!(
                        "  {:2}. {:<20} {:<18} ({} first plays)",
                        i + 1,
                        truncate_ellipsis(name, 20),
                        bar,
                        count
                    );
                }
            }

            if !report.shared_artists.is_empty() {
                println!("\nTop Shared Artists:");
                for (i, (artist, friends, plays)) in report.shared_artists.iter().take(10).enumerate()
                {
                    println!(
                        "  {:2}. {} — {} plays [{}]",
                        i + 1,
                        truncate_ellipsis(artist, 28),
                        plays,
                        friends.join(", ")
                    );
                }
            }

            if !report.shared_tracks.is_empty() {
                println!("\nTop Shared Tracks:");
                for (i, t) in report.shared_tracks.iter().take(10).enumerate() {
                    println!(
                        "  {:2}. {} — {} ({} plays · [{}])",
                        i + 1,
                        truncate_ellipsis(&t.artist_name, 24),
                        truncate_ellipsis(&t.track_name, 30),
                        t.total_plays,
                        t.friends_who_played.join(", "),
                    );
                    println!(
                        "      First played by {} on {} ({})",
                        t.first_player_name,
                        format_timestamp_local(t.first_played_ms),
                        spotify_uri_to_url(&t.track_uri),
                    );
                }
            }
        }

        Commands::History {
            friend,
            days,
            limit,
        } => {
            let db = Database::open(&db_path)?;
            let records = db.query_history(friend.as_deref(), days, Some(limit))?;
            if records.is_empty() {
                println!("No history records found.");
                return Ok(());
            }

            for r in records {
                let artists = r
                    .all_artists
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&r.artist_name);
                let dur = r
                    .duration_ms
                    .map(|ms| format!(" ({})", format_duration_ms(ms)))
                    .unwrap_or_default();

                let mut badges = Vec::new();
                if r.item_type == "episode" {
                    badges.push("[PODCAST]");
                }
                if r.explicit == Some(true) {
                    badges.push("[E]");
                }
                if r.was_skipped == Some(true) {
                    badges.push("[SKIP]");
                }
                if r.inferred_repeat {
                    badges.push("[REPEAT]");
                }
                let badge_str = if badges.is_empty() {
                    String::new()
                } else {
                    format!(" {}", badges.join(" "))
                };

                let ctx = r
                    .context_name
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|c| {
                        if r.context_is_private {
                            format!(" [{c} (PRIVATE)]")
                        } else {
                            format!(" [{c}]")
                        }
                    })
                    .unwrap_or_default();

                println!(
                    "[{}] {} | {} — {}{}{}{}",
                    r.played_at_local, r.user_name, artists, r.track_name, badge_str, dur, ctx
                );
            }
        }

        Commands::Export {
            format,
            out,
            friend,
            days,
        } => {
            let db = Database::open(&db_path)?;
            let records = db.query_history(friend.as_deref(), days, None)?;

            let mut writer: Box<dyn Write> = match &out {
                Some(path) => Box::new(
                    File::create(path)
                        .with_context(|| format!("Failed to create {}", path.display()))?,
                ),
                None => Box::new(io::stdout().lock()),
            };

            match format {
                ExportFormat::Json => {
                    serde_json::to_writer_pretty(&mut writer, &records)?;
                    writeln!(writer)?;
                }
                ExportFormat::Csv => {
                    writeln!(
                        writer,
                        "played_at_local,played_at_ms,user_name,user_uri,item_type,artist_name,all_artists,track_name,album_name,duration_ms,listened_ms,was_skipped,inferred_repeat,popularity,explicit,release_date,context_name,context_type,context_owner,context_is_private,track_uri,spotify_url"
                    )?;
                    for r in &records {
                        let spotify_url = spotify_uri_to_url(&r.track_uri);
                        writeln!(
                            writer,
                            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                            escape_csv(&r.played_at_local),
                            r.played_at_ms,
                            escape_csv(&r.user_name),
                            escape_csv(&r.user_uri),
                            escape_csv(&r.item_type),
                            escape_csv(&r.artist_name),
                            escape_csv(r.all_artists.as_deref().unwrap_or("")),
                            escape_csv(&r.track_name),
                            escape_csv(r.album_name.as_deref().unwrap_or("")),
                            r.duration_ms.map(|v| v.to_string()).unwrap_or_default(),
                            r.listened_ms.map(|v| v.to_string()).unwrap_or_default(),
                            r.was_skipped.map(|v| v.to_string()).unwrap_or_default(),
                            r.inferred_repeat,
                            r.popularity.map(|v| v.to_string()).unwrap_or_default(),
                            r.explicit.map(|v| v.to_string()).unwrap_or_default(),
                            escape_csv(r.release_date.as_deref().unwrap_or("")),
                            escape_csv(r.context_name.as_deref().unwrap_or("")),
                            escape_csv(r.context_type.as_deref().unwrap_or("")),
                            escape_csv(r.context_owner.as_deref().unwrap_or("")),
                            r.context_is_private,
                            escape_csv(&r.track_uri),
                            escape_csv(&spotify_url),
                        )?;
                    }
                }
            }

            if let Some(path) = out {
                eprintln!("Exported {} records to {}", records.len(), path.display());
            }
        }
    }

    Ok(())
}
