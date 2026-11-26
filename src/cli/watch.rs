use std::collections::BTreeSet;
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
use crate::tracker::{ActivityOutcome, OfflineNotice, PlayTracker, RepeatNotice};

use super::format_context_badge;

const DEFAULT_RESYNC_INTERVAL_SECS: u64 = 60;
const MAINTENANCE_CHECK_INTERVAL_SECS: u64 = 15;
const STREAM_SHUTDOWN_TIMEOUT_SECS: u64 = 3;

fn print_activity_outcome(outcome: &ActivityOutcome) {
    for skip in &outcome.skipped {
        let dur_str = skip
            .duration_ms
            .map(format_duration_ms)
            .unwrap_or_else(|| "?".to_string());
        println!(
            "[{}] [SKIP] {} skipped {} — {} after {} / {}",
            format_timestamp_local(skip.ended_at_ms),
            skip.activity.user.name,
            skip.activity.all_artists_display(),
            skip.activity.track.name,
            format_duration_ms(skip.elapsed_ms),
            dur_str,
        );
    }

    for item in &outcome.inserted_live {
        print_live_listen_line(item);
    }

    if let Some(pause) = &outcome.paused {
        println!(
            "[{}] [PAUSE/END] {} stopped {} — {} ({})",
            format_timestamp_local(pause.ended_at_ms),
            pause.activity.user.name,
            pause.activity.all_artists_display(),
            pause.activity.track.name,
            format_duration_ms(pause.elapsed_ms),
        );
    }

    for rep in &outcome.repeats {
        print_repeat_notice(rep);
    }
}

fn print_repeat_notice(rep: &RepeatNotice) {
    println!(
        "[{}] [REPEAT #{}] {} | {} — {} ({})",
        format_timestamp_local(rep.cycle_end_ms),
        rep.cycle_number,
        rep.activity.user.name,
        rep.activity.all_artists_display(),
        rep.activity.track.name,
        format_duration_ms(rep.duration_ms),
    );
}

fn print_offline_notices(notices: &[OfflineNotice]) {
    for notice in notices {
        println!(
            "[{}] [OFFLINE] {} left feed during {} — {}",
            format_timestamp_local(notice.ended_at_ms),
            notice.activity.user.name,
            notice.activity.all_artists_display(),
            notice.activity.track.name,
        );
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

    let mut tracker = PlayTracker::new(friend, max_inferred_repeats, grace_ms);
    let mut last_profile_sync: Option<Instant> = None;
    let mut fatal_error: Option<anyhow::Error> = None;

    let mut maintenance_tick =
        tokio::time::interval(Duration::from_secs(MAINTENANCE_CHECK_INTERVAL_SECS));
    maintenance_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Consume the immediate first tick so checks start after MAINTENANCE_CHECK_INTERVAL_SECS.
    maintenance_tick.tick().await;

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
        tracker.insert_known_friend_uri(uri.clone());
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
                            if own_item.playing
                                && let Some(prev) = tracker.active_plays().get(uri)
                                && prev.is_playing
                                && prev.track_uri == own_item.track.uri
                                && (own_item.timestamp - prev.cycle_started_at_ms).abs() <= 20_000
                            {
                                own_item.timestamp = prev.cycle_started_at_ms;
                            }
                            friends.retain(|a| !a.user.uri.eq_ignore_ascii_case(uri));
                            friends.push(own_item);
                        }
                        let (outcomes, offline_notices) =
                            tracker.handle_full_snapshot(&db, friends, now_ms);
                        for outcome in &outcomes {
                            print_activity_outcome(outcome);
                        }
                        print_offline_notices(&offline_notices);

                        if profile_interval > 0
                            && !tracker.known_friend_uris().is_empty()
                            && last_profile_sync
                                .is_none_or(|t| t.elapsed() >= Duration::from_secs(profile_interval))
                        {
                            sync_profiles_once(
                                cache_dir,
                                &db,
                                tracker.known_friend_uris(),
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
                        let outcome = tracker.handle_friend_updated(&db, activity, now_ms);
                        print_activity_outcome(&outcome);
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
                    if own_item.playing
                        && let Some(prev) = tracker.active_plays().get(uri)
                        && prev.is_playing
                        && prev.track_uri == own_item.track.uri
                        && (own_item.timestamp - prev.cycle_started_at_ms).abs() <= 20_000
                    {
                        own_item.timestamp = prev.cycle_started_at_ms;
                    }
                    let outcome = tracker.handle_friend_updated(&db, own_item, now_ms);
                    print_activity_outcome(&outcome);
                }
            }
            _ = maintenance_tick.tick() => {
                let now_ms = Utc::now().timestamp_millis();
                for rep in &tracker.check_maintenance(&db, now_ms) {
                    print_repeat_notice(rep);
                }

                if profile_interval > 0
                    && !tracker.known_friend_uris().is_empty()
                    && last_profile_sync
                        .is_none_or(|t| t.elapsed() >= Duration::from_secs(profile_interval))
                {
                    sync_profiles_once(
                        cache_dir,
                        &db,
                        tracker.known_friend_uris(),
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
    use crate::tracker::is_likely_skipped;

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
        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);

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
            .active_plays()
            .get("spotify:user:bob")
            .expect("bob active play");
        assert!(bob_state.is_playing);
        assert_eq!(bob_state.missed_polls, 0);
        assert!(bob_state.session_id.is_some());
        assert!(!is_likely_skipped(124_640, Some(124_000)));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn full_snapshot_marks_disappeared_friend_offline_after_missed_polls() {
        let db_path = make_temp_db_path("offline-missed-polls");
        let db = Database::open(&db_path).expect("open db");
        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);

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
            let bob_state = tracker.active_plays().get("spotify:user:bob").unwrap();
            assert!(bob_state.is_playing);
            assert_eq!(bob_state.missed_polls, 1);
            assert!(bob_state.session_id.is_some());
        }

        // 2nd missed full snapshot -> missed_polls == 2, still playing
        tracker.handle_full_snapshot(&db, vec![alice.clone()], base_ms + 30_000);
        {
            let bob_state = tracker.active_plays().get("spotify:user:bob").unwrap();
            assert!(bob_state.is_playing);
            assert_eq!(bob_state.missed_polls, 2);
            assert!(bob_state.session_id.is_some());
        }

        // 3rd missed full snapshot -> missed_polls == 3 (> 2), marked offline & finalized
        let offline_ms = base_ms + 45_000;
        tracker.handle_full_snapshot(&db, vec![alice], offline_ms);
        {
            let bob_state = tracker.active_plays().get("spotify:user:bob").unwrap();
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
        let alice_state = tracker.active_plays().get("spotify:user:alice").unwrap();
        assert!(alice_state.is_playing);
        assert_eq!(alice_state.missed_polls, 0);

        let _ = std::fs::remove_file(&db_path);
    }
}
