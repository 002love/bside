use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::{Mutex, broadcast, watch};

pub const DEFAULT_DRIFT_THRESHOLD_MS: i64 = 5_000;
pub const MIN_DRIFT_THRESHOLD_MS: i64 = 1_500;
pub const MAX_DRIFT_THRESHOLD_MS: i64 = 60_000;

pub const DEFAULT_LATENCY_OFFSET_MS: i64 = 500;
pub const MIN_LATENCY_OFFSET_MS: i64 = -5_000;
pub const MAX_LATENCY_OFFSET_MS: i64 = 10_000;

const COMMAND_GRACE_WINDOW_MS: i64 = 4_000;
const AUTO_RESYNC_COOLDOWN_MS: i64 = 15_000;
const HOST_REPLAY_JUMP_THRESHOLD_MS: i64 = 15_000;
const SPOTIFY_DJ_PLAYLIST_ID: &str = "37i9dQZF1EYkqdzj48dyYq";
const STATE_BROADCAST_CAPACITY: usize = 64;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenAlongStatus {
    #[default]
    Idle,
    Connecting,
    Waiting,
    Syncing,
    Synced,
    HostPaused,
    Diverged,
    Error,
}

impl ListenAlongStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Waiting => "waiting",
            Self::Syncing => "syncing",
            Self::Synced => "synced",
            Self::HostPaused => "host_paused",
            Self::Diverged => "diverged",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for ListenAlongStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListenAlongTarget {
    pub user_uri: String,
    pub user_name: String,
    pub user_image_url: Option<String>,
    pub playing: bool,
    pub timestamp_ms: i64,
    pub track_uri: Option<String>,
    pub track_name: Option<String>,
    pub all_artists: Option<String>,
    pub album_name: Option<String>,
    pub cover_url: Option<String>,
    pub duration_ms: Option<i64>,
    pub context_uri: Option<String>,
    pub context_name: Option<String>,
    pub context_type: Option<String>,
    pub context_is_private: bool,
    pub spotify_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListenAlongSettings {
    pub device_id: Option<String>,
    pub sync_pause: bool,
    pub auto_resync: bool,
    pub drift_threshold_ms: i64,
    pub latency_offset_ms: i64,
    pub auto_stop_on_manual_change: bool,
}

impl Default for ListenAlongSettings {
    fn default() -> Self {
        Self {
            device_id: None,
            sync_pause: true,
            auto_resync: true,
            drift_threshold_ms: DEFAULT_DRIFT_THRESHOLD_MS,
            latency_offset_ms: DEFAULT_LATENCY_OFFSET_MS,
            auto_stop_on_manual_change: false,
        }
    }
}

impl ListenAlongSettings {
    pub fn normalize(&mut self) {
        self.device_id = self.device_id.take().and_then(non_empty_trimmed);
        self.drift_threshold_ms = self
            .drift_threshold_ms
            .clamp(MIN_DRIFT_THRESHOLD_MS, MAX_DRIFT_THRESHOLD_MS);
        self.latency_offset_ms = self
            .latency_offset_ms
            .clamp(MIN_LATENCY_OFFSET_MS, MAX_LATENCY_OFFSET_MS);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListenAlongState {
    pub active: bool,
    pub status: ListenAlongStatus,
    pub target: Option<ListenAlongTarget>,
    pub settings: ListenAlongSettings,
    pub resolved_device_id: Option<String>,
    pub started_at_ms: Option<i64>,
    pub last_synced_at_ms: Option<i64>,
    pub last_synced_track_uri: Option<String>,
    pub last_synced_position_ms: Option<i64>,
    pub sync_count: u64,
    pub drift_ms: Option<i64>,
    pub diverged: bool,
    pub last_error: Option<String>,
    pub last_error_code: Option<String>,
    pub updated_at_ms: i64,
}

impl Default for ListenAlongState {
    fn default() -> Self {
        Self {
            active: false,
            status: ListenAlongStatus::Idle,
            target: None,
            settings: ListenAlongSettings::default(),
            resolved_device_id: None,
            started_at_ms: None,
            last_synced_at_ms: None,
            last_synced_track_uri: None,
            last_synced_position_ms: None,
            sync_count: 0,
            drift_ms: None,
            diverged: false,
            last_error: None,
            last_error_code: None,
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StartListenAlongRequest {
    pub user_uri: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub sync_pause: Option<bool>,
    #[serde(default)]
    pub auto_resync: Option<bool>,
    #[serde(default)]
    pub drift_threshold_ms: Option<i64>,
    #[serde(default)]
    pub latency_offset_ms: Option<i64>,
    #[serde(default)]
    pub auto_stop_on_manual_change: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StopListenAlongRequest {
    #[serde(default)]
    pub pause_playback: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResyncListenAlongRequest {
    #[serde(default)]
    pub device_id: Option<String>,
}

fn deserialize_double_option<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateListenAlongSettingsRequest {
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub device_id: Option<Option<String>>,
    #[serde(default)]
    pub sync_pause: Option<bool>,
    #[serde(default)]
    pub auto_resync: Option<bool>,
    #[serde(default)]
    pub drift_threshold_ms: Option<i64>,
    #[serde(default)]
    pub latency_offset_ms: Option<i64>,
    #[serde(default)]
    pub auto_stop_on_manual_change: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenAlongErrorCode {
    InvalidInput,
    SelfTarget,
    NotActive,
    Unauthenticated,
    Offline,
    NoActiveDevice,
    CommandFailed,
}

impl ListenAlongErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::SelfTarget => "self_target",
            Self::NotActive => "not_active",
            Self::Unauthenticated => "unauthenticated",
            Self::Offline => "offline",
            Self::NoActiveDevice => "no_active_device",
            Self::CommandFailed => "command_failed",
        }
    }
}

impl fmt::Display for ListenAlongErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenAlongError {
    pub code: ListenAlongErrorCode,
    pub message: String,
}

impl ListenAlongError {
    pub fn new(code: ListenAlongErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ListenAlongError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ListenAlongError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTargetObservation {
    pub user_uri: String,
    pub user_name: String,
    pub user_image_url: Option<String>,
    pub playing: bool,
    pub timestamp_ms: i64,
    pub track_uri: Option<String>,
    pub track_name: Option<String>,
    pub all_artists: Option<String>,
    pub album_name: Option<String>,
    pub cover_url: Option<String>,
    pub duration_ms: Option<i64>,
    pub context_uri: Option<String>,
    pub context_name: Option<String>,
    pub context_type: Option<String>,
    pub context_is_private: bool,
    pub spotify_url: Option<String>,
}

impl From<LiveTargetObservation> for ListenAlongTarget {
    fn from(obs: LiveTargetObservation) -> Self {
        let user_uri =
            normalize_target_user_uri(&obs.user_uri).unwrap_or_else(|_| obs.user_uri.clone());
        let user_name = if obs.user_name.trim().is_empty() {
            bare_username(&user_uri).to_string()
        } else {
            obs.user_name
        };
        Self {
            user_uri,
            user_name,
            user_image_url: obs.user_image_url,
            playing: obs.playing,
            timestamp_ms: obs.timestamp_ms,
            track_uri: obs.track_uri.and_then(non_empty_trimmed),
            track_name: obs.track_name,
            all_artists: obs.all_artists,
            album_name: obs.album_name,
            cover_url: obs.cover_url,
            duration_ms: obs.duration_ms,
            context_uri: obs.context_uri.and_then(non_empty_trimmed),
            context_name: obs.context_name,
            context_type: obs.context_type,
            context_is_private: obs.context_is_private,
            spotify_url: obs.spotify_url,
        }
    }
}

impl From<&LiveTargetObservation> for ListenAlongTarget {
    fn from(obs: &LiveTargetObservation) -> Self {
        Self::from(obs.clone())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnPlayerObservation {
    pub connected: bool,
    pub active_device_id: Option<String>,
    pub has_playable_device: bool,
    pub is_playing: bool,
    pub is_paused: bool,
    pub track_uri: Option<String>,
    pub estimated_position_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenAlongPlayCommand {
    pub track_uri: String,
    pub context_uri: Option<String>,
    pub position_ms: i64,
    pub device_id: Option<String>,
}

pub trait ListenAlongPlayerAdapter: Send + Sync {
    fn play(&self, cmd: ListenAlongPlayCommand) -> BoxFuture<'_, Result<String, ListenAlongError>>;
    fn pause(&self, device_id: Option<&str>) -> BoxFuture<'_, Result<String, ListenAlongError>>;
    fn resume(&self, device_id: Option<&str>) -> BoxFuture<'_, Result<String, ListenAlongError>>;
    fn seek(
        &self,
        position_ms: i64,
        device_id: Option<&str>,
    ) -> BoxFuture<'_, Result<String, ListenAlongError>>;
    fn current_observation(&self, now_ms: i64) -> BoxFuture<'_, OwnPlayerObservation>;
}

fn non_empty_trimmed(s: String) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == s.len() {
        Some(s)
    } else {
        Some(trimmed.to_string())
    }
}

fn bare_username(user_uri: &str) -> &str {
    const PREFIX: &str = "spotify:user:";
    if user_uri.len() >= PREFIX.len() && user_uri[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        &user_uri[PREFIX.len()..]
    } else {
        user_uri
    }
}

fn same_user_uri(a: &str, b: &str) -> bool {
    let norm_a = normalize_target_user_uri(a).unwrap_or_else(|_| a.trim().to_string());
    let norm_b = normalize_target_user_uri(b).unwrap_or_else(|_| b.trim().to_string());
    norm_a.eq_ignore_ascii_case(&norm_b)
}

pub fn normalize_target_user_uri(raw: &str) -> Result<String, ListenAlongError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ListenAlongError::new(
            ListenAlongErrorCode::InvalidInput,
            "Target user URI cannot be empty",
        ));
    }

    const USER_PREFIX: &str = "spotify:user:";
    if trimmed.len() >= USER_PREFIX.len()
        && trimmed[..USER_PREFIX.len()].eq_ignore_ascii_case(USER_PREFIX)
    {
        let id = trimmed[USER_PREFIX.len()..].trim();
        if id.is_empty() || id.contains(':') || id.contains('/') || id.contains('?') {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::InvalidInput,
                format!("Invalid Spotify user URI: '{trimmed}'"),
            ));
        }
        return Ok(format!("spotify:user:{id}"));
    }

    if trimmed.len() >= "spotify:".len() && trimmed[.."spotify:".len()].eq_ignore_ascii_case("spotify:") {
        return Err(ListenAlongError::new(
            ListenAlongErrorCode::InvalidInput,
            format!("Expected a Spotify user URI, got '{trimmed}'"),
        ));
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        let after_scheme = if lower.starts_with("https://") {
            &trimmed["https://".len()..]
        } else {
            &trimmed["http://".len()..]
        };
        let without_query = after_scheme
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .trim_end_matches('/');

        let mut segments = without_query.split('/');
        let host = segments.next().unwrap_or_default();
        if !host.eq_ignore_ascii_case("open.spotify.com") {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::InvalidInput,
                format!("Unsupported user URL host: '{trimmed}'"),
            ));
        }

        let rest: Vec<&str> = segments.filter(|s| !s.is_empty()).collect();
        let user_id = match rest.as_slice() {
            [kind, id] if kind.eq_ignore_ascii_case("user") => Some(*id),
            [intl, kind, id]
                if intl.to_ascii_lowercase().starts_with("intl-")
                    && kind.eq_ignore_ascii_case("user") =>
            {
                Some(*id)
            }
            _ => None,
        };

        let Some(id) = user_id.map(str::trim).filter(|s| !s.is_empty()) else {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::InvalidInput,
                format!("Invalid Spotify user URL: '{trimmed}'"),
            ));
        };
        if id.contains(':') {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::InvalidInput,
                format!("Invalid Spotify user ID in URL: '{trimmed}'"),
            ));
        }
        return Ok(format!("spotify:user:{id}"));
    }

    if trimmed.contains(':') || trimmed.contains('/') || trimmed.contains('?') || trimmed.contains('#')
    {
        return Err(ListenAlongError::new(
            ListenAlongErrorCode::InvalidInput,
            format!("Invalid Spotify username or URI: '{trimmed}'"),
        ));
    }

    Ok(format!("spotify:user:{trimmed}"))
}

pub fn compute_target_position_ms(
    target_timestamp_ms: i64,
    duration_ms: Option<i64>,
    latency_offset_ms: i64,
    now_ms: i64,
) -> i64 {
    let elapsed = now_ms.saturating_sub(target_timestamp_ms).max(0);
    let mut pos = elapsed.saturating_add(latency_offset_ms).max(0);

    if let Some(dur) = duration_ms
        && dur > 2_000
    {
        if pos >= dur {
            if dur >= 30_000 && pos < dur.saturating_mul(10) {
                pos %= dur;
            } else {
                pos = 0;
            }
        }
        let max_pos = (dur - 1_500).max(0);
        pos = pos.clamp(0, max_pos);
    }

    pos
}

pub fn playable_context_uri(target: &ListenAlongTarget) -> Option<String> {
    if target.context_is_private {
        return None;
    }

    if let Some(ct) = target.context_type.as_deref() {
        let ct_trimmed = ct.trim();
        if ct_trimmed.eq_ignore_ascii_case("liked_songs")
            || ct_trimmed.eq_ignore_ascii_case("dj")
        {
            return None;
        }
    }

    let uri = target.context_uri.as_deref()?.trim();
    if uri.is_empty() {
        return None;
    }

    let uri_lc = uri.to_ascii_lowercase();
    let dj_id_lc = SPOTIFY_DJ_PLAYLIST_ID.to_ascii_lowercase();
    if uri_lc.contains(":collection")
        || uri_lc == "spotify:dj"
        || uri_lc.ends_with(":dj")
        || uri_lc.contains(":dj:")
        || uri_lc == format!("spotify:playlist:{dj_id_lc}")
        || uri_lc.ends_with(&format!(":playlist:{dj_id_lc}"))
        || uri_lc.starts_with("spotify:track:")
        || uri_lc.starts_with("spotify:episode:")
        || uri_lc.starts_with("spotify:local:")
    {
        return None;
    }

    Some(uri.to_string())
}

struct ManagerInner {
    state: ListenAlongState,
    target_username: Option<String>,
    last_command_at_ms: Option<i64>,
    last_auto_resync_at_ms: Option<i64>,
    last_host_track_uri: Option<String>,
    last_host_timestamp_ms: Option<i64>,
    last_host_playing: Option<bool>,
}

impl ManagerInner {
    fn new() -> Self {
        Self {
            state: ListenAlongState::default(),
            target_username: None,
            last_command_at_ms: None,
            last_auto_resync_at_ms: None,
            last_host_track_uri: None,
            last_host_timestamp_ms: None,
            last_host_playing: None,
        }
    }

    fn reset_to_idle(&mut self, now_ms: i64) {
        let preserved_settings = self.state.settings.clone();
        self.state = ListenAlongState {
            settings: preserved_settings,
            updated_at_ms: now_ms,
            ..ListenAlongState::default()
        };
        self.target_username = None;
        self.last_command_at_ms = None;
        self.last_auto_resync_at_ms = None;
        self.last_host_track_uri = None;
        self.last_host_timestamp_ms = None;
        self.last_host_playing = None;
    }
}

type ClockFn = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct ListenAlongManager {
    player: Arc<dyn ListenAlongPlayerAdapter>,
    inner: Mutex<ManagerInner>,
    state_watch_tx: watch::Sender<ListenAlongState>,
    state_broadcast_tx: broadcast::Sender<ListenAlongState>,
    extra_targets_tx: watch::Sender<BTreeSet<String>>,
    clock: ClockFn,
}

impl ListenAlongManager {
    pub fn new(player: Arc<dyn ListenAlongPlayerAdapter>) -> Self {
        Self::with_clock(player, Arc::new(|| Utc::now().timestamp_millis()))
    }

    pub fn with_clock(player: Arc<dyn ListenAlongPlayerAdapter>, clock: ClockFn) -> Self {
        let initial_state = ListenAlongState::default();
        let (state_watch_tx, _state_watch_rx) = watch::channel(initial_state);
        let (state_broadcast_tx, _state_broadcast_rx) =
            broadcast::channel(STATE_BROADCAST_CAPACITY);
        let (extra_targets_tx, _extra_targets_rx) = watch::channel(BTreeSet::new());

        Self {
            player,
            inner: Mutex::new(ManagerInner::new()),
            state_watch_tx,
            state_broadcast_tx,
            extra_targets_tx,
            clock,
        }
    }

    pub async fn state(&self) -> ListenAlongState {
        self.state_watch_tx.borrow().clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ListenAlongState> {
        self.state_broadcast_tx.subscribe()
    }

    pub fn subscribe_extra_targets(&self) -> watch::Receiver<BTreeSet<String>> {
        self.extra_targets_tx.subscribe()
    }

    fn publish_state(&self, state: &ListenAlongState) {
        self.state_watch_tx.send_replace(state.clone());
        let _ = self.state_broadcast_tx.send(state.clone());
    }

    fn set_extra_target(&self, username: Option<&str>) {
        let next = match username.map(str::trim).filter(|s| !s.is_empty()) {
            Some(u) => BTreeSet::from([u.to_string()]),
            None => BTreeSet::new(),
        };
        self.extra_targets_tx.send_if_modified(|current| {
            if *current != next {
                *current = next;
                true
            } else {
                false
            }
        });
    }

    async fn sync_play_to_target(&self, inner: &mut ManagerInner, now_ms: i64) {
        let Some(target) = inner.state.target.clone() else {
            inner.state.status = ListenAlongStatus::Waiting;
            inner.state.drift_ms = None;
            inner.state.updated_at_ms = now_ms;
            self.publish_state(&inner.state);
            return;
        };

        let Some(track_uri) = target.track_uri.clone().filter(|s| !s.trim().is_empty()) else {
            inner.state.status = ListenAlongStatus::Waiting;
            inner.state.drift_ms = None;
            inner.state.updated_at_ms = now_ms;
            self.publish_state(&inner.state);
            return;
        };

        inner.state.status = ListenAlongStatus::Syncing;
        inner.state.diverged = false;
        inner.state.last_error = None;
        inner.state.last_error_code = None;
        inner.state.updated_at_ms = now_ms;
        self.publish_state(&inner.state);

        let position_ms = compute_target_position_ms(
            target.timestamp_ms,
            target.duration_ms,
            inner.state.settings.latency_offset_ms,
            now_ms,
        );
        let context_uri = playable_context_uri(&target);
        let cmd = ListenAlongPlayCommand {
            track_uri: track_uri.clone(),
            context_uri,
            position_ms,
            device_id: inner.state.settings.device_id.clone(),
        };

        match self.player.play(cmd).await {
            Ok(resolved_device_id) => {
                inner.state.status = ListenAlongStatus::Synced;
                inner.state.resolved_device_id = non_empty_trimmed(resolved_device_id);
                inner.state.last_synced_at_ms = Some(now_ms);
                inner.state.last_synced_track_uri = Some(track_uri);
                inner.state.last_synced_position_ms = Some(position_ms);
                inner.state.sync_count = inner.state.sync_count.saturating_add(1);
                inner.state.drift_ms = Some(0);
                inner.state.diverged = false;
                inner.state.last_error = None;
                inner.state.last_error_code = None;
                inner.state.updated_at_ms = now_ms;
                inner.last_command_at_ms = Some(now_ms);
            }
            Err(err) => {
                inner.state.status = ListenAlongStatus::Error;
                inner.state.drift_ms = None;
                inner.state.last_error = Some(err.message);
                inner.state.last_error_code = Some(err.code.as_str().to_string());
                inner.state.updated_at_ms = now_ms;
            }
        }

        self.publish_state(&inner.state);
    }

    pub async fn start(
        &self,
        req: StartListenAlongRequest,
        own_user_uri: Option<&str>,
        current_target: Option<LiveTargetObservation>,
        offline: bool,
        authenticated: bool,
    ) -> Result<ListenAlongState, ListenAlongError> {
        if offline {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::Offline,
                "Listen Along is unavailable in offline mode",
            ));
        }
        if !authenticated {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::Unauthenticated,
                "Spotify authentication is required for Listen Along",
            ));
        }

        let target_uri = normalize_target_user_uri(&req.user_uri)?;
        if let Some(own) = own_user_uri
            && !own.trim().is_empty()
            && same_user_uri(&target_uri, own)
        {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::SelfTarget,
                "Cannot listen along with your own account",
            ));
        }

        let mut inner = self.inner.lock().await;
        let now_ms = (self.clock)();

        if let Some(dev) = req.device_id {
            inner.state.settings.device_id = non_empty_trimmed(dev);
        }
        if let Some(sync_pause) = req.sync_pause {
            inner.state.settings.sync_pause = sync_pause;
        }
        if let Some(auto_resync) = req.auto_resync {
            inner.state.settings.auto_resync = auto_resync;
        }
        if let Some(drift_threshold_ms) = req.drift_threshold_ms {
            inner.state.settings.drift_threshold_ms = drift_threshold_ms;
        }
        if let Some(latency_offset_ms) = req.latency_offset_ms {
            inner.state.settings.latency_offset_ms = latency_offset_ms;
        }
        if let Some(auto_stop) = req.auto_stop_on_manual_change {
            inner.state.settings.auto_stop_on_manual_change = auto_stop;
        }
        inner.state.settings.normalize();

        let username = bare_username(&target_uri).to_string();
        let matched_target = current_target
            .filter(|obs| same_user_uri(&obs.user_uri, &target_uri))
            .map(ListenAlongTarget::from);

        let initial_target = matched_target.clone().unwrap_or_else(|| ListenAlongTarget {
            user_uri: target_uri.clone(),
            user_name: username.clone(),
            user_image_url: None,
            playing: false,
            timestamp_ms: 0,
            track_uri: None,
            track_name: None,
            all_artists: None,
            album_name: None,
            cover_url: None,
            duration_ms: None,
            context_uri: None,
            context_name: None,
            context_type: None,
            context_is_private: false,
            spotify_url: None,
        });

        inner.target_username = Some(username.clone());
        inner.last_command_at_ms = None;
        inner.last_auto_resync_at_ms = None;
        inner.last_host_track_uri = matched_target.as_ref().and_then(|t| t.track_uri.clone());
        inner.last_host_timestamp_ms = matched_target.as_ref().map(|t| t.timestamp_ms);
        inner.last_host_playing = matched_target.as_ref().map(|t| t.playing);

        let preserved_settings = inner.state.settings.clone();
        inner.state = ListenAlongState {
            active: true,
            status: ListenAlongStatus::Connecting,
            target: Some(initial_target),
            settings: preserved_settings,
            resolved_device_id: None,
            started_at_ms: Some(now_ms),
            last_synced_at_ms: None,
            last_synced_track_uri: None,
            last_synced_position_ms: None,
            sync_count: 0,
            drift_ms: None,
            diverged: false,
            last_error: None,
            last_error_code: None,
            updated_at_ms: now_ms,
        };

        self.set_extra_target(Some(&username));

        let should_sync_now = inner
            .state
            .target
            .as_ref()
            .is_some_and(|t| t.playing && t.track_uri.is_some());

        if should_sync_now {
            self.sync_play_to_target(&mut inner, now_ms).await;
        } else {
            inner.state.status = ListenAlongStatus::Waiting;
            inner.state.updated_at_ms = now_ms;
            self.publish_state(&inner.state);
        }

        Ok(inner.state.clone())
    }

    pub async fn stop(&self, req: StopListenAlongRequest) -> ListenAlongState {
        let mut inner = self.inner.lock().await;
        let now_ms = (self.clock)();

        let should_pause = req.pause_playback == Some(true)
            && inner.state.active
            && !inner.state.diverged
            && matches!(
                inner.state.status,
                ListenAlongStatus::Synced | ListenAlongStatus::Syncing
            );

        if should_pause {
            let device_id = inner
                .state
                .resolved_device_id
                .clone()
                .or_else(|| inner.state.settings.device_id.clone());
            let _ = self.player.pause(device_id.as_deref()).await;
        }

        self.set_extra_target(None);
        inner.reset_to_idle(now_ms);
        self.publish_state(&inner.state);
        inner.state.clone()
    }

    pub async fn resync(
        &self,
        req: ResyncListenAlongRequest,
        current_target: Option<LiveTargetObservation>,
    ) -> Result<ListenAlongState, ListenAlongError> {
        let mut inner = self.inner.lock().await;
        if !inner.state.active {
            return Err(ListenAlongError::new(
                ListenAlongErrorCode::NotActive,
                "Listen Along is not active",
            ));
        }

        let now_ms = (self.clock)();

        if let Some(dev) = req.device_id {
            inner.state.settings.device_id = non_empty_trimmed(dev);
            inner.state.settings.normalize();
        }

        if let Some(obs) = current_target
            && inner
                .state
                .target
                .as_ref()
                .is_none_or(|t| same_user_uri(&t.user_uri, &obs.user_uri))
        {
            let updated = ListenAlongTarget::from(obs);
            inner.last_host_track_uri = updated.track_uri.clone();
            inner.last_host_timestamp_ms = Some(updated.timestamp_ms);
            inner.last_host_playing = Some(updated.playing);
            inner.state.target = Some(updated);
        }

        inner.state.diverged = false;
        inner.state.last_error = None;
        inner.state.last_error_code = None;

        let Some(target) = inner.state.target.clone() else {
            inner.state.status = ListenAlongStatus::Waiting;
            inner.state.drift_ms = None;
            inner.state.updated_at_ms = now_ms;
            self.publish_state(&inner.state);
            return Ok(inner.state.clone());
        };

        if !target.playing || target.track_uri.is_none() {
            inner.state.status = if target.track_uri.is_some()
                && inner.state.last_synced_track_uri.is_some()
            {
                ListenAlongStatus::HostPaused
            } else {
                ListenAlongStatus::Waiting
            };
            inner.state.drift_ms = None;
            inner.state.updated_at_ms = now_ms;
            self.publish_state(&inner.state);
            return Ok(inner.state.clone());
        }

        let track_uri = target.track_uri.clone().unwrap();
        let target_pos = compute_target_position_ms(
            target.timestamp_ms,
            target.duration_ms,
            inner.state.settings.latency_offset_ms,
            now_ms,
        );

        let own_obs = self.player.current_observation(now_ms).await;
        let same_device = match inner.state.settings.device_id.as_deref() {
            Some(wanted) => own_obs.active_device_id.as_deref() == Some(wanted),
            None => true,
        };
        let can_seek_in_place = own_obs.connected
            && own_obs.is_playing
            && same_device
            && own_obs.track_uri.as_deref() == Some(track_uri.as_str());

        if can_seek_in_place {
            let seek_device = inner
                .state
                .settings
                .device_id
                .clone()
                .or_else(|| own_obs.active_device_id.clone());
            match self.player.seek(target_pos, seek_device.as_deref()).await {
                Ok(resolved_device_id) => {
                    inner.state.status = ListenAlongStatus::Synced;
                    inner.state.resolved_device_id = non_empty_trimmed(resolved_device_id);
                    inner.state.last_synced_at_ms = Some(now_ms);
                    inner.state.last_synced_track_uri = Some(track_uri);
                    inner.state.last_synced_position_ms = Some(target_pos);
                    inner.state.sync_count = inner.state.sync_count.saturating_add(1);
                    inner.state.drift_ms = Some(0);
                    inner.state.diverged = false;
                    inner.state.last_error = None;
                    inner.state.last_error_code = None;
                    inner.state.updated_at_ms = now_ms;
                    inner.last_command_at_ms = Some(now_ms);
                    self.publish_state(&inner.state);
                    return Ok(inner.state.clone());
                }
                Err(err) if err.code == ListenAlongErrorCode::NoActiveDevice => {
                    inner.state.status = ListenAlongStatus::Error;
                    inner.state.drift_ms = None;
                    inner.state.last_error = Some(err.message);
                    inner.state.last_error_code = Some(err.code.as_str().to_string());
                    inner.state.updated_at_ms = now_ms;
                    self.publish_state(&inner.state);
                    return Ok(inner.state.clone());
                }
                Err(_) => {
                    // Fall through to full `play(...)` sync below.
                }
            }
        }

        self.sync_play_to_target(&mut inner, now_ms).await;
        Ok(inner.state.clone())
    }

    pub async fn update_settings(
        &self,
        req: UpdateListenAlongSettingsRequest,
    ) -> Result<ListenAlongState, ListenAlongError> {
        let mut inner = self.inner.lock().await;
        let now_ms = (self.clock)();

        if let Some(dev_opt) = req.device_id {
            inner.state.settings.device_id = dev_opt.and_then(non_empty_trimmed);
        }
        if let Some(sync_pause) = req.sync_pause {
            inner.state.settings.sync_pause = sync_pause;
        }
        if let Some(auto_resync) = req.auto_resync {
            inner.state.settings.auto_resync = auto_resync;
        }
        if let Some(drift_threshold_ms) = req.drift_threshold_ms {
            inner.state.settings.drift_threshold_ms = drift_threshold_ms;
        }
        if let Some(latency_offset_ms) = req.latency_offset_ms {
            inner.state.settings.latency_offset_ms = latency_offset_ms;
        }
        if let Some(auto_stop) = req.auto_stop_on_manual_change {
            inner.state.settings.auto_stop_on_manual_change = auto_stop;
        }
        inner.state.settings.normalize();
        inner.state.updated_at_ms = now_ms;
        self.publish_state(&inner.state);

        Ok(inner.state.clone())
    }

    pub async fn on_live_update(&self, friends: &[LiveTargetObservation], now_ms: i64) {
        let mut inner = self.inner.lock().await;
        if !inner.state.active {
            return;
        }

        let Some(active_user_uri) = inner.state.target.as_ref().map(|t| t.user_uri.clone()) else {
            return;
        };

        let Some(matched) = friends
            .iter()
            .find(|item| same_user_uri(&item.user_uri, &active_user_uri))
        else {
            return;
        };

        let prev_target = inner.state.target.clone();
        let prev_track_uri = inner.last_host_track_uri.clone();
        let prev_timestamp_ms = inner.last_host_timestamp_ms;
        let prev_playing = inner.last_host_playing;
        let prev_status = inner.state.status;

        let new_target = ListenAlongTarget::from(matched);
        let new_track_uri = new_target.track_uri.clone();
        let new_timestamp_ms = new_target.timestamp_ms;
        let new_playing = new_target.playing;

        inner.state.target = Some(new_target.clone());
        inner.last_host_track_uri = new_track_uri.clone();
        inner.last_host_timestamp_ms = Some(new_timestamp_ms);
        inner.last_host_playing = Some(new_playing);

        let track_changed = new_track_uri.is_some() && new_track_uri != prev_track_uri;
        let replayed_same_track = new_playing
            && new_track_uri.is_some()
            && new_track_uri == prev_track_uri
            && prev_timestamp_ms.is_some_and(|prev_ts| {
                new_timestamp_ms.saturating_sub(prev_ts) > HOST_REPLAY_JUMP_THRESHOLD_MS
            });
        let host_paused = prev_playing == Some(true) && !new_playing;
        let host_resumed = prev_playing == Some(false) && new_playing;
        let waiting_or_no_device = matches!(
            prev_status,
            ListenAlongStatus::Waiting | ListenAlongStatus::Connecting
        ) || (prev_status == ListenAlongStatus::Error
            && inner.state.last_error_code.as_deref()
                == Some(ListenAlongErrorCode::NoActiveDevice.as_str()));

        // 1 & 2: Host started a new track or replayed the same track.
        if new_playing && new_track_uri.is_some() && (track_changed || replayed_same_track) {
            inner.state.diverged = false;
            self.sync_play_to_target(&mut inner, now_ms).await;
            return;
        }

        // 3: Host paused playback.
        if host_paused {
            inner.state.status = ListenAlongStatus::HostPaused;
            inner.state.drift_ms = None;
            inner.state.updated_at_ms = now_ms;

            if inner.state.settings.sync_pause && !inner.state.diverged {
                let own_obs = self.player.current_observation(now_ms).await;
                let playing_synced_track = own_obs.is_playing
                    && own_obs.track_uri.is_some()
                    && own_obs.track_uri == inner.state.last_synced_track_uri;
                if playing_synced_track {
                    let device_id = inner
                        .state
                        .resolved_device_id
                        .clone()
                        .or_else(|| inner.state.settings.device_id.clone());
                    if let Ok(resolved) = self.player.pause(device_id.as_deref()).await {
                        inner.state.resolved_device_id = non_empty_trimmed(resolved);
                        inner.last_command_at_ms = Some(now_ms);
                    }
                }
            }

            self.publish_state(&inner.state);
            return;
        }

        // 4: Host resumed playback.
        if host_resumed && let Some(ref track_uri) = new_track_uri {
            inner.state.diverged = false;
            let target_pos = compute_target_position_ms(
                new_target.timestamp_ms,
                new_target.duration_ms,
                inner.state.settings.latency_offset_ms,
                now_ms,
            );
            let own_obs = self.player.current_observation(now_ms).await;
            let drift_if_resumed = own_obs.estimated_position_ms - target_pos;
            let can_resume_in_place = own_obs.is_paused
                && !own_obs.is_playing
                && own_obs.track_uri.as_deref() == Some(track_uri.as_str())
                && drift_if_resumed.abs() <= inner.state.settings.drift_threshold_ms;

            if can_resume_in_place {
                let device_id = inner
                    .state
                    .resolved_device_id
                    .clone()
                    .or_else(|| inner.state.settings.device_id.clone());
                if let Ok(resolved) = self.player.resume(device_id.as_deref()).await {
                    inner.state.status = ListenAlongStatus::Synced;
                    inner.state.resolved_device_id = non_empty_trimmed(resolved);
                    inner.state.last_synced_at_ms = Some(now_ms);
                    inner.state.last_synced_track_uri = Some(track_uri.clone());
                    inner.state.last_synced_position_ms = Some(own_obs.estimated_position_ms);
                    inner.state.sync_count = inner.state.sync_count.saturating_add(1);
                    inner.state.drift_ms = Some(drift_if_resumed);
                    inner.state.diverged = false;
                    inner.state.last_error = None;
                    inner.state.last_error_code = None;
                    inner.state.updated_at_ms = now_ms;
                    inner.last_command_at_ms = Some(now_ms);
                    self.publish_state(&inner.state);
                    return;
                }
            }

            self.sync_play_to_target(&mut inner, now_ms).await;
            return;
        }

        // 5: Was waiting or recovering from no_active_device and host is playing.
        if waiting_or_no_device && new_playing && new_track_uri.is_some() {
            self.sync_play_to_target(&mut inner, now_ms).await;
            return;
        }

        // Target metadata changed without playback transition (e.g., cover art/title enriched).
        if prev_target.as_ref() != Some(&new_target) {
            inner.state.updated_at_ms = now_ms;
            self.publish_state(&inner.state);
        }
    }

    pub async fn on_player_update(&self, obs: OwnPlayerObservation, now_ms: i64) {
        let mut inner = self.inner.lock().await;
        if !inner.state.active {
            return;
        }

        // Auto-recover from `no_active_device` as soon as a playable device appears.
        if inner.state.status == ListenAlongStatus::Error
            && inner.state.last_error_code.as_deref()
                == Some(ListenAlongErrorCode::NoActiveDevice.as_str())
            && obs.has_playable_device
        {
            if inner
                .state
                .target
                .as_ref()
                .is_some_and(|t| t.playing && t.track_uri.is_some())
            {
                self.sync_play_to_target(&mut inner, now_ms).await;
            }
            return;
        }

        // Ignore manual-change and drift checks within grace window after our own command.
        if let Some(last_cmd) = inner.last_command_at_ms
            && now_ms.saturating_sub(last_cmd) < COMMAND_GRACE_WINDOW_MS
        {
            return;
        }

        if inner.state.status != ListenAlongStatus::Synced {
            return;
        }

        let Some(target) = inner.state.target.clone() else {
            return;
        };
        if !target.playing || target.track_uri.is_none() {
            return;
        }

        // Detect manual track switch by the user.
        if obs.connected
            && obs.track_uri.is_some()
            && obs.track_uri != inner.state.last_synced_track_uri
        {
            if inner.state.settings.auto_stop_on_manual_change {
                self.set_extra_target(None);
                inner.reset_to_idle(now_ms);
                self.publish_state(&inner.state);
            } else {
                inner.state.diverged = true;
                inner.state.status = ListenAlongStatus::Diverged;
                inner.state.drift_ms = None;
                inner.state.updated_at_ms = now_ms;
                self.publish_state(&inner.state);
            }
            return;
        }

        // Same track and actively playing -> track drift and optionally auto-resync.
        if obs.track_uri == inner.state.last_synced_track_uri && obs.is_playing {
            let target_pos = compute_target_position_ms(
                target.timestamp_ms,
                target.duration_ms,
                inner.state.settings.latency_offset_ms,
                now_ms,
            );
            let drift_ms = obs.estimated_position_ms - target_pos;
            let drift_changed = inner.state.drift_ms != Some(drift_ms);
            inner.state.drift_ms = Some(drift_ms);

            let cooldown_elapsed = inner
                .last_auto_resync_at_ms
                .is_none_or(|last| now_ms.saturating_sub(last) >= AUTO_RESYNC_COOLDOWN_MS);

            if inner.state.settings.auto_resync
                && drift_ms.abs() > inner.state.settings.drift_threshold_ms
                && cooldown_elapsed
            {
                let device_id = inner
                    .state
                    .resolved_device_id
                    .clone()
                    .or_else(|| inner.state.settings.device_id.clone());
                match self.player.seek(target_pos, device_id.as_deref()).await {
                    Ok(resolved) => {
                        inner.state.resolved_device_id = non_empty_trimmed(resolved);
                        inner.state.last_synced_at_ms = Some(now_ms);
                        inner.state.last_synced_position_ms = Some(target_pos);
                        inner.state.sync_count = inner.state.sync_count.saturating_add(1);
                        inner.state.drift_ms = Some(0);
                        inner.state.last_error = None;
                        inner.state.last_error_code = None;
                        inner.state.updated_at_ms = now_ms;
                        inner.last_command_at_ms = Some(now_ms);
                        inner.last_auto_resync_at_ms = Some(now_ms);
                        self.publish_state(&inner.state);
                    }
                    Err(err) => {
                        inner.last_auto_resync_at_ms = Some(now_ms);
                        if err.code == ListenAlongErrorCode::NoActiveDevice {
                            inner.state.status = ListenAlongStatus::Error;
                            inner.state.drift_ms = None;
                            inner.state.last_error = Some(err.message);
                            inner.state.last_error_code = Some(err.code.as_str().to_string());
                        }
                        inner.state.updated_at_ms = now_ms;
                        self.publish_state(&inner.state);
                    }
                }
            } else if drift_changed {
                inner.state.updated_at_ms = now_ms;
                self.publish_state(&inner.state);
            }
        }
    }

    pub async fn handle_disconnect_or_logout(&self) {
        let mut inner = self.inner.lock().await;
        self.set_extra_target(None);
        if inner.state.active {
            let now_ms = (self.clock)();
            inner.reset_to_idle(now_ms);
            self.publish_state(&inner.state);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum RecordedCommand {
        Play(ListenAlongPlayCommand),
        Pause(Option<String>),
        Resume(Option<String>),
        Seek {
            position_ms: i64,
            device_id: Option<String>,
        },
    }

    struct MockPlayerState {
        commands: Vec<RecordedCommand>,
        observation: OwnPlayerObservation,
        next_play_result: Option<Result<String, ListenAlongError>>,
        next_seek_result: Option<Result<String, ListenAlongError>>,
        next_pause_result: Option<Result<String, ListenAlongError>>,
        next_resume_result: Option<Result<String, ListenAlongError>>,
        default_device_id: String,
    }

    impl Default for MockPlayerState {
        fn default() -> Self {
            Self {
                commands: Vec::new(),
                observation: OwnPlayerObservation {
                    connected: true,
                    active_device_id: Some("dev-default".to_string()),
                    has_playable_device: true,
                    is_playing: false,
                    is_paused: false,
                    track_uri: None,
                    estimated_position_ms: 0,
                },
                next_play_result: None,
                next_seek_result: None,
                next_pause_result: None,
                next_resume_result: None,
                default_device_id: "dev-default".to_string(),
            }
        }
    }

    struct MockPlayer {
        state: StdMutex<MockPlayerState>,
    }

    impl MockPlayer {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                state: StdMutex::new(MockPlayerState::default()),
            })
        }

        fn commands(&self) -> Vec<RecordedCommand> {
            self.state.lock().unwrap().commands.clone()
        }

        fn clear_commands(&self) {
            self.state.lock().unwrap().commands.clear();
        }

        fn set_observation(&self, obs: OwnPlayerObservation) {
            self.state.lock().unwrap().observation = obs;
        }

        fn set_next_play_error(&self, err: ListenAlongError) {
            self.state.lock().unwrap().next_play_result = Some(Err(err));
        }
    }

    impl ListenAlongPlayerAdapter for MockPlayer {
        fn play(
            &self,
            cmd: ListenAlongPlayCommand,
        ) -> BoxFuture<'_, Result<String, ListenAlongError>> {
            let res = {
                let mut st = self.state.lock().unwrap();
                st.commands.push(RecordedCommand::Play(cmd.clone()));
                if let Some(custom) = st.next_play_result.take() {
                    custom
                } else {
                    let dev = cmd
                        .device_id
                        .clone()
                        .unwrap_or_else(|| st.default_device_id.clone());
                    st.observation.connected = true;
                    st.observation.has_playable_device = true;
                    st.observation.active_device_id = Some(dev.clone());
                    st.observation.is_playing = true;
                    st.observation.is_paused = false;
                    st.observation.track_uri = Some(cmd.track_uri);
                    st.observation.estimated_position_ms = cmd.position_ms;
                    Ok(dev)
                }
            };
            Box::pin(async move { res })
        }

        fn pause(&self, device_id: Option<&str>) -> BoxFuture<'_, Result<String, ListenAlongError>> {
            let res = {
                let mut st = self.state.lock().unwrap();
                st.commands
                    .push(RecordedCommand::Pause(device_id.map(str::to_string)));
                if let Some(custom) = st.next_pause_result.take() {
                    custom
                } else {
                    let dev = device_id
                        .map(str::to_string)
                        .unwrap_or_else(|| st.default_device_id.clone());
                    st.observation.is_playing = false;
                    st.observation.is_paused = true;
                    Ok(dev)
                }
            };
            Box::pin(async move { res })
        }

        fn resume(
            &self,
            device_id: Option<&str>,
        ) -> BoxFuture<'_, Result<String, ListenAlongError>> {
            let res = {
                let mut st = self.state.lock().unwrap();
                st.commands
                    .push(RecordedCommand::Resume(device_id.map(str::to_string)));
                if let Some(custom) = st.next_resume_result.take() {
                    custom
                } else {
                    let dev = device_id
                        .map(str::to_string)
                        .unwrap_or_else(|| st.default_device_id.clone());
                    st.observation.is_playing = true;
                    st.observation.is_paused = false;
                    Ok(dev)
                }
            };
            Box::pin(async move { res })
        }

        fn seek(
            &self,
            position_ms: i64,
            device_id: Option<&str>,
        ) -> BoxFuture<'_, Result<String, ListenAlongError>> {
            let res = {
                let mut st = self.state.lock().unwrap();
                st.commands.push(RecordedCommand::Seek {
                    position_ms,
                    device_id: device_id.map(str::to_string),
                });
                if let Some(custom) = st.next_seek_result.take() {
                    custom
                } else {
                    let dev = device_id
                        .map(str::to_string)
                        .unwrap_or_else(|| st.default_device_id.clone());
                    st.observation.estimated_position_ms = position_ms;
                    Ok(dev)
                }
            };
            Box::pin(async move { res })
        }

        fn current_observation(&self, _now_ms: i64) -> BoxFuture<'_, OwnPlayerObservation> {
            let obs = self.state.lock().unwrap().observation.clone();
            Box::pin(async move { obs })
        }
    }

    fn make_manager(now_ms: i64) -> (ListenAlongManager, Arc<MockPlayer>, Arc<AtomicI64>) {
        let player = MockPlayer::new();
        let clock_val = Arc::new(AtomicI64::new(now_ms));
        let clock_ref = Arc::clone(&clock_val);
        let mgr = ListenAlongManager::with_clock(
            player.clone(),
            Arc::new(move || clock_ref.load(Ordering::SeqCst)),
        );
        (mgr, player, clock_val)
    }

    fn sample_target(
        user_uri: &str,
        track_uri: &str,
        playing: bool,
        timestamp_ms: i64,
        context_uri: Option<&str>,
        context_type: Option<&str>,
        context_is_private: bool,
    ) -> LiveTargetObservation {
        LiveTargetObservation {
            user_uri: user_uri.to_string(),
            user_name: "Alice".to_string(),
            user_image_url: Some("https://img/alice".to_string()),
            playing,
            timestamp_ms,
            track_uri: Some(track_uri.to_string()),
            track_name: Some("Song Title".to_string()),
            all_artists: Some("Artist Name".to_string()),
            album_name: Some("Album Name".to_string()),
            cover_url: Some("https://img/cover".to_string()),
            duration_ms: Some(200_000),
            context_uri: context_uri.map(str::to_string),
            context_name: Some("Context".to_string()),
            context_type: context_type.map(str::to_string),
            context_is_private,
            spotify_url: Some("https://open.spotify.com/track/1".to_string()),
        }
    }

    #[test]
    fn normalize_target_user_uri_accepts_uri_url_bare_and_rejects_invalid() {
        assert_eq!(
            normalize_target_user_uri("spotify:user:alice").unwrap(),
            "spotify:user:alice"
        );
        assert_eq!(
            normalize_target_user_uri("  SPOTIFY:USER:Bob_99  ").unwrap(),
            "spotify:user:Bob_99"
        );
        assert_eq!(
            normalize_target_user_uri("https://open.spotify.com/user/carol?si=abc123").unwrap(),
            "spotify:user:carol"
        );
        assert_eq!(
            normalize_target_user_uri("http://open.spotify.com/user/dave/").unwrap(),
            "spotify:user:dave"
        );
        assert_eq!(
            normalize_target_user_uri("https://open.spotify.com/intl-de/user/erin/").unwrap(),
            "spotify:user:erin"
        );
        assert_eq!(
            normalize_target_user_uri("frank").unwrap(),
            "spotify:user:frank"
        );

        for invalid in [
            "",
            "   ",
            "spotify:user:",
            "spotify:track:12345",
            "https://open.spotify.com/track/12345",
            "https://example.com/user/alice",
        ] {
            let err = normalize_target_user_uri(invalid).unwrap_err();
            assert_eq!(err.code, ListenAlongErrorCode::InvalidInput, "for {invalid}");
        }
    }

    #[test]
    fn compute_target_position_handles_latency_looping_and_eof_clamping() {
        // Basic elapsed + latency offset
        assert_eq!(
            compute_target_position_ms(100_000, Some(200_000), 500, 110_000),
            10_500
        );
        // Negative clock skew clamps elapsed to 0 before adding latency
        assert_eq!(
            compute_target_position_ms(105_000, Some(200_000), 500, 100_000),
            500
        );
        // Negative latency offset clamps at 0
        assert_eq!(
            compute_target_position_ms(100_000, Some(200_000), -2_000, 100_500),
            0
        );
        // Near EOF clamped to dur - 1500
        assert_eq!(
            compute_target_position_ms(100_000, Some(200_000), 0, 299_500),
            198_500
        );
        // Looping past duration (< 10x dur, dur >= 30s) wraps cleanly
        assert_eq!(
            compute_target_position_ms(100_000, Some(200_000), 0, 315_000),
            15_000
        );
        // Way past duration (>= 10x dur) restarts at 0
        assert_eq!(
            compute_target_position_ms(100_000, Some(200_000), 0, 2_500_000),
            0
        );
        // Short track (< 30s) past duration restarts at 0
        assert_eq!(
            compute_target_position_ms(100_000, Some(20_000), 0, 125_000),
            0
        );
    }

    #[test]
    fn update_settings_request_deserializes_double_option_device_id() {
        let omitted: UpdateListenAlongSettingsRequest =
            serde_json::from_str(r#"{"sync_pause":false}"#).unwrap();
        assert_eq!(omitted.device_id, None);
        assert_eq!(omitted.sync_pause, Some(false));

        let cleared: UpdateListenAlongSettingsRequest =
            serde_json::from_str(r#"{"device_id":null}"#).unwrap();
        assert_eq!(cleared.device_id, Some(None));

        let set: UpdateListenAlongSettingsRequest =
            serde_json::from_str(r#"{"device_id":"dev-42"}"#).unwrap();
        assert_eq!(set.device_id, Some(Some("dev-42".to_string())));
    }

    #[tokio::test]
    async fn start_rejects_self_target_offline_and_unauthenticated() {
        let (mgr, _player, _clock) = make_manager(1_000_000);

        let err = mgr
            .start(
                StartListenAlongRequest {
                    user_uri: "alice".to_string(),
                    ..Default::default()
                },
                Some("spotify:user:me"),
                None,
                true,
                true,
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, ListenAlongErrorCode::Offline);

        let err = mgr
            .start(
                StartListenAlongRequest {
                    user_uri: "alice".to_string(),
                    ..Default::default()
                },
                Some("spotify:user:me"),
                None,
                false,
                false,
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, ListenAlongErrorCode::Unauthenticated);

        let err = mgr
            .start(
                StartListenAlongRequest {
                    user_uri: "https://open.spotify.com/user/Me?si=1".to_string(),
                    ..Default::default()
                },
                Some("spotify:user:me"),
                None,
                false,
                true,
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, ListenAlongErrorCode::SelfTarget);
    }

    #[tokio::test]
    async fn start_syncs_immediately_with_latency_clamping_and_extra_target_watch() {
        let now = 1_000_000;
        let (mgr, player, _clock) = make_manager(now);
        let mut extra_rx = mgr.subscribe_extra_targets();
        assert!(extra_rx.borrow().is_empty());

        let target = sample_target(
            "spotify:user:alice",
            "spotify:track:t1",
            true,
            now - 12_000,
            Some("spotify:playlist:pub123"),
            Some("playlist"),
            false,
        );

        let state = mgr
            .start(
                StartListenAlongRequest {
                    user_uri: "alice".to_string(),
                    latency_offset_ms: Some(750),
                    ..Default::default()
                },
                Some("spotify:user:me"),
                Some(target),
                false,
                true,
            )
            .await
            .unwrap();

        assert!(state.active);
        assert_eq!(state.status, ListenAlongStatus::Synced);
        assert_eq!(state.sync_count, 1);
        assert_eq!(state.last_synced_track_uri.as_deref(), Some("spotify:track:t1"));
        assert_eq!(state.last_synced_position_ms, Some(12_750));
        assert_eq!(state.resolved_device_id.as_deref(), Some("dev-default"));

        assert_eq!(
            *extra_rx.borrow_and_update(),
            BTreeSet::from(["alice".to_string()])
        );

        assert_eq!(
            player.commands(),
            vec![RecordedCommand::Play(ListenAlongPlayCommand {
                track_uri: "spotify:track:t1".to_string(),
                context_uri: Some("spotify:playlist:pub123".to_string()),
                position_ms: 12_750,
                device_id: None,
            })]
        );

        // Stop clears extra targets and optionally pauses playback.
        let stopped = mgr
            .stop(StopListenAlongRequest {
                pause_playback: Some(true),
            })
            .await;
        assert!(!stopped.active);
        assert_eq!(stopped.status, ListenAlongStatus::Idle);
        assert!(extra_rx.borrow_and_update().is_empty());
        assert!(matches!(
            player.commands().last(),
            Some(RecordedCommand::Pause(Some(dev))) if dev == "dev-default"
        ));
    }

    #[tokio::test]
    async fn private_liked_songs_and_dj_contexts_are_never_passed_to_play() {
        let now = 2_000_000;
        let (mgr, player, _clock) = make_manager(now);

        let cases = [
            (
                Some("spotify:user:alice:collection"),
                Some("liked_songs"),
                true,
            ),
            (Some("spotify:user:alice:dj"), Some("dj"), false),
            (
                Some("spotify:playlist:37i9dQZF1EYkqdzj48dyYq"),
                Some("playlist"),
                false,
            ),
            (Some("spotify:playlist:secret123"), Some("playlist"), true),
        ];

        for (idx, (ctx_uri, ctx_type, is_priv)) in cases.into_iter().enumerate() {
            player.clear_commands();
            let target = sample_target(
                "spotify:user:alice",
                &format!("spotify:track:t{idx}"),
                true,
                now - 5_000,
                ctx_uri,
                ctx_type,
                is_priv,
            );
            mgr.start(
                StartListenAlongRequest {
                    user_uri: "alice".to_string(),
                    ..Default::default()
                },
                Some("spotify:user:me"),
                Some(target),
                false,
                true,
            )
            .await
            .unwrap();

            match &player.commands()[0] {
                RecordedCommand::Play(cmd) => {
                    assert_eq!(cmd.context_uri, None, "case {idx} must omit context_uri");
                }
                other => panic!("expected Play, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn realtime_track_change_host_pause_and_resume_sync() {
        let now = 3_000_000;
        let (mgr, player, _clock) = make_manager(now);

        // Start while host is not yet seen -> Waiting.
        let st = mgr
            .start(
                StartListenAlongRequest {
                    user_uri: "alice".to_string(),
                    ..Default::default()
                },
                Some("spotify:user:me"),
                None,
                false,
                true,
            )
            .await
            .unwrap();
        assert_eq!(st.status, ListenAlongStatus::Waiting);
        assert!(player.commands().is_empty());

        // Host starts playing track 1 -> Synced!
        let t1 = sample_target(
            "spotify:user:alice",
            "spotify:track:t1",
            true,
            now + 1_000,
            Some("spotify:album:a1"),
            Some("album"),
            false,
        );
        mgr.on_live_update(&[t1], now + 3_000).await;
        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert_eq!(st.last_synced_track_uri.as_deref(), Some("spotify:track:t1"));

        // Host changes to track 2 -> immediately plays track 2!
        player.clear_commands();
        let t2 = sample_target(
            "spotify:user:alice",
            "spotify:track:t2",
            true,
            now + 50_000,
            Some("spotify:album:a1"),
            Some("album"),
            false,
        );
        mgr.on_live_update(std::slice::from_ref(&t2), now + 52_000).await;
        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert_eq!(st.last_synced_track_uri.as_deref(), Some("spotify:track:t2"));
        assert_eq!(player.commands().len(), 1);
        assert!(matches!(
            &player.commands()[0],
            RecordedCommand::Play(cmd) if cmd.track_uri == "spotify:track:t2" && cmd.position_ms == 2_500
        ));

        // Host pauses -> HostPaused & own player paused (sync_pause: true).
        player.clear_commands();
        let mut t2_paused = t2.clone();
        t2_paused.playing = false;
        mgr.on_live_update(&[t2_paused], now + 60_000).await;
        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::HostPaused);
        assert_eq!(
            player.commands(),
            vec![RecordedCommand::Pause(Some("dev-default".to_string()))]
        );

        // Host resumes shortly after -> own player still paused on same track within drift -> Resume!
        player.clear_commands();
        let mut t2_resumed = t2;
        t2_resumed.playing = true;
        t2_resumed.timestamp_ms = now + 61_000; // target_pos at now+63_000 = 2_500, own_obs pos = 2_500 -> drift = 0
        mgr.on_live_update(&[t2_resumed], now + 63_000).await;
        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert_eq!(
            player.commands(),
            vec![RecordedCommand::Resume(Some("dev-default".to_string()))]
        );
    }

    #[tokio::test]
    async fn manual_track_switch_diverges_or_auto_stops() {
        let now = 4_000_000;
        let (mgr, player, _clock) = make_manager(now);

        let target = sample_target(
            "spotify:user:alice",
            "spotify:track:t1",
            true,
            now - 10_000,
            None,
            None,
            false,
        );
        mgr.start(
            StartListenAlongRequest {
                user_uri: "alice".to_string(),
                ..Default::default()
            },
            Some("spotify:user:me"),
            Some(target),
            false,
            true,
        )
        .await
        .unwrap();

        // Within 4-second grace window: different track observation is ignored!
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:old_before_switch".to_string()),
                estimated_position_ms: 50_000,
            },
            now + 2_000,
        )
        .await;
        assert_eq!(mgr.state().await.status, ListenAlongStatus::Synced);

        // After 4-second grace window: switches to Diverged without issuing commands!
        player.clear_commands();
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:my_own_song".to_string()),
                estimated_position_ms: 1_000,
            },
            now + 6_000,
        )
        .await;
        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::Diverged);
        assert!(st.diverged);
        assert!(player.commands().is_empty());

        // Host's next track automatically clears diverged and re-syncs!
        let next_track = sample_target(
            "spotify:user:alice",
            "spotify:track:t2",
            true,
            now + 10_000,
            None,
            None,
            false,
        );
        mgr.on_live_update(&[next_track], now + 11_000).await;
        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert!(!st.diverged);
        assert_eq!(st.last_synced_track_uri.as_deref(), Some("spotify:track:t2"));

        // With auto_stop_on_manual_change = true, manual switch stops the session completely.
        mgr.update_settings(UpdateListenAlongSettingsRequest {
            auto_stop_on_manual_change: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
        let mut extra_rx = mgr.subscribe_extra_targets();
        assert!(!extra_rx.borrow_and_update().is_empty());

        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:another_own_song".to_string()),
                estimated_position_ms: 1_000,
            },
            now + 20_000,
        )
        .await;
        let st = mgr.state().await;
        assert!(!st.active);
        assert_eq!(st.status, ListenAlongStatus::Idle);
        assert!(extra_rx.borrow_and_update().is_empty());
    }

    #[tokio::test]
    async fn drift_auto_resync_and_cooldown() {
        let now = 5_000_000;
        let (mgr, player, _clock) = make_manager(now);

        let target = sample_target(
            "spotify:user:alice",
            "spotify:track:t1",
            true,
            now - 10_000,
            None,
            None,
            false,
        );
        mgr.start(
            StartListenAlongRequest {
                user_uri: "alice".to_string(),
                drift_threshold_ms: Some(5_000),
                latency_offset_ms: Some(0),
                ..Default::default()
            },
            Some("spotify:user:me"),
            Some(target),
            false,
            true,
        )
        .await
        .unwrap();
        player.clear_commands();

        // At now + 10_000, target is at 20_000ms. Own player is at 22_000ms -> drift +2000ms (< 5000ms) -> no seek.
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:t1".to_string()),
                estimated_position_ms: 22_000,
            },
            now + 10_000,
        )
        .await;
        assert_eq!(mgr.state().await.drift_ms, Some(2_000));
        assert!(player.commands().is_empty());

        // At now + 12_000, target is at 22_000ms. Own player is at 30_000ms -> drift +8000ms (> 5000ms) -> auto seek!
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:t1".to_string()),
                estimated_position_ms: 30_000,
            },
            now + 12_000,
        )
        .await;
        assert_eq!(
            player.commands(),
            vec![RecordedCommand::Seek {
                position_ms: 22_000,
                device_id: Some("dev-default".to_string()),
            }]
        );
        assert_eq!(mgr.state().await.drift_ms, Some(0));

        // At now + 20_000 (only 8s since last auto-resync, past 4s grace window but inside 15s cooldown):
        // drift is reported, but NO seek yet.
        player.clear_commands();
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:t1".to_string()),
                estimated_position_ms: 40_000, // target is 30_000 -> drift +10_000
            },
            now + 20_000,
        )
        .await;
        assert_eq!(mgr.state().await.drift_ms, Some(10_000));
        assert!(player.commands().is_empty());

        // At now + 28_000 (16s since last auto-resync -> cooldown elapsed): auto-seek fires again!
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:t1".to_string()),
                estimated_position_ms: 50_000, // target is 38_000 -> drift +12_000
            },
            now + 28_000,
        )
        .await;
        assert_eq!(
            player.commands(),
            vec![RecordedCommand::Seek {
                position_ms: 38_000,
                device_id: Some("dev-default".to_string()),
            }]
        );
    }

    #[tokio::test]
    async fn recovers_automatically_from_no_active_device() {
        let now = 6_000_000;
        let (mgr, player, _clock) = make_manager(now);

        player.set_observation(OwnPlayerObservation {
            connected: true,
            active_device_id: None,
            has_playable_device: false,
            is_playing: false,
            is_paused: false,
            track_uri: None,
            estimated_position_ms: 0,
        });
        player.set_next_play_error(ListenAlongError::new(
            ListenAlongErrorCode::NoActiveDevice,
            "Open Spotify on a device to start listening along",
        ));

        let target = sample_target(
            "spotify:user:alice",
            "spotify:track:t1",
            true,
            now - 5_000,
            None,
            None,
            false,
        );

        let st = mgr
            .start(
                StartListenAlongRequest {
                    user_uri: "alice".to_string(),
                    ..Default::default()
                },
                Some("spotify:user:me"),
                Some(target),
                false,
                true,
            )
            .await
            .unwrap();

        assert!(st.active);
        assert_eq!(st.status, ListenAlongStatus::Error);
        assert_eq!(st.last_error_code.as_deref(), Some("no_active_device"));

        // Playable device appears -> automatically recovers and syncs!
        player.clear_commands();
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("macbook-pro".to_string()),
                has_playable_device: true,
                is_playing: false,
                is_paused: false,
                track_uri: None,
                estimated_position_ms: 0,
            },
            now + 3_000,
        )
        .await;

        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert_eq!(st.last_error, None);
        assert_eq!(st.last_error_code, None);
        assert_eq!(st.sync_count, 1);
        assert_eq!(player.commands().len(), 1);
    }

    #[tokio::test]
    async fn settings_persist_across_idle_and_clamp_bounds() {
        let (mgr, _player, _clock) = make_manager(7_000_000);

        let st = mgr
            .update_settings(UpdateListenAlongSettingsRequest {
                device_id: Some(Some("  living-room  ".to_string())),
                sync_pause: Some(false),
                auto_resync: Some(false),
                drift_threshold_ms: Some(500),      // clamps to 1500
                latency_offset_ms: Some(99_999),    // clamps to 10000
                auto_stop_on_manual_change: Some(true),
            })
            .await
            .unwrap();

        assert!(!st.active);
        assert_eq!(st.settings.device_id.as_deref(), Some("living-room"));
        assert!(!st.settings.sync_pause);
        assert!(!st.settings.auto_resync);
        assert_eq!(st.settings.drift_threshold_ms, MIN_DRIFT_THRESHOLD_MS);
        assert_eq!(st.settings.latency_offset_ms, MAX_LATENCY_OFFSET_MS);
        assert!(st.settings.auto_stop_on_manual_change);

        // Explicit null clears device_id back to auto (None).
        let st = mgr
            .update_settings(UpdateListenAlongSettingsRequest {
                device_id: Some(None),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(st.settings.device_id, None);
    }

    #[tokio::test]
    async fn disconnect_stops_active_session_and_clears_extra_targets() {
        let now = 8_000_000;
        let (mgr, player, _clock) = make_manager(now);
        let mut extra_rx = mgr.subscribe_extra_targets();

        mgr.start(
            StartListenAlongRequest {
                user_uri: "alice".to_string(),
                ..Default::default()
            },
            Some("spotify:user:me"),
            None,
            false,
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            *extra_rx.borrow_and_update(),
            BTreeSet::from(["alice".to_string()])
        );

        mgr.handle_disconnect_or_logout().await;
        let st = mgr.state().await;
        assert!(!st.active);
        assert_eq!(st.status, ListenAlongStatus::Idle);
        assert!(extra_rx.borrow_and_update().is_empty());
        assert!(player.commands().is_empty());
    }

    #[tokio::test]
    async fn resync_handles_not_active_in_place_seek_and_diverged_recovery() {
        let now = 9_000_000;
        let (mgr, player, clock) = make_manager(now);

        let err = mgr
            .resync(ResyncListenAlongRequest::default(), None)
            .await
            .unwrap_err();
        assert_eq!(err.code, ListenAlongErrorCode::NotActive);

        let target = sample_target(
            "spotify:user:alice",
            "spotify:track:t1",
            true,
            now - 10_000,
            Some("spotify:album:a1"),
            Some("album"),
            false,
        );
        mgr.start(
            StartListenAlongRequest {
                user_uri: "alice".to_string(),
                latency_offset_ms: Some(0),
                ..Default::default()
            },
            Some("spotify:user:me"),
            Some(target.clone()),
            false,
            true,
        )
        .await
        .unwrap();

        // Already playing same track on same device -> resync seeks in place.
        player.clear_commands();
        clock.store(now + 8_000, Ordering::SeqCst);
        let st = mgr
            .resync(ResyncListenAlongRequest::default(), None)
            .await
            .unwrap();
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert_eq!(
            player.commands(),
            vec![RecordedCommand::Seek {
                position_ms: 18_000,
                device_id: Some("dev-default".to_string()),
            }]
        );

        // Diverged user manually switched tracks -> resync with device override clears diverged and plays on new device.
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:other".to_string()),
                estimated_position_ms: 5_000,
            },
            now + 20_000,
        )
        .await;
        assert_eq!(mgr.state().await.status, ListenAlongStatus::Diverged);

        player.clear_commands();
        clock.store(now + 25_000, Ordering::SeqCst);
        let st = mgr
            .resync(
                ResyncListenAlongRequest {
                    device_id: Some("speaker-2".to_string()),
                },
                Some(target),
            )
            .await
            .unwrap();
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert!(!st.diverged);
        assert_eq!(st.settings.device_id.as_deref(), Some("speaker-2"));
        assert_eq!(st.resolved_device_id.as_deref(), Some("speaker-2"));
        assert!(matches!(
            &player.commands()[0],
            RecordedCommand::Play(cmd) if cmd.device_id.as_deref() == Some("speaker-2") && cmd.position_ms == 35_000
        ));
    }

    #[tokio::test]
    async fn host_replay_same_track_and_broadcast_events_work() {
        let now = 10_000_000;
        let (mgr, player, _clock) = make_manager(now);
        let mut rx = mgr.subscribe();

        let target = sample_target(
            "spotify:user:alice",
            "spotify:track:t1",
            true,
            now - 5_000,
            None,
            None,
            false,
        );
        mgr.start(
            StartListenAlongRequest {
                user_uri: "alice".to_string(),
                latency_offset_ms: Some(0),
                ..Default::default()
            },
            Some("spotify:user:me"),
            Some(target.clone()),
            false,
            true,
        )
        .await
        .unwrap();

        // Broadcast should have published Syncing then Synced.
        let ev1 = rx.recv().await.unwrap();
        let ev2 = rx.recv().await.unwrap();
        assert_eq!(ev1.status, ListenAlongStatus::Syncing);
        assert_eq!(ev2.status, ListenAlongStatus::Synced);

        // User diverges, then host replays the same track (timestamp jumps > 15s forward).
        mgr.on_player_update(
            OwnPlayerObservation {
                connected: true,
                active_device_id: Some("dev-default".to_string()),
                has_playable_device: true,
                is_playing: true,
                is_paused: false,
                track_uri: Some("spotify:track:manual".to_string()),
                estimated_position_ms: 1_000,
            },
            now + 6_000,
        )
        .await;
        assert_eq!(mgr.state().await.status, ListenAlongStatus::Diverged);

        player.clear_commands();
        let mut replayed = target;
        replayed.timestamp_ms = now + 20_000;
        mgr.on_live_update(&[replayed], now + 21_000).await;

        let st = mgr.state().await;
        assert_eq!(st.status, ListenAlongStatus::Synced);
        assert!(!st.diverged);
        assert!(matches!(
            &player.commands()[0],
            RecordedCommand::Play(cmd) if cmd.track_uri == "spotify:track:t1" && cmd.position_ms == 1_000
        ));

        // Verify snake_case JSON serialization of status & error codes.
        assert_eq!(
            serde_json::to_string(&ListenAlongStatus::HostPaused).unwrap(),
            "\"host_paused\""
        );
        assert_eq!(
            serde_json::to_string(&ListenAlongErrorCode::NoActiveDevice).unwrap(),
            "\"no_active_device\""
        );
    }
}
