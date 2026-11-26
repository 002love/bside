use std::collections::{BTreeSet, HashMap, HashSet};

use crate::db::Database;
use crate::spotify::FriendActivity;

/// Maximum gap between a pause and a same-track resume that is still treated as
/// continuing/finishing the same play (12 hours).
pub const PAUSE_RESUME_MAX_GAP_MS: i64 = 12 * 60 * 60 * 1000;

/// Small backward-timestamp tolerance for live track switches (e.g., clock/position jitter
/// across rapid skips in OwnPlaybackObserver).
const OUT_OF_ORDER_LIVE_TOLERANCE_MS: i64 = 1_500;

/// Minimum time without any new Spotify timestamp before a stuck `isPlaying: true`
/// state is finalized once and muted until Spotify emits a newer event.
pub const STALE_PLAYING_MIN_TIMEOUT_MS: i64 = 15 * 60 * 1000;

/// Extra buffer beyond track duration before treating a frozen `isPlaying: true` as stuck.
const STALE_PLAYING_EXTRA_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone)]
struct PendingRepeat {
    activity: FriendActivity,
    started_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct ActivePlay {
    pub track_uri: String,
    pub cycle_started_at_ms: i64,
    pub segment_started_at_ms: i64,
    pub accumulated_played_ms: i64,
    pub last_event_ts_ms: i64,
    pub last_activity_wall_ms: i64,
    pub duration_ms: Option<i64>,
    pub inferred_cycles: u32,
    pub is_playing: bool,
    pub stuck_finalized: bool,
    pub stale_cold_start: bool,
    pub session_id: Option<i64>,
    pub last_activity: FriendActivity,
    pub missed_polls: u32,
    pending_repeat: Option<PendingRepeat>,
}

#[derive(Debug, Clone)]
pub struct SkipNotice {
    pub ended_at_ms: i64,
    pub elapsed_ms: i64,
    pub duration_ms: Option<i64>,
    pub activity: FriendActivity,
}

#[derive(Debug, Clone)]
pub struct PauseNotice {
    pub ended_at_ms: i64,
    pub elapsed_ms: i64,
    pub activity: FriendActivity,
}

#[derive(Debug, Clone)]
pub struct RepeatNotice {
    pub cycle_end_ms: i64,
    pub cycle_number: u32,
    pub duration_ms: i64,
    pub activity: FriendActivity,
}

#[derive(Debug, Clone)]
pub struct OfflineNotice {
    pub ended_at_ms: i64,
    pub activity: FriendActivity,
}

#[derive(Debug, Default)]
pub struct ActivityOutcome {
    pub user_uri: Option<String>,
    pub inserted_live: Vec<FriendActivity>,
    pub skipped: Vec<SkipNotice>,
    pub paused: Option<PauseNotice>,
    pub repeats: Vec<RepeatNotice>,
}

pub struct PlayTracker {
    friend_filter: Vec<String>,
    max_inferred_repeats: u32,
    grace_ms: i64,
    active_plays: HashMap<String, ActivePlay>,
    known_friend_uris: BTreeSet<String>,
}

impl PlayTracker {
    pub fn new(friend_filter: Vec<String>, max_inferred_repeats: u32, grace_ms: i64) -> Self {
        Self {
            friend_filter,
            max_inferred_repeats,
            grace_ms,
            active_plays: HashMap::new(),
            known_friend_uris: BTreeSet::new(),
        }
    }

    pub fn active_plays(&self) -> &HashMap<String, ActivePlay> {
        &self.active_plays
    }

    pub fn known_friend_uris(&self) -> &BTreeSet<String> {
        &self.known_friend_uris
    }

    pub fn insert_known_friend_uri(&mut self, uri: String) {
        self.known_friend_uris.insert(uri);
    }

    pub fn clear(&mut self) {
        self.active_plays.clear();
    }

    pub fn process_activity(
        &mut self,
        db: &Database,
        item: FriendActivity,
        now_ms: i64,
    ) -> ActivityOutcome {
        let mut outcome = ActivityOutcome::default();
        if !item.matches_filter(&self.friend_filter) {
            return outcome;
        }

        let user_uri = item.user.uri.clone();
        self.known_friend_uris.insert(user_uri.clone());
        outcome.user_uri = Some(user_uri.clone());

        // Cold-start path: rehydrate from the user's latest DB row so restarts
        // never create duplicate rows or lose pause-resume state.
        if !self.active_plays.contains_key(&user_uri) {
            if self.try_cold_start_rehydrate(db, &user_uri, &item, now_ms) {
                return outcome;
            }
            self.start_new_play(db, &user_uri, item, now_ms, None, &mut outcome);
            return outcome;
        }

        let same_track = self
            .active_plays
            .get(&user_uri)
            .is_some_and(|prev| prev.track_uri == item.track.uri);

        if !same_track {
            let is_stale_switch = {
                let prev = self.active_plays.get(&user_uri).unwrap();
                if !item.playing {
                    item.timestamp <= prev.last_event_ts_ms
                } else {
                    item.timestamp + OUT_OF_ORDER_LIVE_TOLERANCE_MS < prev.last_event_ts_ms
                }
            };

            if is_stale_switch {
                if let Some(prev) = self.active_plays.get_mut(&user_uri) {
                    prev.missed_polls = 0;
                }
                return outcome;
            }

            let prev = self.active_plays.remove(&user_uri).unwrap();
            let session_id = self.finalize_removed_play(db, &user_uri, prev, &item, now_ms, &mut outcome);
            self.start_new_play(db, &user_uri, item, now_ms, session_id, &mut outcome);
            return outcome;
        }

        // Same track as currently active play.
        // Decide if this requires breaking out into a brand-new play (e.g. >12h resume
        // or a fresh Spotify timestamp clearly past duration + grace).
        let start_fresh_same_track = {
            let prev = self.active_plays.get(&user_uri).unwrap();
            if !item.playing {
                false
            } else if !prev.is_playing {
                let pause_gap = item.timestamp.saturating_sub(prev.last_event_ts_ms).max(0);
                pause_gap > PAUSE_RESUME_MAX_GAP_MS
            } else if item.timestamp > prev.last_event_ts_ms {
                let dur = prev.duration_ms.or_else(|| item.duration_ms());
                let active_elapsed =
                    prev.accumulated_played_ms + (item.timestamp - prev.segment_started_at_ms).max(0);
                let full_plus_grace = dur
                    .filter(|&d| d > 0)
                    .map(|d| d + self.grace_ms)
                    .unwrap_or(180_000);
                active_elapsed >= full_plus_grace
            } else {
                false
            }
        };

        if start_fresh_same_track {
            let prev = self.active_plays.remove(&user_uri).unwrap();
            let session_id = self.finalize_removed_play(db, &user_uri, prev, &item, now_ms, &mut outcome);
            self.start_new_play(db, &user_uri, item, now_ms, session_id, &mut outcome);
            return outcome;
        }

        // Handle as continuation / pause / resume / end-of-track zone of the active play.
        let prev = self.active_plays.get_mut(&user_uri).unwrap();
        prev.missed_polls = 0;
        if prev.duration_ms.is_none() {
            prev.duration_ms = item.duration_ms();
        }

        if !item.playing {
            // Pause / stop on the same track.
            if item.timestamp < prev.cycle_started_at_ms {
                // Stale pause event from before this play started.
                return outcome;
            }

            prev.last_event_ts_ms = prev.last_event_ts_ms.max(item.timestamp);
            prev.last_activity = item.clone();

            if prev.is_playing {
                if !prev.stuck_finalized {
                    let ended_at_ms = item.timestamp.clamp(prev.segment_started_at_ms, now_ms);
                    if let Some(pending) = prev.pending_repeat.take() {
                        let pending_listened = (ended_at_ms - pending.started_at_ms).max(0);
                        let confirm_ms = confirm_repeat_threshold(prev.duration_ms);
                        if pending_listened >= confirm_ms {
                            // Finalize Play #1 up to when the repeat started.
                            let play1_listened = clamp_to_dur(
                                prev.accumulated_played_ms
                                    + (pending.started_at_ms - prev.segment_started_at_ms).max(0),
                                prev.duration_ms,
                            );
                            let _ = db.finalize_play_with_listened(
                                &user_uri,
                                &prev.track_uri,
                                prev.cycle_started_at_ms,
                                pending.started_at_ms,
                                play1_listened,
                                prev.duration_ms,
                                false,
                            );

                            // Insert and finalize Play #2.
                            let mut repeat_act = pending.activity;
                            repeat_act.timestamp = pending.started_at_ms;
                            if db.insert_listen(&repeat_act).unwrap_or(false) {
                                outcome.inserted_live.push(repeat_act.clone());
                            }
                            let play2_listened = clamp_to_dur(pending_listened, prev.duration_ms);
                            let _ = db.finalize_play_with_listened(
                                &user_uri,
                                &prev.track_uri,
                                pending.started_at_ms,
                                ended_at_ms,
                                play2_listened,
                                prev.duration_ms,
                                false,
                            );
                            prev.cycle_started_at_ms = pending.started_at_ms;
                            prev.segment_started_at_ms = pending.started_at_ms;
                            prev.accumulated_played_ms = play2_listened;
                        } else {
                            // Just finished the tail of Play #1.
                            let total_listened = clamp_to_dur(
                                prev.accumulated_played_ms
                                    + (ended_at_ms - prev.segment_started_at_ms).max(0),
                                prev.duration_ms,
                            );
                            prev.accumulated_played_ms = total_listened;
                            let _ = db.finalize_play_with_listened(
                                &user_uri,
                                &prev.track_uri,
                                prev.cycle_started_at_ms,
                                ended_at_ms,
                                total_listened,
                                prev.duration_ms,
                                false,
                            );
                        }
                    } else {
                        let seg_ms = (ended_at_ms - prev.segment_started_at_ms).max(0);
                        prev.accumulated_played_ms =
                            clamp_to_dur(prev.accumulated_played_ms + seg_ms, prev.duration_ms);
                        let _ = db.finalize_play_with_listened(
                            &user_uri,
                            &prev.track_uri,
                            prev.cycle_started_at_ms,
                            ended_at_ms,
                            prev.accumulated_played_ms,
                            prev.duration_ms,
                            false,
                        );
                    }

                    if let Some(sid) = prev.session_id.take() {
                        let _ = db.end_session(sid, ended_at_ms);
                    }
                    outcome.paused = Some(PauseNotice {
                        ended_at_ms,
                        elapsed_ms: prev.accumulated_played_ms,
                        activity: item,
                    });
                }
                prev.is_playing = false;
                prev.stuck_finalized = false;
            }
            return outcome;
        }

        if !prev.is_playing && item.playing {
            // Resuming the same track after a pause or offline gap (within 12h).
            let prev_completed = is_play_completed(prev.accumulated_played_ms, prev.duration_ms);
            let resume_seg_start = if item.timestamp >= prev.last_event_ts_ms {
                item.timestamp
            } else {
                now_ms
            };
            prev.last_event_ts_ms = prev.last_event_ts_ms.max(item.timestamp);
            prev.last_activity_wall_ms = now_ms;
            prev.last_activity = item.clone();
            prev.segment_started_at_ms = resume_seg_start;
            prev.is_playing = true;
            prev.stuck_finalized = false;
            prev.stale_cold_start = false;

            if prev_completed {
                // Previous play was already >=80% complete. Defer creating a 2nd play
                // until this resumed segment proves it's a real replay (not just finishing
                // the remaining tail of the song).
                prev.pending_repeat = Some(PendingRepeat {
                    activity: item.clone(),
                    started_at_ms: resume_seg_start,
                });
            } else {
                let _ = db.mark_playing_seen(&user_uri, &prev.track_uri, prev.cycle_started_at_ms);
            }

            if prev.session_id.is_none()
                && let Ok(sid) = db.start_session(&user_uri, &item.user.name, now_ms)
            {
                prev.session_id = Some(sid);
            }
            return outcome;
        }

        // Both prev.is_playing and item.playing are true on the same track.
        if item.timestamp > prev.last_event_ts_ms {
            // Fresh Spotify timestamp on the same track while playing.
            let active_elapsed =
                prev.accumulated_played_ms + (item.timestamp - prev.segment_started_at_ms).max(0);
            let mid_cutoff = prev
                .duration_ms
                .filter(|&d| d > 0)
                .map(|d| (d * 85) / 100)
                .unwrap_or(150_000);

            prev.last_event_ts_ms = item.timestamp;
            prev.last_activity_wall_ms = now_ms;
            prev.last_activity = item.clone();
            prev.stuck_finalized = false;
            prev.stale_cold_start = false;

            if active_elapsed >= mid_cutoff && prev.pending_repeat.is_none() {
                // End-of-track transition zone (85%..100%+grace): could be an end-of-track
                // heartbeat 2-5s before the next track OR an exact Repeat-1 restart.
                prev.pending_repeat = Some(PendingRepeat {
                    activity: item,
                    started_at_ms: prev.last_event_ts_ms,
                });
            }

            if let Some(sid) = prev.session_id {
                let _ = db.touch_session(sid, now_ms, false);
            }
            return outcome;
        }

        // Same (or older) timestamp while playing — periodic poll/resync with no new Spotify event.
        if prev.stuck_finalized {
            return outcome;
        }

        if let Some(sid) = prev.session_id {
            let _ = db.touch_session(sid, now_ms, false);
        }

        if let Some(rep) = self.check_single_maintenance(db, &user_uri, now_ms) {
            outcome.repeats.push(rep);
        }

        outcome
    }

    pub fn handle_full_snapshot(
        &mut self,
        db: &Database,
        friends: Vec<FriendActivity>,
        now_ms: i64,
    ) -> (Vec<ActivityOutcome>, Vec<OfflineNotice>) {
        let mut seen_users: HashSet<String> = HashSet::new();
        let mut outcomes = Vec::with_capacity(friends.len());

        for item in friends {
            let out = self.process_activity(db, item, now_ms);
            if let Some(ref uri) = out.user_uri {
                seen_users.insert(uri.clone());
            }
            outcomes.push(out);
        }

        let mut offline_notices = Vec::new();
        for (user_uri, state) in self.active_plays.iter_mut() {
            if !seen_users.contains(user_uri) && state.is_playing {
                state.missed_polls += 1;
                if state.missed_polls > 2 {
                    if !state.stuck_finalized {
                        let seg_ms = (now_ms - state.segment_started_at_ms).max(0);
                        state.accumulated_played_ms =
                            clamp_to_dur(state.accumulated_played_ms + seg_ms, state.duration_ms);
                        let _ = db.finalize_play_with_listened(
                            user_uri,
                            &state.track_uri,
                            state.cycle_started_at_ms,
                            now_ms,
                            state.accumulated_played_ms,
                            state.duration_ms,
                            false,
                        );
                    }
                    if let Some(sid) = state.session_id.take() {
                        let _ = db.end_session(sid, now_ms);
                    }
                    state.is_playing = false;
                    state.stuck_finalized = false;
                    state.pending_repeat = None;
                    offline_notices.push(OfflineNotice {
                        ended_at_ms: now_ms,
                        activity: state.last_activity.clone(),
                    });
                }
            }
        }

        (outcomes, offline_notices)
    }

    pub fn handle_friend_updated(
        &mut self,
        db: &Database,
        activity: FriendActivity,
        now_ms: i64,
    ) -> ActivityOutcome {
        self.process_activity(db, activity, now_ms)
    }

    /// Periodic maintenance tick: finalizes stuck `isPlaying: true` states (when inferred
    /// repeats are disabled) or checks guarded inferred repeats (when explicitly enabled).
    pub fn check_maintenance(&mut self, db: &Database, now_ms: i64) -> Vec<RepeatNotice> {
        let user_uris: Vec<String> = self
            .active_plays
            .iter()
            .filter(|(_, state)| state.is_playing && !state.stuck_finalized)
            .map(|(uri, _)| uri.clone())
            .collect();

        let mut repeats = Vec::new();
        for uri in user_uris {
            if let Some(rep) = self.check_single_maintenance(db, &uri, now_ms) {
                repeats.push(rep);
            }
        }
        repeats
    }

    pub fn close_all_sessions(&mut self, db: &Database, now_ms: i64) {
        for state in self.active_plays.values_mut() {
            if let Some(sid) = state.session_id.take() {
                let _ = db.end_session(sid, now_ms);
            }
        }
    }

    fn try_cold_start_rehydrate(
        &mut self,
        db: &Database,
        user_uri: &str,
        item: &FriendActivity,
        now_ms: i64,
    ) -> bool {
        let Ok(Some(latest)) = db.get_latest_listen_for_user(user_uri) else {
            return false;
        };

        let dur = item.duration_ms().or(latest.duration_ms);

        if latest.track_uri != item.track.uri {
            if !item.playing && item.timestamp <= latest.played_at_ms {
                return true;
            }
            if item.playing && item.timestamp + OUT_OF_ORDER_LIVE_TOLERANCE_MS < latest.played_at_ms
            {
                return true;
            }
            return false;
        }

        // Same track as latest row in DB.
        let delta = item.timestamp - latest.played_at_ms;
        let same_play_window = dur
            .filter(|&d| d > 0)
            .map(|d| d + self.grace_ms)
            .unwrap_or(180_000)
            .max(180_000);
        let prev_listened = latest.listened_ms.unwrap_or(0);
        let prev_completed = is_play_completed(prev_listened, dur);

        let is_cold_last = !item.playing;
        let is_restart_same_play = delta.abs() <= same_play_window;
        let is_post_restart_resume =
            item.playing && delta > same_play_window && delta <= PAUSE_RESUME_MAX_GAP_MS;

        if !is_cold_last && !is_restart_same_play && !is_post_restart_resume {
            return false;
        }

        if item.playing && !latest.is_playing_seen {
            let _ = db.mark_playing_seen(user_uri, &latest.track_uri, latest.played_at_ms);
        }

        let stale_cold_start = item.playing
            && is_restart_same_play
            && is_stuck_playing(latest.played_at_ms, dur, now_ms);

        let mut stuck_finalized = false;
        let mut session_id = None;
        let (segment_started_at_ms, accumulated_played_ms, pending_repeat) = if is_post_restart_resume
        {
            let seg_start = item.timestamp.min(now_ms);
            if prev_completed {
                (
                    seg_start,
                    prev_listened,
                    Some(PendingRepeat {
                        activity: item.clone(),
                        started_at_ms: seg_start,
                    }),
                )
            } else {
                (seg_start, prev_listened, None)
            }
        } else if is_cold_last {
            (latest.played_at_ms, prev_listened, None)
        } else {
            // Restart during the same active play.
            if stale_cold_start && self.max_inferred_repeats == 0 {
                if latest.ended_at_ms.is_none() {
                    let cap_end = match dur {
                        Some(d) if d > 0 => (latest.played_at_ms + d).min(now_ms),
                        _ => now_ms,
                    };
                    let final_listened = match dur {
                        Some(d) if d > 0 => d,
                        _ => (cap_end - latest.played_at_ms).max(0),
                    };
                    let _ = db.finalize_play_with_listened(
                        user_uri,
                        &latest.track_uri,
                        latest.played_at_ms,
                        cap_end,
                        final_listened,
                        dur,
                        false,
                    );
                }
                stuck_finalized = true;
                (
                    latest.played_at_ms,
                    dur.unwrap_or(prev_listened).max(prev_listened),
                    None,
                )
            } else {
                (latest.played_at_ms, 0, None)
            }
        };

        if item.playing && !stuck_finalized {
            let start_ms = segment_started_at_ms.min(now_ms);
            if let Ok(sid) = db.start_session(user_uri, &item.user.name, start_ms) {
                let _ = db.touch_session(sid, now_ms, false);
                session_id = Some(sid);
            }
        }

        self.active_plays.insert(
            user_uri.to_string(),
            ActivePlay {
                track_uri: latest.track_uri,
                cycle_started_at_ms: latest.played_at_ms,
                segment_started_at_ms,
                accumulated_played_ms,
                last_event_ts_ms: item.timestamp.max(latest.played_at_ms),
                last_activity_wall_ms: now_ms,
                duration_ms: dur,
                inferred_cycles: 0,
                is_playing: item.playing,
                stuck_finalized,
                stale_cold_start,
                session_id,
                last_activity: item.clone(),
                missed_polls: 0,
                pending_repeat,
            },
        );

        true
    }

    fn finalize_removed_play(
        &self,
        db: &Database,
        user_uri: &str,
        mut prev: ActivePlay,
        next_item: &FriendActivity,
        now_ms: i64,
        outcome: &mut ActivityOutcome,
    ) -> Option<i64> {
        let mut session_id = prev.session_id.take();
        let switched_to_next_track = prev.track_uri != next_item.track.uri;

        if prev.is_playing && !prev.stuck_finalized {
            if let Some(pending) = prev.pending_repeat.take() {
                let ended_at_ms = next_item.timestamp.clamp(pending.started_at_ms, now_ms);
                let pending_listened = (ended_at_ms - pending.started_at_ms).max(0);
                let confirm_ms = confirm_repeat_threshold(prev.duration_ms);

                if pending_listened >= confirm_ms {
                    // Finalize Play #1 up to when the repeat began.
                    let play1_listened = clamp_to_dur(
                        prev.accumulated_played_ms
                            + (pending.started_at_ms - prev.segment_started_at_ms).max(0),
                        prev.duration_ms,
                    );
                    let _ = db.finalize_play_with_listened(
                        user_uri,
                        &prev.track_uri,
                        prev.cycle_started_at_ms,
                        pending.started_at_ms,
                        play1_listened,
                        prev.duration_ms,
                        false,
                    );

                    // Insert and finalize confirmed Play #2.
                    let mut repeat_act = pending.activity;
                    repeat_act.timestamp = pending.started_at_ms;
                    if db.insert_listen(&repeat_act).unwrap_or(false) {
                        outcome.inserted_live.push(repeat_act.clone());
                    }
                    let play2_listened = clamp_to_dur(pending_listened, prev.duration_ms);
                    if let Err(e) = db.finalize_play_with_listened(
                        user_uri,
                        &prev.track_uri,
                        pending.started_at_ms,
                        ended_at_ms,
                        play2_listened,
                        prev.duration_ms,
                        switched_to_next_track,
                    ) {
                        eprintln!("[error] finalize_play failed: {e}");
                    } else if switched_to_next_track
                        && is_likely_skipped(play2_listened, prev.duration_ms)
                    {
                        outcome.skipped.push(SkipNotice {
                            ended_at_ms,
                            elapsed_ms: play2_listened,
                            duration_ms: prev.duration_ms,
                            activity: repeat_act,
                        });
                    }
                } else {
                    // Short tail (e.g., end-of-track heartbeat or finishing last few seconds
                    // after a pause): absorb into Play #1.
                    let total_listened = clamp_to_dur(
                        prev.accumulated_played_ms
                            + (ended_at_ms - prev.segment_started_at_ms).max(0),
                        prev.duration_ms,
                    );
                    if let Err(e) = db.finalize_play_with_listened(
                        user_uri,
                        &prev.track_uri,
                        prev.cycle_started_at_ms,
                        ended_at_ms,
                        total_listened,
                        prev.duration_ms,
                        switched_to_next_track,
                    ) {
                        eprintln!("[error] finalize_play failed: {e}");
                    } else if switched_to_next_track
                        && is_likely_skipped(total_listened, prev.duration_ms)
                    {
                        outcome.skipped.push(SkipNotice {
                            ended_at_ms,
                            elapsed_ms: total_listened,
                            duration_ms: prev.duration_ms,
                            activity: prev.last_activity,
                        });
                    }
                }
            } else {
                let ended_at_ms = next_item.timestamp.clamp(prev.segment_started_at_ms, now_ms);
                let seg_ms = (ended_at_ms - prev.segment_started_at_ms).max(0);
                let total_listened =
                    clamp_to_dur(prev.accumulated_played_ms + seg_ms, prev.duration_ms);

                if let Err(e) = db.finalize_play_with_listened(
                    user_uri,
                    &prev.track_uri,
                    prev.cycle_started_at_ms,
                    ended_at_ms,
                    total_listened,
                    prev.duration_ms,
                    switched_to_next_track,
                ) {
                    eprintln!("[error] finalize_play failed: {e}");
                } else if switched_to_next_track
                    && is_likely_skipped(total_listened, prev.duration_ms)
                {
                    outcome.skipped.push(SkipNotice {
                        ended_at_ms,
                        elapsed_ms: total_listened,
                        duration_ms: prev.duration_ms,
                        activity: prev.last_activity,
                    });
                }
            }
        }

        if !next_item.playing
            && let Some(sid) = session_id.take()
        {
            let ended_at_ms = next_item.timestamp.clamp(prev.cycle_started_at_ms, now_ms);
            let _ = db.end_session(sid, ended_at_ms);
        }

        session_id
    }

    fn start_new_play(
        &mut self,
        db: &Database,
        user_uri: &str,
        item: FriendActivity,
        now_ms: i64,
        mut session_id: Option<i64>,
        outcome: &mut ActivityOutcome,
    ) {
        let skip_duplicate_last = !item.playing
            && db
                .has_recent_listen_near(
                    user_uri,
                    &item.track.uri,
                    item.timestamp,
                    item.duration_ms().unwrap_or(0).max(180_000),
                )
                .unwrap_or(false);

        let inserted = if skip_duplicate_last {
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

        let dur = item.duration_ms();
        let stale_cold_start = item.playing && is_stuck_playing(item.timestamp, dur, now_ms);
        let mut stuck_finalized = false;

        if stale_cold_start && self.max_inferred_repeats == 0 {
            let cap_end = match dur {
                Some(d) if d > 0 => (item.timestamp + d).min(now_ms),
                _ => now_ms,
            };
            let final_listened = match dur {
                Some(d) if d > 0 => d,
                _ => (cap_end - item.timestamp).max(0),
            };
            let _ = db.finalize_play_with_listened(
                user_uri,
                &item.track.uri,
                item.timestamp,
                cap_end,
                final_listened,
                dur,
                false,
            );
            stuck_finalized = true;
            if let Some(sid) = session_id.take() {
                let _ = db.end_session(sid, cap_end);
            }
        } else if item.playing {
            if let Some(sid) = session_id {
                let _ = db.touch_session(sid, now_ms, true);
            } else {
                let start_ms = item.timestamp.min(now_ms);
                match db.start_session(user_uri, &item.user.name, start_ms) {
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
            outcome.inserted_live.push(item.clone());
        }

        self.active_plays.insert(
            user_uri.to_string(),
            ActivePlay {
                track_uri: item.track.uri.clone(),
                cycle_started_at_ms: item.timestamp,
                segment_started_at_ms: item.timestamp,
                accumulated_played_ms: 0,
                last_event_ts_ms: item.timestamp,
                last_activity_wall_ms: now_ms,
                duration_ms: dur,
                inferred_cycles: 0,
                is_playing: item.playing,
                stuck_finalized,
                stale_cold_start,
                session_id,
                last_activity: item,
                missed_polls: 0,
                pending_repeat: None,
            },
        );
    }

    fn check_single_maintenance(
        &mut self,
        db: &Database,
        user_uri: &str,
        now_ms: i64,
    ) -> Option<RepeatNotice> {
        let prev = self.active_plays.get_mut(user_uri)?;
        if !prev.is_playing || prev.stuck_finalized {
            return None;
        }

        if self.max_inferred_repeats == 0 {
            // Default safe mode: never fabricate plays from wall-clock elapsed time.
            // If Spotify's `isPlaying: true` freezes with no new timestamp past the stale
            // timeout, finalize the single play cleanly and close the session.
            let wall_since_progress = now_ms.saturating_sub(prev.last_activity_wall_ms);
            let stuck_timeout = stale_playing_timeout_ms(prev.duration_ms);
            if wall_since_progress >= stuck_timeout {
                let active_elapsed =
                    prev.accumulated_played_ms + now_ms.saturating_sub(prev.segment_started_at_ms).max(0);
                let cap_end = match prev.duration_ms {
                    Some(d) if d > 0 => (prev.cycle_started_at_ms + d).min(now_ms),
                    _ => prev.last_activity_wall_ms.min(now_ms),
                };
                let final_listened = clamp_to_dur(active_elapsed, prev.duration_ms);
                let _ = db.finalize_play_with_listened(
                    user_uri,
                    &prev.track_uri,
                    prev.cycle_started_at_ms,
                    cap_end,
                    final_listened,
                    prev.duration_ms,
                    false,
                );
                if let Some(sid) = prev.session_id.take() {
                    let _ = db.end_session(sid, cap_end);
                }
                prev.accumulated_played_ms = final_listened;
                prev.stuck_finalized = true;
                prev.pending_repeat = None;
            }
            return None;
        }

        // Explicit opt-in inferred repeats (`--max-inferred-repeats N`):
        // Guarded against cold-start stale timestamps and paused wall-clock time.
        if prev.stale_cold_start {
            return None;
        }

        let dur = prev.duration_ms?;
        if dur < 30_000 || prev.inferred_cycles >= self.max_inferred_repeats {
            return None;
        }

        let active_elapsed =
            prev.accumulated_played_ms + now_ms.saturating_sub(prev.segment_started_at_ms).max(0);
        if active_elapsed < dur + self.grace_ms {
            return None;
        }

        let cycle_end = (prev.segment_started_at_ms + (dur - prev.accumulated_played_ms).max(0))
            .min(now_ms);
        let _ = db.finalize_play_with_listened(
            user_uri,
            &prev.track_uri,
            prev.cycle_started_at_ms,
            cycle_end,
            dur,
            Some(dur),
            false,
        );
        prev.cycle_started_at_ms = cycle_end;
        prev.segment_started_at_ms = cycle_end;
        prev.accumulated_played_ms = 0;
        prev.last_event_ts_ms = prev.last_event_ts_ms.max(cycle_end);
        prev.last_activity_wall_ms = now_ms;
        prev.inferred_cycles += 1;
        prev.pending_repeat = None;

        match db.insert_inferred_repeat(&prev.last_activity, cycle_end) {
            Ok(true) => {
                if let Some(sid) = prev.session_id {
                    let _ = db.touch_session(sid, now_ms, true);
                }
                Some(RepeatNotice {
                    cycle_end_ms: cycle_end,
                    cycle_number: prev.inferred_cycles,
                    duration_ms: dur,
                    activity: prev.last_activity.clone(),
                })
            }
            Ok(false) => None,
            Err(e) => {
                eprintln!("[error] insert_inferred_repeat failed: {e}");
                None
            }
        }
    }
}

pub fn is_likely_skipped(elapsed_ms: i64, duration_ms: Option<i64>) -> bool {
    match duration_ms {
        Some(d) if d >= 30_000 => {
            let threshold = (d / 2).min(d - 15_000);
            elapsed_ms < threshold
        }
        _ => false,
    }
}

fn clamp_to_dur(ms: i64, duration_ms: Option<i64>) -> i64 {
    let clamped = ms.max(0);
    match duration_ms {
        Some(d) if d > 0 => clamped.min(d),
        _ => clamped,
    }
}

fn is_play_completed(accumulated_played_ms: i64, duration_ms: Option<i64>) -> bool {
    match duration_ms {
        Some(d) if d > 0 => accumulated_played_ms >= (d * 80) / 100,
        _ => accumulated_played_ms >= 150_000,
    }
}

fn confirm_repeat_threshold(duration_ms: Option<i64>) -> i64 {
    match duration_ms {
        Some(d) if d > 0 => (d / 2).max(30_000).min(((d * 80) / 100).max(1)),
        _ => 30_000,
    }
}

fn stale_playing_timeout_ms(duration_ms: Option<i64>) -> i64 {
    match duration_ms {
        Some(d) if d > 0 => (d + STALE_PLAYING_EXTRA_MS).max(STALE_PLAYING_MIN_TIMEOUT_MS),
        _ => STALE_PLAYING_MIN_TIMEOUT_MS,
    }
}

fn is_stuck_playing(started_at_ms: i64, duration_ms: Option<i64>, now_ms: i64) -> bool {
    let elapsed = now_ms.saturating_sub(started_at_ms);
    elapsed >= stale_playing_timeout_ms(duration_ms)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::spotify::{FriendTrack, FriendUser, NamedUri};

    fn make_temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bside-tracker-test-{label}-{}-{nanos}.db",
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
    fn stuck_spotify_presence_logs_only_one_play_even_across_hour_and_restart() {
        let db_path = make_temp_db_path("stuck-presence");
        let db = Database::open(&db_path).expect("open db");
        let t0 = 1_795_607_888_000i64; // 12:58:08
        let dur = 157_720i64; // Tak to lecialo (2:37)

        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);
        let start = make_activity("wrobelek", "wrobelek", "tak", "Tak to lecialo", t0, true, Some(dur));
        tracker.handle_friend_updated(&db, start.clone(), t0);

        // Simulate 1 hour of stuck `isPlaying: true` polls & maintenance ticks every 60s.
        for min in 1..=60i64 {
            let now = t0 + min * 60_000;
            tracker.handle_full_snapshot(&db, vec![start.clone()], now);
            let reps = tracker.check_maintenance(&db, now);
            assert!(reps.is_empty(), "must never fabricate repeats on stuck presence");
        }

        // Simulate service restart 33 minutes later while Spotify still returns stale isPlaying: true
        // with a slightly bumped timestamp (+129s, inside track duration).
        let mut restarted_tracker = PlayTracker::new(Vec::new(), 0, 25_000);
        let restart_now = t0 + 33 * 60_000;
        let stale_after_restart = make_activity(
            "wrobelek",
            "wrobelek",
            "tak",
            "Tak to lecialo",
            t0 + 129_152,
            true,
            Some(dur),
        );
        restarted_tracker.handle_full_snapshot(&db, vec![stale_after_restart.clone()], restart_now);
        restarted_tracker.check_maintenance(&db, restart_now + 15_000);

        // Spotify finally sends isPlaying: false an hour later.
        let stop = make_activity(
            "wrobelek",
            "wrobelek",
            "tak",
            "Tak to lecialo",
            t0 + 129_152,
            false,
            Some(dur),
        );
        restarted_tracker.handle_friend_updated(&db, stop, t0 + 67 * 60_000);

        let history = db.query_history(Some("wrobelek"), None, None).unwrap();
        assert_eq!(history.len(), 1, "must record exactly 1 play, got {}", history.len());
        assert!(!history[0].inferred_repeat);
        assert_eq!(history[0].was_skipped, Some(false));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn pause_and_resume_across_long_gaps_counts_as_single_completed_play() {
        let db_path = make_temp_db_path("pause-resume");
        let db = Database::open(&db_path).expect("open db");
        let t0 = 1_795_600_000_000i64;
        let dur = 220_000i64; // A$AP Rocky - LVL (3:40)

        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);

        // Segment 1: plays 34s, then pauses
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "lvl", "LVL", t0, true, Some(dur)),
            t0,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "lvl", "LVL", t0 + 34_000, false, Some(dur)),
            t0 + 34_000,
        );

        // Segment 2: resumes 11 minutes later, plays 46s, pauses
        let t1 = t0 + 713_000;
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "lvl", "LVL", t1, true, Some(dur)),
            t1,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "lvl", "LVL", t1 + 46_000, false, Some(dur)),
            t1 + 46_000,
        );

        // Segment 3: resumes 7 minutes later, plays remaining 100s, then switches to next track
        let t2 = t1 + 465_000;
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "lvl", "LVL", t2, true, Some(dur)),
            t2,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "landline", "Landline", t2 + 100_000, true, Some(160_000)),
            t2 + 100_000,
        );

        let mut history = db.query_history(Some("ksiksen"), None, None).unwrap();
        history.sort_by_key(|r| r.played_at_ms);
        assert_eq!(history.len(), 2, "LVL + Landline = 2 rows");
        assert_eq!(history[0].track_uri, "spotify:track:lvl");
        assert_eq!(history[0].listened_ms, Some(180_000)); // 34 + 46 + 100 = 180s!
        assert_eq!(history[0].was_skipped, Some(false));
        assert_eq!(history[1].track_uri, "spotify:track:landline");

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn pause_near_end_and_resume_for_remaining_tail_does_not_duplicate() {
        let db_path = make_temp_db_path("pause-tail");
        let db = Database::open(&db_path).expect("open db");
        let t0 = 1_795_600_000_000i64;
        let dur = 247_000i64; // patrz mi w oczy (4:07)

        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);
        // Plays 236s (95.5%), pauses
        tracker.handle_friend_updated(
            &db,
            make_activity("wrobelek", "wrobelek", "patrz", "patrz mi w oczy", t0, true, Some(dur)),
            t0,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("wrobelek", "wrobelek", "patrz", "patrz mi w oczy", t0 + 236_000, false, Some(dur)),
            t0 + 236_000,
        );
        // Resumes 10 minutes later, finishes last 12s, switches to next track
        let t1 = t0 + 836_000;
        tracker.handle_friend_updated(
            &db,
            make_activity("wrobelek", "wrobelek", "patrz", "patrz mi w oczy", t1, true, Some(dur)),
            t1,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("wrobelek", "wrobelek", "deli", "DELI SUPREME", t1 + 12_000, true, Some(151_000)),
            t1 + 12_000,
        );

        let mut history = db.query_history(Some("wrobelek"), None, None).unwrap();
        history.sort_by_key(|r| r.played_at_ms);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].track_uri, "spotify:track:patrz");
        assert_eq!(history[0].listened_ms, Some(dur));
        assert_eq!(history[0].was_skipped, Some(false));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn pause_after_complete_play_then_full_replay_records_two_plays() {
        let db_path = make_temp_db_path("genuine-replay-after-pause");
        let db = Database::open(&db_path).expect("open db");
        let t0 = 1_795_600_000_000i64;
        let dur = 180_000i64;

        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);
        // Full play 175s then pause
        tracker.handle_friend_updated(
            &db,
            make_activity("alice", "Alice", "t1", "Song 1", t0, true, Some(dur)),
            t0,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("alice", "Alice", "t1", "Song 1", t0 + 175_000, false, Some(dur)),
            t0 + 175_000,
        );
        // 5 minutes later, replays the whole 180s track and switches
        let t1 = t0 + 475_000;
        tracker.handle_friend_updated(
            &db,
            make_activity("alice", "Alice", "t1", "Song 1", t1, true, Some(dur)),
            t1,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("alice", "Alice", "t2", "Song 2", t1 + 180_000, true, Some(180_000)),
            t1 + 180_000,
        );

        let mut history = db.query_history(Some("alice"), None, None).unwrap();
        history.sort_by_key(|r| r.played_at_ms);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].track_uri, "spotify:track:t1");
        assert_eq!(history[0].listened_ms, Some(175_000));
        assert_eq!(history[0].was_skipped, Some(false));
        assert_eq!(history[1].track_uri, "spotify:track:t1");
        assert_eq!(history[1].listened_ms, Some(180_000));
        assert_eq!(history[1].was_skipped, Some(false));
        assert_eq!(history[2].track_uri, "spotify:track:t2");

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn out_of_order_stale_last_snapshot_is_rejected() {
        let db_path = make_temp_db_path("out-of-order");
        let db = Database::open(&db_path).expect("open db");
        let t0 = 1_795_600_000_000i64;

        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);
        // 09:05:43 TV Girl
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "tv", "TV Girl", t0, true, Some(199_000)),
            t0,
        );
        // 09:05:48 PRO8L3M
        tracker.handle_friend_updated(
            &db,
            make_activity("ksiksen", "Ksiksen", "pro", "PRO8L3M", t0 + 5_000, true, Some(214_000)),
            t0 + 5_000,
        );
        // Stale snapshot at 09:08:13 arrives with OLDER timestamp 09:05:47 [LAST] TV Girl!
        tracker.handle_full_snapshot(
            &db,
            vec![make_activity(
                "ksiksen",
                "Ksiksen",
                "tv",
                "TV Girl",
                t0 + 4_000,
                false,
                Some(199_000),
            )],
            t0 + 150_000,
        );

        let mut history = db.query_history(Some("ksiksen"), None, None).unwrap();
        history.sort_by_key(|r| r.played_at_ms);
        assert_eq!(history.len(), 2, "must not insert duplicate stale [LAST] row");
        assert_eq!(history[1].track_uri, "spotify:track:pro");
        assert_eq!(history[1].was_skipped, None, "PRO8L3M must not be falsely skipped");

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn restart_with_small_timestamp_jitter_does_not_duplicate() {
        let db_path = make_temp_db_path("restart-jitter");
        let db = Database::open(&db_path).expect("open db");
        let t0 = 1_795_600_000_000i64;
        let dur = 227_400i64;

        let mut tracker1 = PlayTracker::new(Vec::new(), 0, 25_000);
        tracker1.handle_friend_updated(
            &db,
            make_activity("roma", "Roma", "lilbaby", "Dum, Dumb, and Dumber", t0, true, Some(dur)),
            t0,
        );

        // Service restarts 108s later; OwnPlaybackObserver estimates start +600ms off.
        let mut tracker2 = PlayTracker::new(Vec::new(), 0, 25_000);
        tracker2.handle_friend_updated(
            &db,
            make_activity(
                "roma",
                "Roma",
                "lilbaby",
                "Dum, Dumb, and Dumber",
                t0 + 600,
                true,
                Some(dur),
            ),
            t0 + 108_000,
        );
        // Track finishes normally at t0 + 227_400.
        tracker2.handle_friend_updated(
            &db,
            make_activity("roma", "Roma", "next", "Next Track", t0 + dur, true, Some(180_000)),
            t0 + dur,
        );

        let mut history = db.query_history(Some("roma"), None, None).unwrap();
        history.sort_by_key(|r| r.played_at_ms);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].track_uri, "spotify:track:lilbaby");
        assert_eq!(history[0].played_at_ms, t0);
        assert_eq!(history[0].was_skipped, Some(false));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn end_of_track_heartbeat_vs_genuine_back_to_back_repeat() {
        let db_path = make_temp_db_path("eot-vs-repeat");
        let db = Database::open(&db_path).expect("open db");
        let t0 = 1_795_619_146_000i64;
        let dur = 124_000i64;

        // Case 1: End-of-track heartbeat at +121.2s, next track 3.4s later -> 1 play!
        let mut tracker = PlayTracker::new(Vec::new(), 0, 25_000);
        tracker.handle_friend_updated(
            &db,
            make_activity("alice", "Alice", "bp", "Brain Power", t0, true, Some(dur)),
            t0,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("alice", "Alice", "bp", "Brain Power", t0 + 121_204, true, Some(dur)),
            t0 + 121_204,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("alice", "Alice", "rech", "Recharge", t0 + 124_640, true, Some(128_000)),
            t0 + 124_640,
        );

        let alice_hist = db.query_history(Some("alice"), None, None).unwrap();
        assert_eq!(alice_hist.len(), 2);
        let bp = alice_hist.iter().find(|r| r.track_uri == "spotify:track:bp").unwrap();
        assert_eq!(bp.was_skipped, Some(false));
        assert_eq!(bp.listened_ms, Some(dur));

        // Case 2: Genuine back-to-back repeat at +124s that plays full 124s -> 2 plays!
        tracker.handle_friend_updated(
            &db,
            make_activity("bob", "Bob", "bp", "Brain Power", t0, true, Some(dur)),
            t0,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("bob", "Bob", "bp", "Brain Power", t0 + dur, true, Some(dur)),
            t0 + dur,
        );
        tracker.handle_friend_updated(
            &db,
            make_activity("bob", "Bob", "rech", "Recharge", t0 + dur * 2, true, Some(128_000)),
            t0 + dur * 2,
        );

        let bob_hist = db.query_history(Some("bob"), None, None).unwrap();
        let bob_bp: Vec<_> = bob_hist.iter().filter(|r| r.track_uri == "spotify:track:bp").collect();
        assert_eq!(bob_bp.len(), 2, "genuine back-to-back repeat must count as 2 plays");
        assert!(bob_bp.iter().all(|r| r.was_skipped == Some(false)));

        let _ = std::fs::remove_file(&db_path);
    }
}
