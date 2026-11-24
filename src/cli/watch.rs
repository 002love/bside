use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use tokio::time::sleep;

use crate::db::{Database, ProfileDiff, format_timestamp_local};
use crate::format::format_duration_ms;
use crate::spotify::{
    FriendActivity, NameCache, connect_cached, fetch_buddylist, fetch_user_profile,
};

use super::format_context_badge;

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

fn is_likely_skipped(elapsed_ms: i64, duration_ms: Option<i64>) -> bool {
    match duration_ms {
        Some(d) if d >= 30_000 => {
            let threshold = (d / 2).min(d - 15_000);
            elapsed_ms < threshold
        }
        _ => false,
    }
}

pub(super) fn print_profile_diff(user_name: &str, diff: &ProfileDiff) {
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

pub(super) async fn run_watch(
    cache_dir: &Path,
    db_path: &Path,
    friend: Vec<String>,
    interval: u64,
    profile_interval: u64,
    max_inferred_repeats: u32,
) -> Result<()> {
    let interval_secs = interval.max(10);
    let db = Database::open(db_path)?;
    let mut session = connect_cached(cache_dir).await?;

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
            match connect_cached(cache_dir).await {
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
                        prev.track_uri == item.track.uri && prev.played_at_ms == item.timestamp
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
                                match db.start_session(&user_uri, &item.user.name, start_ms) {
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
                                        eprintln!("[error] insert_inferred_repeat failed: {e}");
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
                    && last_profile_sync
                        .is_none_or(|t| t.elapsed() >= Duration::from_secs(profile_interval))
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

    Ok(())
}
