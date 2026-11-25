use std::sync::{Arc, RwLock};

use chrono::Utc;
use futures_util::StreamExt;
use librespot_core::{
    Error as LibrespotError, dealer::protocol::Message as DealerMessage, session::Session,
};
use librespot_protocol::{
    connect::{
        Capabilities, Cluster, ClusterUpdate, Device, DeviceInfo, MemberType, PutStateReason,
        PutStateRequest,
    },
    devices::DeviceType,
};
use protobuf::{Message, MessageField};
use tokio::sync::{Mutex, watch};

use super::models::{
    FriendActivity, FriendTrack, FriendUser, NamedUri, TrackContext, canonical_spotify_dj_uri,
    classify_context_type, is_spotify_dj_context, normalize_context_uri,
};

const PUSHER_CONNECTIONS_URI: &str = "hm://pusher/v1/connections/";
const CONNECT_CLUSTER_URI: &str = "hm://connect-state/v1/cluster";
const TIMESTAMP_STABILIZE_WINDOW_MS: i64 = 20_000;

#[derive(Debug, Clone)]
struct OwnProfile {
    user_uri: String,
    display_name: String,
    image_url: Option<String>,
}

pub struct OwnPlaybackObserver {
    session: Session,
    latest: Arc<RwLock<Option<FriendActivity>>>,
    profile: Arc<RwLock<OwnProfile>>,
    #[allow(dead_code)]
    updates_rx: watch::Receiver<u64>,
    shutdown_tx: watch::Sender<bool>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl OwnPlaybackObserver {
    pub fn spawn(session: Session, user_name: String, user_image_url: Option<String>) -> Self {
        let username = session.username();
        let display_name = if user_name.trim().is_empty() {
            username.clone()
        } else {
            user_name
        };
        let profile = Arc::new(RwLock::new(OwnProfile {
            user_uri: format!("spotify:user:{username}"),
            display_name,
            image_url: user_image_url,
        }));
        let latest = Arc::new(RwLock::new(None));
        let (updates_tx, updates_rx) = watch::channel(0u64);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let task = tokio::spawn(run_observer_loop(
            session.clone(),
            Arc::clone(&latest),
            Arc::clone(&profile),
            updates_tx,
            shutdown_rx,
        ));

        Self {
            session,
            latest,
            profile,
            updates_rx,
            shutdown_tx,
            task: Mutex::new(Some(task)),
        }
    }

    pub fn latest(&self) -> Option<FriendActivity> {
        self.latest
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }

    #[allow(dead_code)]
    pub fn subscribe_updates(&self) -> watch::Receiver<u64> {
        self.updates_rx.clone()
    }

    pub fn update_profile(&self, display_name: String, image_url: Option<String>) {
        let trimmed = display_name.trim().to_string();
        let updated_name = if let Ok(mut guard) = self.profile.write() {
            if !trimmed.is_empty() {
                guard.display_name = trimmed;
            }
            guard.image_url = image_url.clone();
            guard.display_name.clone()
        } else {
            return;
        };

        if let Ok(mut guard) = self.latest.write()
            && let Some(activity) = guard.as_mut()
        {
            activity.user.name = updated_name;
            activity.user.image_url = image_url;
        }
    }

    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.session.spclient().delete_connect_state_request().await;
        self.session.dealer().close().await;
        if let Some(handle) = self.task.lock().await.take() {
            handle.abort();
        }
    }
}

fn extract_connection_id(msg: DealerMessage) -> Result<String, LibrespotError> {
    msg.headers
        .get("Spotify-Connection-Id")
        .or_else(|| {
            msg.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("spotify-connection-id"))
                .map(|(_, v)| v)
        })
        .cloned()
        .ok_or_else(|| LibrespotError::failed_precondition("missing Spotify-Connection-Id header"))
}

fn build_put_state_request(
    device_id: &str,
    reason: PutStateReason,
    client_side_timestamp: u64,
) -> PutStateRequest {
    let capabilities = Capabilities {
        can_be_player: false,
        hidden: true,
        is_observable: false,
        is_controllable: false,
        needs_full_player_state: true,
        supports_gzip_pushes: true,
        ..Default::default()
    };

    let device_info = DeviceInfo {
        can_play: false,
        device_id: device_id.to_string(),
        name: "bside".to_string(),
        device_type: DeviceType::OBSERVER.into(),
        capabilities: MessageField::some(capabilities),
        ..Default::default()
    };

    let device = Device {
        device_info: MessageField::some(device_info),
        ..Default::default()
    };

    PutStateRequest {
        member_type: MemberType::CONNECT_STATE.into(),
        is_active: false,
        put_state_reason: reason.into(),
        client_side_timestamp,
        device: MessageField::some(device),
        ..Default::default()
    }
}

fn apply_cluster_update(
    cluster: &Cluster,
    latest: &Arc<RwLock<Option<FriendActivity>>>,
    profile: &Arc<RwLock<OwnProfile>>,
    updates_tx: &watch::Sender<u64>,
) {
    let prof = match profile.read() {
        Ok(guard) => guard.clone(),
        Err(_) => return,
    };
    let now_ms = Utc::now().timestamp_millis();

    let updated = if let Ok(mut guard) = latest.write() {
        let next = cluster_to_activity(
            cluster,
            &prof.user_uri,
            &prof.display_name,
            prof.image_url.as_deref(),
            guard.as_ref(),
            now_ms,
        );
        *guard = next;
        true
    } else {
        false
    };

    if updated {
        let next_ver = updates_tx.borrow().wrapping_add(1);
        let _ = updates_tx.send(next_ver);
    }
}

async fn run_observer_loop(
    session: Session,
    latest: Arc<RwLock<Option<FriendActivity>>>,
    profile: Arc<RwLock<OwnProfile>>,
    updates_tx: watch::Sender<u64>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut conn_id_stream = match session
        .dealer()
        .listen_for(PUSHER_CONNECTIONS_URI, extract_connection_id)
    {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("[warn] Own playback observer failed to subscribe to connection id: {e}");
            return;
        }
    };

    let mut cluster_stream = match session
        .dealer()
        .listen_for(CONNECT_CLUSTER_URI, DealerMessage::from_raw::<ClusterUpdate>)
    {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("[warn] Own playback observer failed to subscribe to cluster updates: {e}");
            return;
        }
    };

    if let Err(e) = session.dealer().start().await {
        eprintln!("[warn] Own playback observer failed to start dealer: {e}");
        return;
    }

    let mut has_connected = false;

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            maybe_conn = conn_id_stream.next() => {
                match maybe_conn {
                    Some(Ok(conn_id)) => {
                        session.set_connection_id(&conn_id);
                        let reason = if has_connected {
                            PutStateReason::NEW_CONNECTION
                        } else {
                            PutStateReason::NEW_DEVICE
                        };
                        has_connected = true;

                        let now_ms = Utc::now().timestamp_millis().max(0) as u64;
                        let req = build_put_state_request(session.device_id(), reason, now_ms);
                        match session.spclient().put_connect_state_request(&req).await {
                            Ok(bytes) => match Cluster::parse_from_bytes(&bytes) {
                                Ok(cluster) => {
                                    apply_cluster_update(&cluster, &latest, &profile, &updates_tx);
                                }
                                Err(e) => {
                                    eprintln!("[warn] Own playback observer failed to parse initial cluster: {e}");
                                }
                            },
                            Err(e) => {
                                eprintln!("[warn] Own playback observer put_connect_state_request failed: {e}");
                            }
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("[warn] Own playback observer connection-id error: {e}");
                    }
                    None => break,
                }
            }
            maybe_cluster = cluster_stream.next() => {
                match maybe_cluster {
                    Some(Ok(update)) => {
                        if let Some(cluster) = update.cluster.as_ref() {
                            apply_cluster_update(cluster, &latest, &profile, &updates_tx);
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("[warn] Own playback observer cluster update error: {e}");
                    }
                    None => break,
                }
            }
        }
    }
}

fn normalize_spotify_image_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(id) = trimmed.strip_prefix("spotify:image:") {
        if id.is_empty() {
            None
        } else {
            Some(format!("https://i.scdn.co/image/{id}"))
        }
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn idle_from_prev(
    prev: Option<&FriendActivity>,
    user_uri: &str,
    user_name: &str,
    user_image_url: Option<&str>,
) -> Option<FriendActivity> {
    prev.map(|p| {
        let mut cloned = p.clone();
        cloned.playing = false;
        cloned.user = FriendUser {
            uri: user_uri.to_string(),
            name: user_name.to_string(),
            image_url: user_image_url.map(str::to_string),
        };
        cloned
    })
}

pub(crate) fn cluster_to_activity(
    cluster: &Cluster,
    user_uri: &str,
    user_name: &str,
    user_image_url: Option<&str>,
    prev: Option<&FriendActivity>,
    now_ms: i64,
) -> Option<FriendActivity> {
    let Some(player_state) = cluster.player_state.as_ref() else {
        return idle_from_prev(prev, user_uri, user_name, user_image_url);
    };
    let Some(track) = player_state.track.as_ref() else {
        return idle_from_prev(prev, user_uri, user_name, user_image_url);
    };

    let track_uri = track.uri.trim();
    if track_uri.is_empty()
        || track_uri.starts_with("spotify:ad:")
        || track_uri.starts_with("spotify:delimiter")
    {
        return idle_from_prev(prev, user_uri, user_name, user_image_url);
    }

    let playing = player_state.is_playing && !player_state.is_paused;

    let meta = &track.metadata;
    let track_name = meta
        .get("title")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(track_uri)
        .to_string();

    let mut artists: Vec<NamedUri> = Vec::new();
    let primary_artist_name = meta
        .get("artist_name")
        .map(String::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let primary_artist_uri = if !track.artist_uri.trim().is_empty() {
        track.artist_uri.trim()
    } else {
        meta.get("artist_uri")
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default()
    };
    if !primary_artist_name.is_empty() || !primary_artist_uri.is_empty() {
        artists.push(NamedUri {
            uri: primary_artist_uri.to_string(),
            name: primary_artist_name.to_string(),
        });
    }
    for idx in 1..=10 {
        let name_key = format!("artist_name:{idx}");
        let uri_key = format!("artist_uri:{idx}");
        let extra_name = meta
            .get(&name_key)
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let extra_uri = meta
            .get(&uri_key)
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if extra_name.is_empty() && extra_uri.is_empty() {
            break;
        }
        artists.push(NamedUri {
            uri: extra_uri.to_string(),
            name: extra_name.to_string(),
        });
    }
    let artist = artists.first().cloned();

    let album_name = meta
        .get("album_title")
        .map(String::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let album_uri = if !track.album_uri.trim().is_empty() {
        track.album_uri.trim()
    } else {
        meta.get("album_uri")
            .map(String::as_str)
            .map(str::trim)
            .unwrap_or_default()
    };
    let album = if !album_name.is_empty() || !album_uri.is_empty() {
        Some(NamedUri {
            uri: album_uri.to_string(),
            name: album_name.to_string(),
        })
    } else {
        None
    };

    let cover_url = ["image_xlarge_url", "image_large_url", "image_url", "image_small_url"]
        .into_iter()
        .find_map(|key| meta.get(key).and_then(|v| normalize_spotify_image_url(v)));

    let duration_ms = if player_state.duration > 0 {
        Some(player_state.duration)
    } else {
        meta.get("duration")
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|&d| d > 0)
    };

    let item_type = if track_uri.starts_with("spotify:episode:") {
        "episode".to_string()
    } else {
        "track".to_string()
    };

    let raw_ctx = player_state.context_uri.trim();
    let context = if !raw_ctx.is_empty() {
        let ctx_uri = normalize_context_uri(raw_ctx);
        let ctx_meta_title = player_state
            .context_metadata
            .get("context_description")
            .or_else(|| player_state.context_metadata.get("title"))
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let is_liked_songs = ctx_uri.eq_ignore_ascii_case("spotify:collection")
            || ctx_uri.ends_with(":collection")
            || ctx_uri.contains(":collection:")
            || ctx_meta_title.is_some_and(|t| t.eq_ignore_ascii_case("liked songs"));
        let is_dj = is_spotify_dj_context(Some(&ctx_uri), None);

        if is_liked_songs || is_dj {
            let uid = user_uri
                .strip_prefix("spotify:user:")
                .unwrap_or(user_uri)
                .trim();
            let owner_name = if user_name.trim().is_empty() {
                uid.to_string()
            } else {
                user_name.trim().to_string()
            };
            if is_liked_songs {
                Some(TrackContext {
                    uri: format!("spotify:user:{uid}:collection"),
                    name: "Liked Songs".to_string(),
                    index: None,
                    context_type: "liked_songs".to_string(),
                    owner: Some(owner_name),
                    is_private: true,
                    image_url: None,
                })
            } else {
                Some(TrackContext {
                    uri: canonical_spotify_dj_uri(user_uri),
                    name: "DJ".to_string(),
                    index: None,
                    context_type: "dj".to_string(),
                    owner: Some(owner_name),
                    is_private: true,
                    image_url: None,
                })
            }
        } else {
            let is_album_ctx = album.as_ref().is_some_and(|a| a.uri == ctx_uri);
            let ctx_name = if is_album_ctx {
                album
                    .as_ref()
                    .map(|a| a.name.clone())
                    .filter(|s| !s.is_empty())
                    .or_else(|| ctx_meta_title.map(str::to_string))
                    .unwrap_or_default()
            } else if let Some(a) = artists.iter().find(|a| a.uri == ctx_uri) {
                a.name.clone()
            } else if ctx_uri == track_uri {
                track_name.clone()
            } else {
                ctx_meta_title.unwrap_or_default().to_string()
            };

            let context_type = classify_context_type(&ctx_uri, &ctx_name).to_string();
            let is_private = context_type == "liked_songs";
            let ctx_image_url = if is_album_ctx {
                cover_url.clone()
            } else {
                player_state
                    .context_metadata
                    .get("image_url")
                    .and_then(|v| normalize_spotify_image_url(v))
            };

            Some(TrackContext {
                uri: ctx_uri,
                name: ctx_name,
                index: None,
                context_type,
                owner: None,
                is_private,
                image_url: ctx_image_url,
            })
        }
    } else {
        None
    };

    let estimated_start = if player_state.timestamp > 0 {
        let pos = player_state.position_as_of_timestamp.max(0);
        player_state.timestamp.saturating_sub(pos)
    } else if let Some(started) = cluster.started_playing_at_timestamp.filter(|&ts| ts > 0) {
        started as i64
    } else {
        now_ms
    };

    let timestamp = if let Some(prev_act) = prev
        && prev_act.track.uri == track_uri
        && (estimated_start - prev_act.timestamp).abs() <= TIMESTAMP_STABILIZE_WINDOW_MS
    {
        prev_act.timestamp
    } else {
        estimated_start
    };

    Some(FriendActivity {
        timestamp,
        playing,
        user: FriendUser {
            uri: user_uri.to_string(),
            name: user_name.to_string(),
            image_url: user_image_url.map(str::to_string),
        },
        track: FriendTrack {
            uri: track_uri.to_string(),
            name: track_name,
            image_url: cover_url.clone(),
            album,
            artist,
            context,
            item_type,
            artists,
            duration_ms,
            cover_url,
            ..Default::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use librespot_protocol::connect::Cluster;
    use librespot_protocol::player::{PlayerState, ProvidedTrack};
    use protobuf::MessageField;

    use super::cluster_to_activity;

    fn sample_cluster(
        uri: &str,
        is_playing: bool,
        is_paused: bool,
        timestamp: i64,
        position_ms: i64,
        context_uri: &str,
    ) -> Cluster {
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), "Midnight City".to_string());
        metadata.insert("artist_name".to_string(), "M83".to_string());
        metadata.insert("album_title".to_string(), "Hurry Up, We're Dreaming".to_string());
        metadata.insert(
            "image_url".to_string(),
            "spotify:image:ab67616d0000b273123456".to_string(),
        );
        metadata.insert("duration".to_string(), "243000".to_string());

        let track = ProvidedTrack {
            uri: uri.to_string(),
            artist_uri: "spotify:artist:63MQldklfxkjYDoUE4Tppz".to_string(),
            album_uri: "spotify:album:6R0ynY7RF20ofs9GJR5TXR".to_string(),
            metadata,
            ..Default::default()
        };

        let player_state = PlayerState {
            timestamp,
            position_as_of_timestamp: position_ms,
            duration: 243_000,
            is_playing,
            is_paused,
            context_uri: context_uri.to_string(),
            track: MessageField::some(track),
            ..Default::default()
        };

        Cluster {
            player_state: MessageField::some(player_state),
            ..Default::default()
        }
    }

    #[test]
    fn cluster_to_activity_parses_playing_track_and_context() {
        let cluster = sample_cluster(
            "spotify:track:1eyzqe2QqGZUmfcPZtrIyt",
            true,
            false,
            1_700_000_050_000,
            50_000,
            "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
        );

        let act = cluster_to_activity(
            &cluster,
            "spotify:user:roma",
            "Roma",
            Some("https://i.scdn.co/image/avatar"),
            None,
            1_700_000_055_000,
        )
        .expect("expected activity");

        assert!(act.playing);
        assert_eq!(act.timestamp, 1_700_000_000_000);
        assert_eq!(act.user.uri, "spotify:user:roma");
        assert_eq!(act.user.name, "Roma");
        assert_eq!(
            act.user.image_url.as_deref(),
            Some("https://i.scdn.co/image/avatar")
        );
        assert_eq!(act.track.uri, "spotify:track:1eyzqe2QqGZUmfcPZtrIyt");
        assert_eq!(act.track.name, "Midnight City");
        assert_eq!(act.all_artists_display(), "M83");
        assert_eq!(act.album_name(), Some("Hurry Up, We're Dreaming"));
        assert_eq!(act.duration_ms(), Some(243_000));
        assert_eq!(
            act.track.cover_url.as_deref(),
            Some("https://i.scdn.co/image/ab67616d0000b273123456")
        );
        assert_eq!(
            act.context_uri(),
            Some("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M")
        );
        assert_eq!(act.context_type(), Some("playlist"));
    }

    #[test]
    fn cluster_to_activity_handles_paused_state() {
        let cluster = sample_cluster(
            "spotify:track:1eyzqe2QqGZUmfcPZtrIyt",
            true,
            true,
            1_700_000_050_000,
            50_000,
            "spotify:user:roma:collection",
        );

        let act = cluster_to_activity(
            &cluster,
            "spotify:user:roma",
            "Roma",
            None,
            None,
            1_700_000_055_000,
        )
        .expect("expected paused activity");

        assert!(!act.playing);
        assert_eq!(act.context_type(), Some("liked_songs"));
        assert_eq!(act.context_name(), Some("Liked Songs"));
        assert!(act.is_private_context());
    }

    #[test]
    fn cluster_to_activity_scopes_spotify_dj_per_user_and_private() {
        let cluster = sample_cluster(
            "spotify:track:1eyzqe2QqGZUmfcPZtrIyt",
            true,
            false,
            1_700_000_050_000,
            50_000,
            "spotify:playlist:37i9dQZF1EYkqdzj48dyYq",
        );

        let act = cluster_to_activity(
            &cluster,
            "spotify:user:roma",
            "Roma",
            None,
            None,
            1_700_000_055_000,
        )
        .expect("expected DJ activity");

        assert!(act.playing);
        assert_eq!(act.context_uri(), Some("spotify:user:roma:dj"));
        assert_eq!(act.context_type(), Some("dj"));
        assert_eq!(act.context_name(), Some("DJ"));
        assert_eq!(act.context_owner(), Some("Roma"));
        assert!(act.is_private_context());
        assert!(act.context_image_url().is_none());
    }

    #[test]
    fn cluster_to_activity_ignores_ads_and_delimiters() {
        let prev = cluster_to_activity(
            &sample_cluster(
                "spotify:track:1eyzqe2QqGZUmfcPZtrIyt",
                true,
                false,
                1_700_000_050_000,
                50_000,
                "",
            ),
            "spotify:user:roma",
            "Roma",
            None,
            None,
            1_700_000_055_000,
        )
        .unwrap();

        let ad_cluster = sample_cluster(
            "spotify:ad:12345",
            true,
            false,
            1_700_000_060_000,
            1_000,
            "",
        );
        assert!(
            cluster_to_activity(
                &ad_cluster,
                "spotify:user:roma",
                "Roma",
                None,
                None,
                1_700_000_060_000,
            )
            .is_none()
        );

        let idle_after_ad = cluster_to_activity(
            &ad_cluster,
            "spotify:user:roma",
            "Roma",
            None,
            Some(&prev),
            1_700_000_060_000,
        )
        .expect("should retain previous track as paused/idle");
        assert!(!idle_after_ad.playing);
        assert_eq!(idle_after_ad.track.uri, prev.track.uri);

        let delim_cluster = sample_cluster(
            "spotify:delimiter",
            true,
            false,
            1_700_000_070_000,
            0,
            "",
        );
        let idle_after_delim = cluster_to_activity(
            &delim_cluster,
            "spotify:user:roma",
            "Roma",
            None,
            Some(&prev),
            1_700_000_070_000,
        )
        .expect("should retain previous track as paused/idle");
        assert!(!idle_after_delim.playing);
        assert_eq!(idle_after_delim.track.uri, prev.track.uri);
    }

    #[test]
    fn cluster_to_activity_stabilizes_timestamp_across_position_updates() {
        let first = cluster_to_activity(
            &sample_cluster(
                "spotify:track:1eyzqe2QqGZUmfcPZtrIyt",
                true,
                false,
                1_700_000_050_000,
                50_000,
                "",
            ),
            "spotify:user:roma",
            "Roma",
            None,
            None,
            1_700_000_050_000,
        )
        .unwrap();
        assert_eq!(first.timestamp, 1_700_000_000_000);

        // 15s later with 3s of clock/position jitter => estimated start is +3s (within ±20s)
        let second = cluster_to_activity(
            &sample_cluster(
                "spotify:track:1eyzqe2QqGZUmfcPZtrIyt",
                true,
                false,
                1_700_000_068_000,
                65_000,
                "",
            ),
            "spotify:user:roma",
            "Roma",
            None,
            Some(&first),
            1_700_000_068_000,
        )
        .unwrap();
        assert_eq!(second.timestamp, first.timestamp);

        // Replay of the same track 250s later => outside ±20s window, gets new timestamp
        let replay = cluster_to_activity(
            &sample_cluster(
                "spotify:track:1eyzqe2QqGZUmfcPZtrIyt",
                true,
                false,
                1_700_000_255_000,
                5_000,
                "",
            ),
            "spotify:user:roma",
            "Roma",
            None,
            Some(&second),
            1_700_000_255_000,
        )
        .unwrap();
        assert_eq!(replay.timestamp, 1_700_000_250_000);
    }
}
