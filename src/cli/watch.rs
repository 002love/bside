use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use chrono::Utc;
use tokio::sync::{Notify, watch};

use crate::db::{Database, ProfileDiff, format_timestamp_local};
use crate::format::format_duration_ms;
use crate::spotify::{
    ActivityEvent, FriendActivity, NameCache, OwnPlaybackObserver, StreamConfig,
    TransportPreference, connect_cached, enrich_activity, fetch_user_profile,
    spawn_activity_stream,
};

use super::format_context_badge;

const DEFAULT_RESYNC_INTERVAL_SECS: u64 = 60;
const REPEAT_CHECK_INTERVAL_SECS: u64 = 15;
const STREAM_SHUTDOWN_TIMEOUT_SECS: u64 = 3;

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

struct WatchTracker {
    friend_filter: Vec<String>,
    max_inferred_repeats: u32,
    grace_ms: i64,
    active_plays: HashMap<String, ActivePlay>,
    known_friend_uris: BTreeSet<String>,
}

impl WatchTracker {
    fn new(friend_filter: Vec<String>, max_inferred_repeats: u32, grace_ms: i64) -> Self {
        Self {
            friend_filter,
            max_inferred_repeats,
            grace_ms,
            active_plays: HashMap::new(),
            known_friend_uris: BTreeSet::new(),
        }
    }

    fn process_activity(
        &mut self,
        db: &Database,
        item: FriendActivity,
        now_ms: i64,
    ) -> Option<String> {
        if !item.matches_filter(&self.friend_filter) {
            return None;
        }

        let user_uri = item.user.uri.clone();
        self.known_friend_uris.insert(user_uri.clone());

        let same_track = self
            .active_plays
            .get(&user_uri)
            .is_some_and(|prev| prev.track_uri == item.track.uri);

        let handle_as_same_play = if same_track {
            let prev = self.active_plays.get(&user_uri).unwrap();
            should_handle_as_same_play(
                prev.is_playing,
                prev.cycle_started_at_ms,
                prev.last_seen_ts_ms,
                prev.duration_ms,
                item.playing,
                item.timestamp,
                self.grace_ms,
            )
        } else {
            false
        };

        if !handle_as_same_play {
            let mut session_id = None;
            let prev_opt = self.active_plays.remove(&user_uri);
            let had_active_play = prev_opt.is_some();
            if let Some(prev) = prev_opt {
                session_id = prev.session_id;
                let ended_at_ms = item.timestamp.clamp(prev.cycle_started_at_ms, now_ms);
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

            self.active_plays.insert(
                user_uri.clone(),
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
        } else if let Some(prev) = self.active_plays.get_mut(&user_uri) {
            prev.missed_polls = 0;
            prev.last_seen_ts_ms = item.timestamp;
            prev.last_activity = item.clone();

            if prev.is_playing && !item.playing {
                let ended_at_ms = item.timestamp.clamp(prev.cycle_started_at_ms, now_ms);
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
                let _ = db.mark_playing_seen(&user_uri, &prev.track_uri, prev.cycle_started_at_ms);
                if prev.session_id.is_none()
                    && let Ok(sid) = db.start_session(&user_uri, &item.user.name, now_ms)
                {
                    prev.session_id = Some(sid);
                }
                prev.is_playing = true;
            } else if prev.is_playing && item.playing {
                if let Some(sid) = prev.session_id {
                    let _ = db.touch_session(sid, now_ms, false);
                }
                check_single_inferred_repeat(
                    db,
                    &user_uri,
                    prev,
                    now_ms,
                    self.max_inferred_repeats,
                    self.grace_ms,
                );
            }
        }

        Some(user_uri)
    }

    fn handle_full_snapshot(&mut self, db: &Database, friends: Vec<FriendActivity>, now_ms: i64) {
        let mut seen_users: HashSet<String> = HashSet::new();
        for item in friends {
            if let Some(user_uri) = self.process_activity(db, item, now_ms) {
                seen_users.insert(user_uri);
            }
        }

        // Finalize friends who disappeared from the live feed while playing.
        for (user_uri, state) in self.active_plays.iter_mut() {
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
    }

    fn handle_friend_updated(&mut self, db: &Database, activity: FriendActivity, now_ms: i64) {
        let _ = self.process_activity(db, activity, now_ms);
    }

    fn check_inferred_repeats(&mut self, db: &Database, now_ms: i64) {
        for (user_uri, prev) in self.active_plays.iter_mut() {
            if prev.is_playing {
                check_single_inferred_repeat(
                    db,
                    user_uri,
                    prev,
                    now_ms,
                    self.max_inferred_repeats,
                    self.grace_ms,
                );
            }
        }
    }

    fn close_all_sessions(&mut self, db: &Database, now_ms: i64) {
        for state in self.active_plays.values_mut() {
            if let Some(sid) = state.session_id.take() {
                let _ = db.end_session(sid, now_ms);
            }
        }
    }
}

fn check_single_inferred_repeat(
    db: &Database,
    user_uri: &str,
    prev: &mut ActivePlay,
    now_ms: i64,
    max_inferred_repeats: u32,
    grace_ms: i64,
) {
    if let Some(dur) = prev.duration_ms
        && dur >= 30_000
        && prev.inferred_cycles < max_inferred_repeats
        && (now_ms - prev.cycle_started_at_ms) >= dur + grace_ms
    {
        let cycle_end = prev.cycle_started_at_ms + dur;
        let _ = db.finalize_play(
            user_uri,
            &prev.track_uri,
            prev.cycle_started_at_ms,
            cycle_end,
            Some(dur),
            false,
        );
        prev.cycle_started_at_ms = cycle_end;
        prev.last_seen_ts_ms = cycle_end;
        prev.inferred_cycles += 1;

        match db.insert_inferred_repeat(&prev.last_activity, cycle_end) {
            Ok(true) => {
                if let Some(sid) = prev.session_id {
                    let _ = db.touch_session(sid, now_ms, true);
                }
                println!(
                    "[{}] [REPEAT #{}] {} | {} — {} ({})",
                    format_timestamp_local(cycle_end),
                    prev.inferred_cycles,
                    prev.last_activity.user.name,
                    prev.last_activity.all_artists_display(),
                    prev.last_activity.track.name,
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

fn format_transport_label(transport: TransportPreference, interval_secs: u64) -> String {
    match transport {
        TransportPreference::Auto => {
            format!("auto (realtime push -> {interval_secs}s poll fallback)")
        }
        TransportPreference::Dealer => {
            format!("dealer (Dealer WS push -> {interval_secs}s poll fallback)")
        }
        TransportPreference::Mercury => {
            format!("mercury (Mercury push -> {interval_secs}s poll fallback)")
        }
        TransportPreference::Poll => format!("poll ({interval_secs}s HTTP polling)"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn sync_profiles_once(
    cache_dir: &Path,
    db: &Database,
    known_friend_uris: &BTreeSet<String>,
    own_session: Option<&librespot_core::session::Session>,
    own_uri: Option<&str>,
    own_name: &mut String,
    own_image_url: &mut Option<String>,
    own_observer: Option<&OwnPlaybackObserver>,
) {
    let owned_session = if own_session.is_some_and(|s| !s.is_invalid()) {
        None
    } else {
        match connect_cached(cache_dir).await {
            Ok(session) => Some(session),
            Err(e) => {
                eprintln!("[warn] Profile sync session connect failed: {e}");
                return;
            }
        }
    };
    let Some(session) = owned_session.as_ref().or(own_session) else {
        return;
    };

    for uri in known_friend_uris {
        match fetch_user_profile(session, uri).await {
            Ok(snap) => {
                if own_uri.is_some_and(|own| own == uri) {
                    if !snap.user_name.is_empty() {
                        *own_name = snap.user_name.clone();
                    }
                    *own_image_url = snap.image_url.clone();
                    if let Some(obs) = own_observer {
                        obs.update_profile(own_name.clone(), own_image_url.clone());
                    }
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

    if let Some(session) = owned_session {
        session.shutdown();
    }
}

async fn wait_own_update(rx: &mut Option<watch::Receiver<u64>>) -> bool {
    match rx {
        Some(r) => r.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_watch(
    cache_dir: &Path,
    db_path: &Path,
    friend: Vec<String>,
    interval: u64,
    profile_interval: u64,
    max_inferred_repeats: u32,
    transport: TransportPreference,
    debug_push: bool,
) -> Result<()> {
    let interval_secs = interval.max(10);
    let grace_ms = ((interval_secs as i64) * 1000).max(25_000);
    let db = Database::open(db_path)?;

    let transport_label = format_transport_label(transport, interval_secs);
    let profile_label = if profile_interval == 0 {
        "off".to_string()
    } else {
        format!("{profile_interval}s")
    };

    if friend.is_empty() {
        println!(
            "Watching ALL friends via {transport_label} (profile sync: {profile_label}) -> {}",
            db_path.display()
        );
    } else {
        println!(
            "Watching [{}] via {transport_label} (profile sync: {profile_label}) -> {}",
            friend.join(", "),
            db_path.display()
        );
    }
    println!("Press Ctrl+C to stop.\n");

    let refresh_notify = Arc::new(Notify::new());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let stream_config = StreamConfig {
        poll_interval_secs: interval_secs,
        resync_interval_secs: DEFAULT_RESYNC_INTERVAL_SECS,
        transport,
        debug_push,
    };

    let (mut event_rx, stream_handle) = spawn_activity_stream(
        cache_dir.to_path_buf(),
        stream_config,
        refresh_notify,
        shutdown_rx,
    );

    let mut tracker = WatchTracker::new(friend, max_inferred_repeats, grace_ms);
    let mut last_profile_sync: Option<Instant> = None;
    let mut fatal_error: Option<anyhow::Error> = None;

    let mut repeat_tick = tokio::time::interval(Duration::from_secs(REPEAT_CHECK_INTERVAL_SECS));
    repeat_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Consume the immediate first tick so checks start after REPEAT_CHECK_INTERVAL_SECS.
    repeat_tick.tick().await;

    let mut own_session = match connect_cached(cache_dir).await {
        Ok(session) => Some(session),
        Err(e) => {
            if !e.to_string().contains("No cached Spotify credentials") {
                eprintln!("[warn] Own playback observer session connect failed: {e}");
            }
            None
        }
    };
    let mut own_name_cache = NameCache::default();
    let mut own_uri: Option<String> = None;
    let mut own_name = String::new();
    let mut own_image_url: Option<String> = None;
    let mut own_observer: Option<OwnPlaybackObserver> = None;
    let mut own_updates_rx: Option<watch::Receiver<u64>> = None;

    if let Some(session) = own_session.as_ref() {
        let uri = format!("spotify:user:{}", session.username());
        own_name = session.username();
        if let Ok(profiles) = db.query_profiles(Some(&uri))
            && let Some(p) = profiles.into_iter().find(|p| p.user_uri == uri)
        {
            if !p.user_name.is_empty() {
                own_name = p.user_name;
            }
            own_image_url = p.image_url;
        }
        tracker.known_friend_uris.insert(uri.clone());
        let observer =
            OwnPlaybackObserver::spawn(session.clone(), own_name.clone(), own_image_url.clone());
        own_updates_rx = Some(observer.subscribe_updates());
        own_observer = Some(observer);
        own_uri = Some(uri);
    }

    loop {
        if own_session.as_ref().is_some_and(|s| s.is_invalid()) {
            if let Some(obs) = own_observer.take() {
                obs.shutdown().await;
            }
            own_updates_rx = None;
            if let Some(s) = own_session.take() {
                s.shutdown();
            }
            match connect_cached(cache_dir).await {
                Ok(new_session) => {
                    let observer = OwnPlaybackObserver::spawn(
                        new_session.clone(),
                        own_name.clone(),
                        own_image_url.clone(),
                    );
                    own_updates_rx = Some(observer.subscribe_updates());
                    own_observer = Some(observer);
                    own_session = Some(new_session);
                }
                Err(e) => {
                    eprintln!("[warn] Own playback observer reconnect failed: {e}");
                }
            }
        }

        tokio::select! {
            event = event_rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                match event {
                    ActivityEvent::FullSnapshot { mut friends, .. } => {
                        let now_ms = Utc::now().timestamp_millis();
                        if let (Some(obs), Some(session), Some(uri)) =
                            (own_observer.as_ref(), own_session.as_ref(), own_uri.as_deref())
                            && let Some(mut own_item) = obs.latest()
                        {
                            enrich_activity(session, &mut own_item, &mut own_name_cache).await;
                            if let Some(prev) = tracker.active_plays.get(uri)
                                && prev.track_uri == own_item.track.uri
                                && (own_item.timestamp - prev.cycle_started_at_ms).abs() <= 20_000
                            {
                                own_item.timestamp = prev.cycle_started_at_ms;
                            }
                            friends.retain(|a| !a.user.uri.eq_ignore_ascii_case(uri));
                            friends.push(own_item);
                        }
                        tracker.handle_full_snapshot(&db, friends, now_ms);

                        if profile_interval > 0
                            && !tracker.known_friend_uris.is_empty()
                            && last_profile_sync
                                .is_none_or(|t| t.elapsed() >= Duration::from_secs(profile_interval))
                        {
                            sync_profiles_once(
                                cache_dir,
                                &db,
                                &tracker.known_friend_uris,
                                own_session.as_ref(),
                                own_uri.as_deref(),
                                &mut own_name,
                                &mut own_image_url,
                                own_observer.as_ref(),
                            )
                            .await;
                            last_profile_sync = Some(Instant::now());
                        }
                    }
                    ActivityEvent::FriendUpdated { activity, .. } => {
                        let now_ms = Utc::now().timestamp_millis();
                        tracker.handle_friend_updated(&db, activity, now_ms);
                    }
                    ActivityEvent::TransportChanged { mode, reason } => {
                        match reason {
                            Some(reason) => {
                                println!("[TRANSPORT] Active transport: {mode} ({reason})");
                            }
                            None => {
                                println!("[TRANSPORT] Active transport: {mode}");
                            }
                        }
                    }
                    ActivityEvent::SessionReconnected { account } => {
                        match account {
                            Some(account) => {
                                eprintln!("[info] Connected to Spotify ({account})");
                            }
                            None => {
                                eprintln!("[info] Connected to Spotify");
                            }
                        }
                    }
                    ActivityEvent::Error { message, consecutive_errors } => {
                        eprintln!("[error] Stream error (#{consecutive_errors}): {message}");
                        if message.contains("No cached Spotify credentials") {
                            fatal_error = Some(anyhow!("{message}"));
                            break;
                        }
                    }
                }
            }
            updated = wait_own_update(&mut own_updates_rx) => {
                if !updated {
                    own_updates_rx = None;
                } else if let (Some(obs), Some(session), Some(uri)) =
                    (own_observer.as_ref(), own_session.as_ref(), own_uri.as_deref())
                    && let Some(mut own_item) = obs.latest()
                {
                    let now_ms = Utc::now().timestamp_millis();
                    enrich_activity(session, &mut own_item, &mut own_name_cache).await;
                    if let Some(prev) = tracker.active_plays.get(uri)
                        && prev.track_uri == own_item.track.uri
                        && (own_item.timestamp - prev.cycle_started_at_ms).abs() <= 20_000
                    {
                        own_item.timestamp = prev.cycle_started_at_ms;
                    }
                    tracker.handle_friend_updated(&db, own_item, now_ms);
                }
            }
            _ = repeat_tick.tick() => {
                let now_ms = Utc::now().timestamp_millis();
                tracker.check_inferred_repeats(&db, now_ms);

                if profile_interval > 0
                    && !tracker.known_friend_uris.is_empty()
                    && last_profile_sync
                        .is_none_or(|t| t.elapsed() >= Duration::from_secs(profile_interval))
                {
                    sync_profiles_once(
                        cache_dir,
                        &db,
                        &tracker.known_friend_uris,
                        own_session.as_ref(),
                        own_uri.as_deref(),
                        &mut own_name,
                        &mut own_image_url,
                        own_observer.as_ref(),
                    )
                    .await;
                    last_profile_sync = Some(Instant::now());
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nStopping watcher...");
                break;
            }
        }
    }

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(
        Duration::from_secs(STREAM_SHUTDOWN_TIMEOUT_SECS),
        stream_handle,
    )
    .await;

    // Close all open listening sessions cleanly in SQLite before exiting.
    let end_now_ms = Utc::now().timestamp_millis();
    tracker.close_all_sessions(&db, end_now_ms);

    if let Some(obs) = own_observer {
        obs.shutdown().await;
    }
    if let Some(session) = own_session {
        session.shutdown();
    }

    if let Some(err) = fatal_error {
        return Err(err);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::spotify::{FriendTrack, FriendUser, NamedUri};

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

    fn make_temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bside-watch-test-{label}-{}-{nanos}.db",
            std::process::id()
        ))
    }

    fn make_activity(
        user_id: &str,
        user_name: &str,
        track_id: &str,
        track_name: &str,
        timestamp: i64,
        playing: bool,
        duration_ms: Option<i64>,
    ) -> FriendActivity {
        FriendActivity {
            timestamp,
            playing,
            user: FriendUser {
                uri: format!("spotify:user:{user_id}"),
                name: user_name.to_string(),
                image_url: None,
            },
            track: FriendTrack {
                uri: format!("spotify:track:{track_id}"),
                name: track_name.to_string(),
                artist: Some(NamedUri {
                    uri: "spotify:artist:a1".to_string(),
                    name: "Test Artist".to_string(),
                }),
                artists: vec![NamedUri {
                    uri: "spotify:artist:a1".to_string(),
                    name: "Test Artist".to_string(),
                }],
                duration_ms,
                item_type: "track".to_string(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn rapid_friend_updated_records_all_listens_and_skips_without_marking_others_offline() {
        let db_path = make_temp_db_path("rapid-skips");
        let db = Database::open(&db_path).expect("open db");
        let mut tracker = WatchTracker::new(Vec::new(), 10, 25_000);

        let base_ms: i64 = 1_750_000_000_000;

        // Seed another active friend (Bob) via an initial full snapshot.
        let bob_activity = make_activity(
            "bob",
            "Bob",
            "bob_track",
            "Long Ambient Track",
            base_ms,
            true,
            Some(240_000),
        );
        tracker.handle_full_snapshot(&db, vec![bob_activity], base_ms);

        // Rapidly push 5 tracks for Alice within 2 seconds (every 500ms).
        for idx in 0..5i64 {
            let ts = base_ms + idx * 500;
            let alice_activity = make_activity(
                "alice",
                "Alice",
                &format!("track_{idx}"),
                &format!("Track {idx}"),
                ts,
                true,
                Some(180_000),
            );
            tracker.handle_friend_updated(&db, alice_activity, ts);
        }

        // All 5 tracks for Alice must be recorded in SQLite.
        let mut alice_history = db
            .query_history(Some("alice"), None, None)
            .expect("query alice history");
        alice_history.sort_by_key(|r| r.played_at_ms);
        assert_eq!(alice_history.len(), 5);

        // First 4 tracks were rapidly switched after 500ms (< skip threshold) -> marked skipped.
        for (i, rec) in alice_history.iter().take(4).enumerate() {
            assert_eq!(rec.track_uri, format!("spotify:track:track_{i}"));
            assert_eq!(
                rec.was_skipped,
                Some(true),
                "expected track_{i} to be marked skipped"
            );
            assert!(
                rec.ended_at_ms.is_some(),
                "expected track_{i} to have ended_at_ms set"
            );
        }

        // 5th track is still currently playing -> not finalized or skipped yet.
        let last = &alice_history[4];
        assert_eq!(last.track_uri, "spotify:track:track_4");
        assert_eq!(last.was_skipped, None);
        assert_eq!(last.ended_at_ms, None);

        // Bob must remain active and untouched (missed_polls == 0, still playing).
        let bob_state = tracker
            .active_plays
            .get("spotify:user:bob")
            .expect("bob active play");
        assert!(bob_state.is_playing);
        assert_eq!(bob_state.missed_polls, 0);
        assert!(bob_state.session_id.is_some());

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn full_snapshot_marks_disappeared_friend_offline_after_missed_polls() {
        let db_path = make_temp_db_path("offline-missed-polls");
        let db = Database::open(&db_path).expect("open db");
        let mut tracker = WatchTracker::new(Vec::new(), 10, 25_000);

        let base_ms: i64 = 1_750_000_100_000;
        let alice = make_activity(
            "alice",
            "Alice",
            "alice_1",
            "Alice Song",
            base_ms,
            true,
            Some(200_000),
        );
        let bob = make_activity(
            "bob",
            "Bob",
            "bob_1",
            "Bob Song",
            base_ms,
            true,
            Some(200_000),
        );

        tracker.handle_full_snapshot(&db, vec![alice.clone(), bob], base_ms);

        // 1st missed full snapshot -> missed_polls == 1, still playing
        tracker.handle_full_snapshot(&db, vec![alice.clone()], base_ms + 15_000);
        {
            let bob_state = tracker.active_plays.get("spotify:user:bob").unwrap();
            assert!(bob_state.is_playing);
            assert_eq!(bob_state.missed_polls, 1);
            assert!(bob_state.session_id.is_some());
        }

        // 2nd missed full snapshot -> missed_polls == 2, still playing
        tracker.handle_full_snapshot(&db, vec![alice.clone()], base_ms + 30_000);
        {
            let bob_state = tracker.active_plays.get("spotify:user:bob").unwrap();
            assert!(bob_state.is_playing);
            assert_eq!(bob_state.missed_polls, 2);
            assert!(bob_state.session_id.is_some());
        }

        // 3rd missed full snapshot -> missed_polls == 3 (> 2), marked offline & finalized
        let offline_ms = base_ms + 45_000;
        tracker.handle_full_snapshot(&db, vec![alice], offline_ms);
        {
            let bob_state = tracker.active_plays.get("spotify:user:bob").unwrap();
            assert!(!bob_state.is_playing);
            assert_eq!(bob_state.missed_polls, 3);
            assert!(bob_state.session_id.is_none());
        }

        let bob_history = db
            .query_history(Some("bob"), None, None)
            .expect("query bob history");
        assert_eq!(bob_history.len(), 1);
        assert_eq!(bob_history[0].ended_at_ms, Some(offline_ms));
        assert_eq!(bob_history[0].listened_ms, Some(45_000));

        // Alice remained present and active throughout.
        let alice_state = tracker.active_plays.get("spotify:user:alice").unwrap();
        assert!(alice_state.is_playing);
        assert_eq!(alice_state.missed_polls, 0);

        let _ = std::fs::remove_file(&db_path);
    }
}
