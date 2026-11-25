use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use tokio::time::sleep;

use crate::db::{Database, ProfileDiff, format_timestamp_local};
use crate::format::format_duration_ms;
use crate::spotify::{
    FriendActivity, NameCache, OwnPlaybackObserver, connect_cached, enrich_activity,
    fetch_buddylist, fetch_user_profile,
};

use super::format_context_badge;

struct ActivePlay {
    track_uri: String,
    cycle_started_at_ms: i64,
    last_seen_ts_ms: i64,
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

/// Decides whether a same-track activity update belongs to the currently tracked play
/// rather than starting a brand-new repeat play.
///
/// Spotify frequently emits an end-of-track completion/transition timestamp bump for the
/// current track around 85%..105% of `duration_ms` (just 1–5s before switching tracks).
/// Treating that bump as a new play inserts a phantom duplicate row that immediately gets
/// marked `SKIP` when the next track begins. We keep same-track updates anchored to the
/// original `cycle_started_at_ms` until `duration_ms + repeat_grace_ms` has elapsed (or a
/// long pause/offline gap has occurred).
fn should_handle_as_same_play(
    prev_was_playing: bool,
    prev_cycle_started_at_ms: i64,
    prev_last_seen_ts_ms: i64,
    prev_duration_ms: Option<i64>,
    item_playing: bool,
    item_timestamp_ms: i64,
    repeat_grace_ms: i64,
) -> bool {
    if !item_playing {
        // Pause/stop on the same track, even if Spotify bumped activity.timestamp.
        return true;
    }

    let elapsed_from_cycle = item_timestamp_ms.saturating_sub(prev_cycle_started_at_ms);
    let ts_delta = item_timestamp_ms.saturating_sub(prev_last_seen_ts_ms);
    let max_gap = prev_duration_ms
        .filter(|&d| d > 0)
        .map(|d| d + repeat_grace_ms)
        .unwrap_or(0)
        .max(300_000);

    if !prev_was_playing {
        // Resuming after a pause/offline gap: start a new play only after a long gap.
        return ts_delta < max_gap && elapsed_from_cycle < max_gap;
    }

    let within_track_window = match prev_duration_ms {
        Some(d) if d > 0 => elapsed_from_cycle < d + repeat_grace_ms,
        _ => elapsed_from_cycle < 180_000,
    };
    within_track_window && ts_delta < max_gap && elapsed_from_cycle < max_gap
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

    let own_uri = format!("spotify:user:{}", session.username());
    let mut own_name = session.username();
    let mut own_image_url: Option<String> = None;
    if let Ok(profiles) = db.query_profiles(Some(&own_uri))
        && let Some(p) = profiles.into_iter().find(|p| p.user_uri == own_uri)
    {
        if !p.user_name.is_empty() {
            own_name = p.user_name;
        }
        own_image_url = p.image_url;
    }
    known_friend_uris.insert(own_uri.clone());

    let mut own_observer =
        OwnPlaybackObserver::spawn(session.clone(), own_name.clone(), own_image_url.clone());

    let grace_ms = ((interval_secs as i64) * 1000).max(25_000);

    loop {
        if session.is_invalid() {
            eprintln!("[warn] Spotify session disconnected, reconnecting...");
            own_observer.shutdown().await;
            match connect_cached(cache_dir).await {
                Ok(new_session) => {
                    session = new_session;
                    own_observer = OwnPlaybackObserver::spawn(
                        session.clone(),
                        own_name.clone(),
                        own_image_url.clone(),
                    );
                    eprintln!("[info] Reconnected.");
                }
                Err(e) => {
                    eprintln!("[error] Reconnect failed: {e}");
                }
            }
        }

        match fetch_buddylist(&session, &mut name_cache).await {
            Ok(mut list) => {
                consecutive_errors = 0;
                let now_ms = Utc::now().timestamp_millis();

                // Read own connected account's playback from the hidden Connect observer.
                if let Some(mut own_item) = own_observer.latest() {
                    enrich_activity(&session, &mut own_item, &mut name_cache).await;
                    if let Some(prev) = active_plays.get(&own_uri)
                        && prev.track_uri == own_item.track.uri
                        && (own_item.timestamp - prev.cycle_started_at_ms).abs() <= 20_000
                    {
                        own_item.timestamp = prev.cycle_started_at_ms;
                    }
                    list.retain(|a| !a.user.uri.eq_ignore_ascii_case(&own_uri));
                    list.push(own_item);
                }

                let mut seen_users: HashSet<String> = HashSet::new();

                for item in list.into_iter().filter(|f| f.matches_filter(&friend)) {
                    let user_uri = item.user.uri.clone();
                    seen_users.insert(user_uri.clone());
                    known_friend_uris.insert(user_uri.clone());

                    let same_track = active_plays
                        .get(&user_uri)
                        .is_some_and(|prev| prev.track_uri == item.track.uri);

                    let handle_as_same_play = if same_track {
                        let prev = active_plays.get(&user_uri).unwrap();
                        should_handle_as_same_play(
                            prev.is_playing,
                            prev.cycle_started_at_ms,
                            prev.last_seen_ts_ms,
                            prev.duration_ms,
                            item.playing,
                            item.timestamp,
                            grace_ms,
                        )
                    } else {
                        false
                    };

                    if !handle_as_same_play {
                        let mut session_id = None;
                        let prev_opt = active_plays.remove(&user_uri);
                        let had_active_play = prev_opt.is_some();
                        if let Some(prev) = prev_opt {
                            session_id = prev.session_id;
                            let ended_at_ms =
                                item.timestamp.clamp(prev.cycle_started_at_ms, now_ms);
                            if prev.is_playing {
                                let elapsed_ms = (ended_at_ms - prev.cycle_started_at_ms).max(0);
                                let switched_to_next_track = prev.track_uri != item.track.uri;
                                if let Err(e) = db.finalize_play(
                                    &user_uri,
                                    &prev.track_uri,
                                    prev.cycle_started_at_ms,
                                    ended_at_ms,
                                    prev.duration_ms,
                                    switched_to_next_track,
                                ) {
                                    eprintln!("[error] finalize_play failed: {e}");
                                } else if switched_to_next_track
                                    && is_likely_skipped(elapsed_ms, prev.duration_ms)
                                {
                                    let dur_str = prev
                                        .duration_ms
                                        .map(format_duration_ms)
                                        .unwrap_or_else(|| "?".to_string());
                                    println!(
                                        "[{}] [SKIP] {} skipped {} — {} after {} / {}",
                                        format_timestamp_local(ended_at_ms),
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
                                let _ = db.end_session(sid, ended_at_ms);
                            }
                        }

                        // Avoid inserting duplicate [LAST] rows on cold-start / service restart
                        // when Spotify's paused/stopped timestamp differs from the original play start.
                        let skip_cold_start_last = !item.playing
                            && !had_active_play
                            && db
                                .has_recent_listen_near(
                                    &user_uri,
                                    &item.track.uri,
                                    item.timestamp,
                                    item.duration_ms().unwrap_or(0).max(180_000),
                                )
                                .unwrap_or(false);

                        let inserted = if skip_cold_start_last {
                            false
                        } else {
                            match db.insert_listen(&item) {
                                Ok(ins) => ins,
                                Err(e) => {
                                    eprintln!("[error] DB insert failed: {e}");
                                    false
                                }
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
                                cycle_started_at_ms: item.timestamp,
                                last_seen_ts_ms: item.timestamp,
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
                        prev.last_seen_ts_ms = item.timestamp;
                        prev.last_activity = item.clone();

                        if prev.is_playing && !item.playing {
                            let ended_at_ms =
                                item.timestamp.clamp(prev.cycle_started_at_ms, now_ms);
                            let elapsed_ms = (ended_at_ms - prev.cycle_started_at_ms).max(0);
                            if let Err(e) = db.finalize_play(
                                &user_uri,
                                &prev.track_uri,
                                prev.cycle_started_at_ms,
                                ended_at_ms,
                                prev.duration_ms,
                                false,
                            ) {
                                eprintln!("[error] finalize_play failed: {e}");
                            }
                            if let Some(sid) = prev.session_id.take() {
                                let _ = db.end_session(sid, ended_at_ms);
                            }
                            prev.is_playing = false;
                            println!(
                                "[{}] [PAUSE/END] {} stopped {} — {} ({})",
                                format_timestamp_local(ended_at_ms),
                                item.user.name,
                                item.all_artists_display(),
                                item.track.name,
                                format_duration_ms(elapsed_ms),
                            );
                        } else if !prev.is_playing && item.playing {
                            let _ = db.mark_playing_seen(
                                &user_uri,
                                &prev.track_uri,
                                prev.cycle_started_at_ms,
                            );
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
                                && (now_ms - prev.cycle_started_at_ms) >= dur + grace_ms
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
                                prev.last_seen_ts_ms = cycle_end;
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
                            Ok(snap) => {
                                if uri == &own_uri {
                                    if !snap.user_name.is_empty() {
                                        own_name = snap.user_name.clone();
                                    }
                                    own_image_url = snap.image_url.clone();
                                    own_observer
                                        .update_profile(own_name.clone(), own_image_url.clone());
                                }
                                match db.upsert_profile(&snap) {
                                    Ok(diff) => print_profile_diff(&snap.user_name, &diff),
                                    Err(e) => {
                                        eprintln!("[error] upsert_profile failed: {e}");
                                    }
                                }
                            }
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
    own_observer.shutdown().await;
    session.shutdown();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_likely_skipped, should_handle_as_same_play};

    #[test]
    fn end_of_track_heartbeat_at_97_5_percent_stays_same_play() {
        let t0 = 1_795_619_146_000i64; // 16:05:46
        let dur = 124_000i64; // Brain Power (02:04)
        let grace = 25_000i64;

        // End-of-track heartbeat at +121.2s (97.7% of 124s)
        assert!(should_handle_as_same_play(
            true,
            t0,
            t0,
            Some(dur),
            true,
            t0 + 121_204,
            grace,
        ));

        // Even at 105% (inside grace buffer), still same play
        assert!(should_handle_as_same_play(
            true,
            t0,
            t0 + 121_204,
            Some(dur),
            true,
            t0 + 130_000,
            grace,
        ));

        // Once clearly past dur + grace_ms (149s), treat as a new repeat play
        assert!(!should_handle_as_same_play(
            true,
            t0,
            t0 + 121_204,
            Some(dur),
            true,
            t0 + dur + grace,
            grace,
        ));

        // When next track starts at +124.6s, original cycle is NOT skipped
        assert!(!is_likely_skipped(124_640, Some(dur)));
    }

    #[test]
    fn pause_and_long_gap_resume_behave_correctly() {
        let t0 = 1_795_619_146_000i64;
        let dur = 124_000i64;
        let grace = 25_000i64;

        // Pausing on the same track always stays on the same play
        assert!(should_handle_as_same_play(
            true,
            t0,
            t0,
            Some(dur),
            false,
            t0 + 121_000,
            grace,
        ));

        // Resuming after a long pause (> 5 min) starts a new play
        assert!(!should_handle_as_same_play(
            false,
            t0,
            t0 + 60_000,
            Some(dur),
            true,
            t0 + 400_000,
            grace,
        ));

        // Long track (10 min): end-of-track heartbeat at 605s stays same play
        let long_dur = 600_000i64;
        assert!(should_handle_as_same_play(
            true,
            t0,
            t0,
            Some(long_dur),
            true,
            t0 + 605_000,
            grace,
        ));
    }
}
