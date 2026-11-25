use std::collections::{BTreeSet, HashMap, HashSet};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chrono::Utc;
use futures_util::stream::{self, Stream, StreamExt};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify, RwLock, broadcast, watch};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::db::{
    ContextSummary, Database, FriendSummary, ListenRecord, OverlapReport, ProfileEvent,
    RebuiltTrack, StatsSummary, StoredProfile, spotify_uri_to_url,
};
use crate::spotify::{
    ActivityEvent, FriendActivity, NameCache, OwnPlaybackObserver, StreamConfig,
    TransportPreference, connect_cached, enrich_activity, fetch_user_profile, login_oauth,
    open_cache, spawn_activity_stream,
};

const MIN_POLL_INTERVAL_SECS: u64 = 10;
const DEFAULT_RESYNC_INTERVAL_SECS: u64 = 60;
const MANUAL_REFRESH_COOLDOWN: Duration = Duration::from_secs(5);
const MANUAL_REFRESH_WAIT: Duration = Duration::from_secs(3);
const SSE_KEEP_ALIVE_SECS: u64 = 15;
const PROFILE_SYNC_INTERVAL: Duration = Duration::from_secs(1800);
const OAUTH_LOGIN_TIMEOUT_SECS: u64 = 180;
const MAX_INFERRED_REPEATS: u32 = 10;

#[derive(Embed)]
#[folder = "web/dist"]
struct WebAssets;

pub struct UiConfig {
    pub cache_dir: PathBuf,
    pub db_path: PathBuf,
    pub host: String,
    pub port: u16,
    pub open_browser: bool,
    pub poll_interval_secs: u64,
    pub offline: bool,
    pub log_to_db: bool,
    pub transport: TransportPreference,
    pub debug_push: bool,
}

impl UiConfig {
    pub fn desktop_default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bside");
        Self {
            cache_dir: data_dir.join("cache"),
            db_path: data_dir.join("history.db"),
            host: "127.0.0.1".to_string(),
            port: 0,
            open_browser: false,
            poll_interval_secs: 15,
            offline: false,
            log_to_db: true,
            transport: TransportPreference::Auto,
            debug_push: false,
        }
    }
}

pub struct EmbeddedServerHandle {
    pub addr: std::net::SocketAddr,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl EmbeddedServerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.task.await;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountProfile {
    pub user_uri: String,
    pub username: String,
    pub display_name: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveFriendItem {
    pub user_uri: String,
    pub user_name: String,
    pub user_image_url: Option<String>,
    pub playing: bool,
    pub timestamp_ms: i64,
    pub item_type: String,
    pub track_uri: String,
    pub track_name: String,
    pub all_artists: String,
    pub album_name: Option<String>,
    pub cover_url: Option<String>,
    pub duration_ms: Option<i64>,
    pub popularity: Option<i32>,
    pub explicit: Option<bool>,
    pub release_date: Option<String>,
    pub context_name: Option<String>,
    pub context_uri: Option<String>,
    pub context_type: Option<String>,
    pub context_owner: Option<String>,
    pub context_is_private: bool,
    pub context_image_url: Option<String>,
    pub spotify_url: String,
    pub is_self: bool,
}

impl From<&FriendActivity> for LiveFriendItem {
    fn from(a: &FriendActivity) -> Self {
        let cover_url = a
            .track
            .cover_url
            .clone()
            .or_else(|| a.track.image_url.clone());

        Self {
            user_uri: a.user.uri.clone(),
            user_name: a.user.name.clone(),
            user_image_url: a.user.image_url.clone(),
            playing: a.playing,
            timestamp_ms: a.timestamp,
            item_type: a.item_type().to_string(),
            track_uri: a.track.uri.clone(),
            track_name: a.track.name.clone(),
            all_artists: a.all_artists_display(),
            album_name: a.album_name().map(str::to_string),
            cover_url,
            duration_ms: a.duration_ms(),
            popularity: a.track.popularity,
            explicit: a.track.explicit,
            release_date: a.track.release_date.clone(),
            context_name: a.context_name().map(str::to_string),
            context_uri: a.context_uri().map(str::to_string),
            context_type: a.context_type().map(str::to_string),
            context_owner: a.context_owner().map(str::to_string),
            context_is_private: a.is_private_context(),
            context_image_url: a.context_image_url().map(str::to_string),
            spotify_url: crate::format::spotify_uri_to_url(&a.track.uri),
            is_self: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveSnapshot {
    pub connected: bool,
    pub status: String, // "live" | "connecting" | "offline" | "unauthenticated" | "error"
    pub transport: Option<String>, // "dealer_push" | "mercury_push" | "http_polling" | "offline" | null
    pub message: Option<String>,
    pub account: Option<String>,
    pub account_uri: Option<String>,
    pub account_profile: Option<AccountProfile>,
    pub updated_at_ms: Option<i64>,
    pub poll_interval_secs: u64,
    pub friends: Vec<LiveFriendItem>,
}

fn sort_live_friends(friends: &mut [LiveFriendItem]) {
    friends.sort_by(|a, b| {
        b.playing
            .cmp(&a.playing)
            .then_with(|| b.timestamp_ms.cmp(&a.timestamp_ms))
            .then_with(|| a.user_name.to_lowercase().cmp(&b.user_name.to_lowercase()))
    });
}

pub struct LiveHub {
    latest: RwLock<LiveSnapshot>,
    account_profile: Arc<RwLock<Option<AccountProfile>>>,
    tx: broadcast::Sender<LiveSnapshot>,
    refresh_notify: Arc<Notify>,
    auth_notify: Arc<Notify>,
    auth_in_progress: Arc<AtomicBool>,
    last_manual_refresh: Mutex<Option<Instant>>,
    offline: bool,
}

impl LiveHub {
    pub fn new(poll_interval_secs: u64, offline: bool) -> Self {
        let interval = poll_interval_secs.max(MIN_POLL_INTERVAL_SECS);
        let initial = if offline {
            LiveSnapshot {
                connected: false,
                status: "offline".to_string(),
                transport: Some("offline".to_string()),
                message: Some("Running in offline archive mode".to_string()),
                account: None,
                account_uri: None,
                account_profile: None,
                updated_at_ms: None,
                poll_interval_secs: interval,
                friends: Vec::new(),
            }
        } else {
            LiveSnapshot {
                connected: false,
                status: "connecting".to_string(),
                transport: None,
                message: Some("Connecting to Spotify...".to_string()),
                account: None,
                account_uri: None,
                account_profile: None,
                updated_at_ms: None,
                poll_interval_secs: interval,
                friends: Vec::new(),
            }
        };

        let (tx, _rx) = broadcast::channel(32);
        Self {
            latest: RwLock::new(initial),
            account_profile: Arc::new(RwLock::new(None)),
            tx,
            refresh_notify: Arc::new(Notify::new()),
            auth_notify: Arc::new(Notify::new()),
            auth_in_progress: Arc::new(AtomicBool::new(false)),
            last_manual_refresh: Mutex::new(None),
            offline,
        }
    }

    pub fn notify_auth_changed(&self) {
        self.auth_notify.notify_waiters();
    }

    pub fn auth_in_progress_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.auth_in_progress)
    }

    pub async fn set_account_profile(&self, profile: Option<AccountProfile>) {
        let mut guard = self.account_profile.write().await;
        *guard = profile;
    }

    pub fn account_profile_handle(&self) -> Arc<RwLock<Option<AccountProfile>>> {
        Arc::clone(&self.account_profile)
    }

    pub async fn snapshot(&self) -> LiveSnapshot {
        self.latest.read().await.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LiveSnapshot> {
        self.tx.subscribe()
    }

    pub async fn publish(&self, snapshot: LiveSnapshot) {
        {
            let mut guard = self.latest.write().await;
            *guard = snapshot.clone();
        }
        let _ = self.tx.send(snapshot);
    }

    pub async fn request_refresh(&self) -> LiveSnapshot {
        if self.offline {
            return self.snapshot().await;
        }

        {
            let mut last = self.last_manual_refresh.lock().await;
            let now = Instant::now();
            if let Some(prev) = *last
                && now.duration_since(prev) < MANUAL_REFRESH_COOLDOWN
            {
                return self.snapshot().await;
            }
            *last = Some(now);
        }

        let mut rx = self.subscribe();
        self.refresh_notify.notify_one();

        let wait_result = tokio::time::timeout(MANUAL_REFRESH_WAIT, async {
            loop {
                match rx.recv().await {
                    Ok(snapshot) => return Some(snapshot),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .await;

        match wait_result {
            Ok(Some(snapshot)) => snapshot,
            _ => self.snapshot().await,
        }
    }
}

#[derive(Clone)]
struct AppState {
    db_path: Arc<PathBuf>,
    cache_dir: Arc<PathBuf>,
    live: Arc<LiveHub>,
    account_profile: Arc<RwLock<Option<AccountProfile>>>,
    auth_in_progress: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

fn internal_error(err: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: format!("{err:#}"),
        }),
    )
}

#[derive(Debug, Serialize)]
struct ApiListenRecord {
    #[serde(flatten)]
    record: ListenRecord,
    spotify_url: String,
    user_image_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiHistoryResponse {
    items: Vec<ApiListenRecord>,
    total: i64,
    limit: usize,
    offset: usize,
    has_more: bool,
}

#[derive(Debug, Serialize)]
struct ApiFriendSummary {
    #[serde(flatten)]
    friend: FriendSummary,
    is_self: bool,
}

#[derive(Debug, Serialize)]
struct ApiStoredProfile {
    #[serde(flatten)]
    profile: StoredProfile,
    is_self: bool,
}

#[derive(Debug, Serialize)]
struct OverviewResponse {
    stats: StatsSummary,
    friends_summary: Vec<ApiFriendSummary>,
    recent_listens: Vec<ApiListenRecord>,
}

#[derive(Debug, Deserialize)]
struct OverviewQuery {
    users: Option<String>,
    friend: Option<String>,
    days: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    users: Option<String>,
    friend: Option<String>,
    days: Option<String>,
    limit: Option<String>,
    offset: Option<String>,
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContextsQuery {
    users: Option<String>,
    friend: Option<String>,
    days: Option<String>,
    private_only: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RebuildQuery {
    users: Option<String>,
    friend: Option<String>,
    days: Option<String>,
    context: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OverlapQuery {
    days: Option<String>,
    users: Option<String>,
    friends: Option<String>,
    friend: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProfilesQuery {
    users: Option<String>,
    friend: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChangesQuery {
    users: Option<String>,
    friend: Option<String>,
    days: Option<String>,
    limit: Option<String>,
}

fn clean_opt_str(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn parse_users_filter(friend: Option<String>, users: Option<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in [users, friend].into_iter().flatten() {
        for part in raw.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(trimmed)) {
                out.push(trimmed.to_string());
            }
        }
    }
    out
}

fn parse_opt_i64(v: Option<String>) -> Option<i64> {
    clean_opt_str(v).and_then(|s| s.parse::<i64>().ok()).filter(|&n| n > 0)
}

fn parse_opt_usize(v: Option<String>) -> Option<usize> {
    clean_opt_str(v).and_then(|s| s.parse::<usize>().ok()).filter(|&n| n > 0)
}

fn parse_offset_usize(v: Option<String>) -> usize {
    clean_opt_str(v)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0)
}

fn parse_bool(v: Option<String>) -> bool {
    clean_opt_str(v)
        .map(|s| matches!(s.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn enrich_listen_records(
    db: &Database,
    records: Vec<ListenRecord>,
) -> Result<Vec<ApiListenRecord>> {
    if records.is_empty() {
        return Ok(Vec::new());
    }

    let profiles = db.query_profiles(None)?;
    let mut user_images: HashMap<String, String> = HashMap::new();
    let mut playlist_images: HashMap<String, String> = HashMap::new();

    for p in profiles {
        if let Some(img) = p.image_url.filter(|s| !s.is_empty()) {
            user_images.insert(p.user_uri.to_lowercase(), img);
        }
        for pl in p.public_playlists {
            if let Some(img) = pl.image_url.filter(|s| !s.is_empty()) {
                playlist_images.entry(pl.uri.to_lowercase()).or_insert(img);
            }
        }
    }

    let enriched = records
        .into_iter()
        .map(|mut record| {
            let has_ctx_img = record
                .context_image_url
                .as_deref()
                .is_some_and(|s| !s.is_empty());
            if !has_ctx_img
                && let Some(ctx_uri) = record.context_uri.as_deref()
                && let Some(img) = playlist_images.get(&ctx_uri.to_lowercase())
            {
                record.context_image_url = Some(img.clone());
            }
            let user_image_url = user_images.get(&record.user_uri.to_lowercase()).cloned();
            let spotify_url = spotify_uri_to_url(&record.track_uri);
            ApiListenRecord {
                record,
                spotify_url,
                user_image_url,
            }
        })
        .collect();

    Ok(enriched)
}

fn build_friends_summary(
    db: &Database,
    account_profile: Option<&AccountProfile>,
) -> Result<Vec<ApiFriendSummary>> {
    let account_uri = account_profile.map(|p| p.user_uri.as_str());
    let mut items: Vec<ApiFriendSummary> = db
        .query_friends_summary()?
        .into_iter()
        .map(|friend| {
            let is_self = account_uri
                .is_some_and(|uri| friend.user_uri.eq_ignore_ascii_case(uri));
            ApiFriendSummary { friend, is_self }
        })
        .collect();

    if let Some(acct) = account_profile
        && !items.iter().any(|item| item.is_self)
        && let Some(stored) = db.get_profile_by_uri(&acct.user_uri)?
    {
        items.push(ApiFriendSummary {
            friend: FriendSummary {
                user_uri: stored.user_uri,
                user_name: if stored.user_name.is_empty() {
                    acct.display_name.clone()
                } else {
                    stored.user_name
                },
                image_url: stored.image_url.or_else(|| acct.image_url.clone()),
                total_plays: 0,
                last_active_ms: 0,
                last_active_local: String::new(),
                latest_track_uri: String::new(),
                latest_track_name: String::new(),
                latest_artist_name: String::new(),
                latest_cover_url: None,
                latest_spotify_url: String::new(),
                is_live: false,
            },
            is_self: true,
        });
    }

    Ok(items)
}

async fn run_db<T, F>(db_path: Arc<PathBuf>, f: F) -> Result<T, (StatusCode, Json<ApiError>)>
where
    T: Send + 'static,
    F: FnOnce(&Database) -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let db = Database::open(&db_path)?;
        f(&db)
    })
    .await
    .map_err(|e| internal_error(anyhow::anyhow!("Database task panicked: {e}")))?
    .map_err(internal_error)
}

async fn api_overview(
    State(state): State<AppState>,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<OverviewResponse>, (StatusCode, Json<ApiError>)> {
    let users = parse_users_filter(query.friend, query.users);
    let days = parse_opt_i64(query.days);
    let account_profile = state.account_profile.read().await.clone();

    let data = run_db(state.db_path, move |db| {
        let stats = db.query_stats_for_users(&users, days)?;
        let friends_summary = build_friends_summary(db, account_profile.as_ref())?;
        let page = db.query_history_paginated(&users, days, None, 10, 0)?;
        let recent_listens = enrich_listen_records(db, page.items)?;
        Ok(OverviewResponse {
            stats,
            friends_summary,
            recent_listens,
        })
    })
    .await?;

    Ok(Json(data))
}

async fn api_friends(
    State(state): State<AppState>,
) -> Result<Json<Vec<ApiFriendSummary>>, (StatusCode, Json<ApiError>)> {
    let account_profile = state.account_profile.read().await.clone();
    let friends = run_db(state.db_path, move |db| {
        build_friends_summary(db, account_profile.as_ref())
    })
    .await?;
    Ok(Json(friends))
}

async fn api_history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<ApiHistoryResponse>, (StatusCode, Json<ApiError>)> {
    let users = parse_users_filter(query.friend, query.users);
    let days = parse_opt_i64(query.days);
    let limit = parse_opt_usize(query.limit).unwrap_or(50).min(200);
    let offset = parse_offset_usize(query.offset);
    let q = clean_opt_str(query.q);

    let response = run_db(state.db_path, move |db| {
        let page = db.query_history_paginated(&users, days, q.as_deref(), limit, offset)?;
        let items = enrich_listen_records(db, page.items)?;
        Ok(ApiHistoryResponse {
            items,
            total: page.total,
            limit: page.limit,
            offset: page.offset,
            has_more: page.has_more,
        })
    })
    .await?;

    Ok(Json(response))
}

async fn api_contexts(
    State(state): State<AppState>,
    Query(query): Query<ContextsQuery>,
) -> Result<Json<Vec<ContextSummary>>, (StatusCode, Json<ApiError>)> {
    let users = parse_users_filter(query.friend, query.users);
    let days = parse_opt_i64(query.days);
    let private_only = parse_bool(query.private_only);

    let rows = run_db(state.db_path, move |db| {
        db.query_contexts_for_users(&users, days, private_only)
    })
    .await?;

    Ok(Json(rows))
}

async fn api_rebuild(
    State(state): State<AppState>,
    Query(query): Query<RebuildQuery>,
) -> Result<Json<Vec<RebuiltTrack>>, (StatusCode, Json<ApiError>)> {
    let users = parse_users_filter(query.friend, query.users);
    let days = parse_opt_i64(query.days);
    let Some(context) = clean_opt_str(query.context) else {
        return Ok(Json(Vec::new()));
    };

    let rows = run_db(state.db_path, move |db| {
        db.rebuild_playlist_for_users(&users, days, &context)
    })
    .await?;

    Ok(Json(rows))
}

async fn api_overlap(
    State(state): State<AppState>,
    Query(query): Query<OverlapQuery>,
) -> Result<Json<OverlapReport>, (StatusCode, Json<ApiError>)> {
    let days = parse_opt_i64(query.days);
    let mut friends = parse_users_filter(query.friend, query.users);
    for extra in parse_users_filter(query.friends, None) {
        if !friends.iter().any(|x| x.eq_ignore_ascii_case(&extra)) {
            friends.push(extra);
        }
    }

    let report = run_db(state.db_path, move |db| db.query_overlap(days, &friends)).await?;
    Ok(Json(report))
}

async fn api_profiles(
    State(state): State<AppState>,
    Query(query): Query<ProfilesQuery>,
) -> Result<Json<Vec<ApiStoredProfile>>, (StatusCode, Json<ApiError>)> {
    let users = parse_users_filter(query.friend, query.users);
    let account_uri = state
        .account_profile
        .read()
        .await
        .as_ref()
        .map(|p| p.user_uri.clone());

    let profiles = run_db(state.db_path, move |db| {
        let rows = db.query_profiles_for_users(&users)?;
        Ok(rows
            .into_iter()
            .map(|profile| {
                let is_self = account_uri
                    .as_deref()
                    .is_some_and(|uri| profile.user_uri.eq_ignore_ascii_case(uri));
                ApiStoredProfile { profile, is_self }
            })
            .collect())
    })
    .await?;
    Ok(Json(profiles))
}

async fn api_changes(
    State(state): State<AppState>,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<Vec<ProfileEvent>>, (StatusCode, Json<ApiError>)> {
    let users = parse_users_filter(query.friend, query.users);
    let days = parse_opt_i64(query.days);
    let limit = parse_opt_usize(query.limit).unwrap_or(100).min(1000);

    let events = run_db(state.db_path, move |db| {
        db.query_profile_events_for_users(&users, days, limit)
    })
    .await?;

    Ok(Json(events))
}

fn snapshot_to_sse_event(snapshot: &LiveSnapshot) -> Event {
    let data = serde_json::to_string(snapshot).unwrap_or_else(|_| "{}".to_string());
    Event::default().event("snapshot").data(data)
}

async fn api_live(State(state): State<AppState>) -> Json<LiveSnapshot> {
    Json(state.live.snapshot().await)
}

async fn api_live_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.live.subscribe();
    let initial = state.live.snapshot().await;

    let initial_stream = stream::once(async move {
        Ok::<Event, Infallible>(snapshot_to_sse_event(&initial))
    });

    let updates_stream = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(snapshot) => Some(Ok::<Event, Infallible>(snapshot_to_sse_event(&snapshot))),
            Err(BroadcastStreamRecvError::Lagged(_)) => None,
        }
    });

    let stream = initial_stream.chain(updates_stream);

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(SSE_KEEP_ALIVE_SECS))
            .text("keep-alive"),
    )
}

async fn api_live_refresh(State(state): State<AppState>) -> Json<LiveSnapshot> {
    Json(state.live.request_refresh().await)
}

#[derive(Debug, Serialize)]
struct AuthStatusResponse {
    authenticated: bool,
    in_progress: bool,
    account: Option<String>,
    account_profile: Option<AccountProfile>,
}

#[derive(Debug, Serialize)]
struct AuthLoginResponse {
    started: bool,
    in_progress: bool,
}

#[derive(Debug, Serialize)]
struct AuthLogoutResponse {
    ok: bool,
    authenticated: bool,
}

fn has_cached_credentials(cache_dir: &Path) -> bool {
    open_cache(cache_dir)
        .ok()
        .and_then(|cache| cache.credentials())
        .is_some()
}

fn remove_cached_credentials(cache_dir: &Path) -> Result<()> {
    let cred_path = cache_dir.join("credentials.json");
    match std::fs::remove_file(&cred_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(anyhow::anyhow!(
            "Failed to remove cached credentials at {}: {err}",
            cred_path.display()
        )),
    }
}

async fn api_auth_status(State(state): State<AppState>) -> Json<AuthStatusResponse> {
    let snapshot = state.live.snapshot().await;
    let in_progress = state.auth_in_progress.load(Ordering::SeqCst);
    let cache_dir = (*state.cache_dir).clone();
    let has_creds = tokio::task::spawn_blocking(move || has_cached_credentials(&cache_dir))
        .await
        .unwrap_or(false);
    let authenticated = has_creds || (snapshot.connected && snapshot.status == "live");
    let account_profile = state
        .account_profile
        .read()
        .await
        .clone()
        .or(snapshot.account_profile);

    Json(AuthStatusResponse {
        authenticated,
        in_progress,
        account: snapshot.account,
        account_profile,
    })
}

async fn api_auth_login(State(state): State<AppState>) -> Json<AuthLoginResponse> {
    if state
        .auth_in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Json(AuthLoginResponse {
            started: false,
            in_progress: true,
        });
    }

    let cache_dir = (*state.cache_dir).clone();
    let live = Arc::clone(&state.live);
    let auth_in_progress = Arc::clone(&state.auth_in_progress);

    tokio::spawn(async move {
        let prev = live.snapshot().await;
        live.publish(LiveSnapshot {
            connected: false,
            status: "connecting".to_string(),
            transport: prev.transport,
            message: Some("Waiting for Spotify browser login...".to_string()),
            account: prev.account,
            account_uri: prev.account_uri,
            account_profile: prev.account_profile,
            updated_at_ms: Some(Utc::now().timestamp_millis()),
            poll_interval_secs: prev.poll_interval_secs,
            friends: prev.friends,
        })
        .await;

        let login_result = tokio::time::timeout(
            Duration::from_secs(OAUTH_LOGIN_TIMEOUT_SECS),
            login_oauth(&cache_dir, true, None),
        )
        .await;

        auth_in_progress.store(false, Ordering::SeqCst);

        match login_result {
            Ok(Ok(session)) => {
                session.shutdown();
                live.notify_auth_changed();
            }
            Ok(Err(err)) => {
                let err_msg = format!("{err:#}");
                eprintln!("[warn] In-app OAuth login failed: {err_msg}");
                let has_creds = has_cached_credentials(&cache_dir);
                let prev = live.snapshot().await;
                live.publish(LiveSnapshot {
                    connected: false,
                    status: if has_creds { "error" } else { "unauthenticated" }.to_string(),
                    transport: if has_creds { prev.transport } else { None },
                    message: Some(format!("Spotify login failed: {err_msg}")),
                    account: if has_creds { prev.account } else { None },
                    account_uri: if has_creds { prev.account_uri } else { None },
                    account_profile: if has_creds { prev.account_profile } else { None },
                    updated_at_ms: Some(Utc::now().timestamp_millis()),
                    poll_interval_secs: prev.poll_interval_secs,
                    friends: if has_creds { prev.friends } else { Vec::new() },
                })
                .await;
            }
            Err(_) => {
                eprintln!(
                    "[warn] In-app OAuth login timed out after {OAUTH_LOGIN_TIMEOUT_SECS}s"
                );
                let has_creds = has_cached_credentials(&cache_dir);
                let prev = live.snapshot().await;
                live.publish(LiveSnapshot {
                    connected: false,
                    status: if has_creds { "error" } else { "unauthenticated" }.to_string(),
                    transport: if has_creds { prev.transport } else { None },
                    message: Some("Spotify login timed out. Please try again.".to_string()),
                    account: if has_creds { prev.account } else { None },
                    account_uri: if has_creds { prev.account_uri } else { None },
                    account_profile: if has_creds { prev.account_profile } else { None },
                    updated_at_ms: Some(Utc::now().timestamp_millis()),
                    poll_interval_secs: prev.poll_interval_secs,
                    friends: if has_creds { prev.friends } else { Vec::new() },
                })
                .await;
            }
        }
    });

    Json(AuthLoginResponse {
        started: true,
        in_progress: true,
    })
}

async fn api_auth_logout(
    State(state): State<AppState>,
) -> Result<Json<AuthLogoutResponse>, (StatusCode, Json<ApiError>)> {
    let cache_dir = (*state.cache_dir).clone();
    tokio::task::spawn_blocking(move || remove_cached_credentials(&cache_dir))
        .await
        .map_err(|e| internal_error(anyhow::anyhow!("Logout task panicked: {e}")))?
        .map_err(internal_error)?;

    state.live.set_account_profile(None).await;
    let prev = state.live.snapshot().await;
    state
        .live
        .publish(LiveSnapshot {
            connected: false,
            status: "unauthenticated".to_string(),
            transport: None,
            message: Some("Signed out of Spotify".to_string()),
            account: None,
            account_uri: None,
            account_profile: None,
            updated_at_ms: Some(Utc::now().timestamp_millis()),
            poll_interval_secs: prev.poll_interval_secs,
            friends: Vec::new(),
        })
        .await;
    state.live.notify_auth_changed();

    Ok(Json(AuthLogoutResponse {
        ok: true,
        authenticated: false,
    }))
}

async fn api_not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "API endpoint not found".to_string(),
        }),
    )
}

async fn serve_static(uri: Uri) -> Response {
    let raw_path = uri.path().trim_start_matches('/');
    let path = if raw_path.is_empty() {
        "index.html"
    } else {
        raw_path
    };

    if path != ".gitkeep"
        && let Some(asset) = WebAssets::get(path)
    {
        return asset_response(path, asset.data.into_owned());
    }

    // SPA fallback for client-side routes (only when not requesting a missing file with an extension)
    if !path.contains('.')
        && let Some(index) = WebAssets::get("index.html")
    {
        return asset_response("index.html", index.data.into_owned());
    }

    (
        StatusCode::NOT_FOUND,
        "Web UI assets not built. Run `npm install && npm run build` inside `web/`.",
    )
        .into_response()
}

fn asset_response(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut response = (StatusCode::OK, bytes).into_response();
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
}

fn enrich_own_context_from_db(db_path: &Path, activity: &mut FriendActivity) {
    let Some(ctx) = activity.track.context.as_mut() else {
        return;
    };
    let needs_name = ctx.name.is_empty();
    let needs_image = ctx.image_url.as_deref().is_none_or(str::is_empty);
    if !needs_name && !needs_image {
        return;
    }
    let Ok(db) = Database::open(db_path) else {
        return;
    };
    let Ok(profiles) = db.query_profiles(None) else {
        return;
    };
    for profile in profiles {
        if let Some(pl) = profile
            .public_playlists
            .into_iter()
            .find(|p| p.uri.eq_ignore_ascii_case(&ctx.uri))
        {
            if needs_name && !pl.name.is_empty() {
                ctx.name = pl.name;
            }
            if ctx.owner.is_none() {
                ctx.owner = pl.owner_name;
            }
            if needs_image && pl.image_url.as_deref().is_some_and(|s| !s.is_empty()) {
                ctx.image_url = pl.image_url;
            }
            break;
        }
    }
}

struct ActivePlay {
    track_uri: String,
    cycle_started_at_ms: i64,
    last_seen_ts_ms: i64,
    duration_ms: Option<i64>,
    inferred_cycles: u32,
    is_playing: bool,
    session_id: Option<i64>,
    missed_polls: u32,
}

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
        return ts_delta < max_gap && elapsed_from_cycle < max_gap;
    }

    let within_track_window = match prev_duration_ms {
        Some(d) if d > 0 => elapsed_from_cycle < d + repeat_grace_ms,
        _ => elapsed_from_cycle < 180_000,
    };
    within_track_window && ts_delta < max_gap && elapsed_from_cycle < max_gap
}

fn process_single_db_activity(
    db: &Database,
    item: &FriendActivity,
    active_plays: &mut HashMap<String, ActivePlay>,
    now_ms: i64,
    grace_ms: i64,
) -> Result<String> {
    let user_uri = item.user.uri.clone();

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
            let ended_at_ms = item.timestamp.clamp(prev.cycle_started_at_ms, now_ms);
            if prev.is_playing {
                let switched_to_next_track = prev.track_uri != item.track.uri;
                let _ = db.finalize_play(
                    &user_uri,
                    &prev.track_uri,
                    prev.cycle_started_at_ms,
                    ended_at_ms,
                    prev.duration_ms,
                    switched_to_next_track,
                );
            }
            if !item.playing
                && let Some(sid) = session_id.take()
            {
                let _ = db.end_session(sid, ended_at_ms);
            }
        }

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

        if !skip_cold_start_last {
            db.insert_listen(item)?;
        }

        if item.playing {
            if let Some(sid) = session_id {
                let _ = db.touch_session(sid, now_ms, true);
            } else {
                let start_ms = item.timestamp.min(now_ms);
                if let Ok(sid) = db.start_session(&user_uri, &item.user.name, start_ms) {
                    let _ = db.touch_session(sid, now_ms, true);
                    session_id = Some(sid);
                }
            }
        }

        active_plays.insert(
            user_uri.clone(),
            ActivePlay {
                track_uri: item.track.uri.clone(),
                cycle_started_at_ms: item.timestamp,
                last_seen_ts_ms: item.timestamp,
                duration_ms: item.duration_ms(),
                inferred_cycles: 0,
                is_playing: item.playing,
                session_id,
                missed_polls: 0,
            },
        );
    } else if let Some(prev) = active_plays.get_mut(&user_uri) {
        prev.missed_polls = 0;
        prev.last_seen_ts_ms = item.timestamp;

        if prev.is_playing && !item.playing {
            let ended_at_ms = item.timestamp.clamp(prev.cycle_started_at_ms, now_ms);
            let _ = db.finalize_play(
                &user_uri,
                &prev.track_uri,
                prev.cycle_started_at_ms,
                ended_at_ms,
                prev.duration_ms,
                false,
            );
            if let Some(sid) = prev.session_id.take() {
                let _ = db.end_session(sid, ended_at_ms);
            }
            prev.is_playing = false;
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

            if let Some(dur) = prev.duration_ms
                && dur >= 30_000
                && prev.inferred_cycles < MAX_INFERRED_REPEATS
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

                if db.insert_inferred_repeat(item, cycle_end).unwrap_or(false)
                    && let Some(sid) = prev.session_id
                {
                    let _ = db.touch_session(sid, now_ms, true);
                }
            }
        }
    }

    Ok(user_uri)
}

fn log_poll_activities_to_db(
    db_path: &Path,
    activities: &[FriendActivity],
    active_plays: &mut HashMap<String, ActivePlay>,
    now_ms: i64,
    grace_ms: i64,
    check_missed_polls: bool,
) -> Result<()> {
    let db = Database::open(db_path)?;
    let mut seen_users: HashSet<String> = HashSet::new();

    for item in activities {
        let user_uri = process_single_db_activity(&db, item, active_plays, now_ms, grace_ms)?;
        seen_users.insert(user_uri);
    }

    if check_missed_polls {
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
                }
            }
        }
    }

    Ok(())
}

async fn log_activities_with_state(
    db_path: &Path,
    activities: Vec<FriendActivity>,
    active_plays: &mut HashMap<String, ActivePlay>,
    now_ms: i64,
    grace_ms: i64,
    check_missed_polls: bool,
) {
    if activities.is_empty() && !check_missed_polls {
        return;
    }
    let db_path_clone = db_path.to_path_buf();
    let mut plays_for_task = std::mem::take(active_plays);
    let log_res = tokio::task::spawn_blocking(move || {
        let res = log_poll_activities_to_db(
            &db_path_clone,
            &activities,
            &mut plays_for_task,
            now_ms,
            grace_ms,
            check_missed_polls,
        );
        (plays_for_task, res)
    })
    .await;
    match log_res {
        Ok((returned_plays, Ok(()))) => {
            *active_plays = returned_plays;
        }
        Ok((returned_plays, Err(err))) => {
            *active_plays = returned_plays;
            eprintln!("[warn] Failed to log live listens to SQLite: {err:#}");
        }
        Err(err) => {
            eprintln!("[warn] Live DB logging task panicked: {err}");
        }
    }
}

async fn close_open_sessions(db_path: &Path, active_plays: &mut HashMap<String, ActivePlay>) {
    let session_ids: Vec<i64> = active_plays
        .values_mut()
        .filter_map(|state| state.session_id.take())
        .collect();
    if session_ids.is_empty() {
        return;
    }
    let db_path_owned = db_path.to_path_buf();
    let end_now_ms = Utc::now().timestamp_millis();
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(db) = Database::open(&db_path_owned) {
            for sid in session_ids {
                let _ = db.end_session(sid, end_now_ms);
            }
        }
    })
    .await;
}

fn build_sorted_live_friends(
    friends_map: &HashMap<String, FriendActivity>,
    account_uri: Option<&str>,
) -> Vec<LiveFriendItem> {
    let mut friends: Vec<LiveFriendItem> = friends_map
        .values()
        .map(|a| {
            let mut item = LiveFriendItem::from(a);
            item.is_self =
                account_uri.is_some_and(|u| item.user_uri.eq_ignore_ascii_case(u));
            item
        })
        .collect();
    sort_live_friends(&mut friends);
    friends
}

fn format_transport_banner(transport: TransportPreference, poll_interval_secs: u64) -> String {
    match transport {
        TransportPreference::Auto => {
            format!("auto (realtime push -> {poll_interval_secs}s poll fallback)")
        }
        TransportPreference::Dealer => {
            format!("dealer (Dealer WS push -> {poll_interval_secs}s poll fallback)")
        }
        TransportPreference::Mercury => {
            format!("mercury (Mercury push -> {poll_interval_secs}s poll fallback)")
        }
        TransportPreference::Poll => format!("poll ({poll_interval_secs}s HTTP polling)"),
    }
}

async fn wait_own_update(rx: &mut Option<watch::Receiver<u64>>) -> bool {
    match rx {
        Some(r) => r.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}

async fn prepare_own_activity(
    own_observer: Option<&OwnPlaybackObserver>,
    own_session: Option<&librespot_core::session::Session>,
    own_name_cache: &mut NameCache,
    account_uri: Option<&str>,
    active_plays: &HashMap<String, ActivePlay>,
    db_path: &Path,
) -> Option<FriendActivity> {
    let mut own_activity = own_observer?.latest()?;
    if let Some(session) = own_session {
        enrich_activity(session, &mut own_activity, own_name_cache).await;
    }
    if let Some(own_uri) = account_uri
        && let Some(prev) = active_plays.get(own_uri)
        && prev.track_uri == own_activity.track.uri
        && (own_activity.timestamp - prev.cycle_started_at_ms).abs() <= 20_000
    {
        own_activity.timestamp = prev.cycle_started_at_ms;
    }
    let db_path_clone = db_path.to_path_buf();
    let enriched = tokio::task::spawn_blocking(move || {
        enrich_own_context_from_db(&db_path_clone, &mut own_activity);
        own_activity
    })
    .await
    .ok()?;
    Some(enriched)
}

#[allow(clippy::too_many_arguments)]
fn spawn_live_stream(
    live: Arc<LiveHub>,
    cache_dir: PathBuf,
    db_path: PathBuf,
    poll_interval_secs: u64,
    transport_pref: TransportPreference,
    debug_push: bool,
    log_to_db: bool,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let interval_secs = poll_interval_secs.max(MIN_POLL_INTERVAL_SECS);
    let grace_ms = ((interval_secs as i64) * 1000).max(25_000);

    tokio::spawn(async move {
        let mut own_name_cache = NameCache::default();
        let mut active_plays: HashMap<String, ActivePlay> = HashMap::new();
        let mut known_friend_uris: BTreeSet<String> = BTreeSet::new();
        let mut last_profile_sync: Option<Instant> = None;

        loop {
            if *shutdown_rx.borrow() {
                close_open_sessions(&db_path, &mut active_plays).await;
                return;
            }

            // Register wakeup before checking credentials so a concurrent login notify is never missed.
            let auth_notified = live.auth_notify.notified();
            tokio::pin!(auth_notified);
            auth_notified.as_mut().enable();

            let cache_dir_check = cache_dir.clone();
            let has_credentials =
                tokio::task::spawn_blocking(move || has_cached_credentials(&cache_dir_check))
                    .await
                    .unwrap_or(false);

            if !has_credentials {
                close_open_sessions(&db_path, &mut active_plays).await;
                active_plays.clear();
                live.set_account_profile(None).await;
                let prev = live.snapshot().await;
                if prev.status != "unauthenticated" || prev.connected || prev.account.is_some() {
                    live.publish(LiveSnapshot {
                        connected: false,
                        status: "unauthenticated".to_string(),
                        transport: None,
                        message: Some(
                            "Connect your Spotify account to enable live activity".to_string(),
                        ),
                        account: None,
                        account_uri: None,
                        account_profile: None,
                        updated_at_ms: Some(Utc::now().timestamp_millis()),
                        poll_interval_secs: interval_secs,
                        friends: Vec::new(),
                    })
                    .await;
                }

                tokio::select! {
                    _ = auth_notified => {}
                    _ = shutdown_rx.changed() => {
                        return;
                    }
                }
                continue;
            }

            let own_session = match connect_cached(&cache_dir).await {
                Ok(session) => Some(session),
                Err(err) => {
                    let err_msg = format!("{err:#}");
                    if err_msg.contains("No cached Spotify credentials") {
                        continue;
                    }
                    eprintln!("[warn] Own playback observer session connect failed: {err_msg}");
                    None
                }
            };

            let mut current_account: Option<String> = own_session
                .as_ref()
                .map(|s| s.username())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    open_cache(&cache_dir)
                        .ok()
                        .and_then(|cache| cache.credentials())
                        .and_then(|creds| creds.username)
                        .filter(|s| !s.is_empty())
                });
            let mut account_uri = current_account
                .as_ref()
                .map(|u| format!("spotify:user:{u}"));
            if let Some(uri) = &account_uri {
                known_friend_uris.insert(uri.clone());
            }

            let mut account_profile: Option<AccountProfile> = if let (Some(username), Some(own_uri)) =
                (&current_account, &account_uri)
            {
                let fetched = if let Some(session) = own_session.as_ref() {
                    fetch_user_profile(session, own_uri).await.ok()
                } else {
                    None
                };
                match fetched {
                    Some(snap) => {
                        let display_name = if snap.user_name.trim().is_empty() {
                            username.clone()
                        } else {
                            snap.user_name.clone()
                        };
                        let image_url = snap.image_url.clone();
                        let db_path_clone = db_path.clone();
                        let snap_clone = snap.clone();
                        let _ = tokio::task::spawn_blocking(move || -> Result<()> {
                            let db = Database::open(&db_path_clone)?;
                            let _ = db.upsert_profile(&snap_clone)?;
                            Ok(())
                        })
                        .await;
                        Some(AccountProfile {
                            user_uri: own_uri.clone(),
                            username: username.clone(),
                            display_name,
                            image_url,
                        })
                    }
                    None => {
                        let db_path_clone = db_path.clone();
                        let own_uri_clone = own_uri.clone();
                        let stored = tokio::task::spawn_blocking(move || -> Option<StoredProfile> {
                            Database::open(&db_path_clone)
                                .ok()
                                .and_then(|db| db.get_profile_by_uri(&own_uri_clone).ok().flatten())
                        })
                        .await
                        .ok()
                        .flatten();

                        match stored {
                            Some(p) => Some(AccountProfile {
                                user_uri: own_uri.clone(),
                                username: username.clone(),
                                display_name: if p.user_name.trim().is_empty() {
                                    username.clone()
                                } else {
                                    p.user_name
                                },
                                image_url: p.image_url,
                            }),
                            None => Some(AccountProfile {
                                user_uri: own_uri.clone(),
                                username: username.clone(),
                                display_name: username.clone(),
                                image_url: None,
                            }),
                        }
                    }
                }
            } else {
                None
            };

            live.set_account_profile(account_profile.clone()).await;

            let mut own_observer: Option<OwnPlaybackObserver> = None;
            let mut own_updates_rx: Option<watch::Receiver<u64>> = None;
            if let Some(session) = own_session.as_ref() {
                let (own_name, own_image) = match &account_profile {
                    Some(p) => (p.display_name.clone(), p.image_url.clone()),
                    None => (session.username(), None),
                };
                let obs = OwnPlaybackObserver::spawn(session.clone(), own_name, own_image);
                own_updates_rx = Some(obs.subscribe_updates());
                own_observer = Some(obs);
            }

            let mut friends_map: HashMap<String, FriendActivity> = HashMap::new();
            let mut current_transport: Option<String> = None;

            let (stream_shutdown_tx, stream_shutdown_rx) = watch::channel(false);
            let stream_config = StreamConfig {
                poll_interval_secs: interval_secs,
                resync_interval_secs: DEFAULT_RESYNC_INTERVAL_SECS,
                transport: transport_pref,
                debug_push,
            };

            let (mut event_rx, stream_handle) = spawn_activity_stream(
                cache_dir.clone(),
                stream_config,
                Arc::clone(&live.refresh_notify),
                stream_shutdown_rx,
            );

            loop {
                if *shutdown_rx.borrow() {
                    let _ = stream_shutdown_tx.send(true);
                    let _ = tokio::time::timeout(Duration::from_secs(2), stream_handle).await;
                    close_open_sessions(&db_path, &mut active_plays).await;
                    if let Some(obs) = own_observer.take() {
                        obs.shutdown().await;
                    }
                    if let Some(session) = own_session {
                        session.shutdown();
                    }
                    return;
                }

                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            let _ = stream_shutdown_tx.send(true);
                            let _ = tokio::time::timeout(Duration::from_secs(2), stream_handle).await;
                            close_open_sessions(&db_path, &mut active_plays).await;
                            if let Some(obs) = own_observer.take() {
                                obs.shutdown().await;
                            }
                            if let Some(session) = own_session {
                                session.shutdown();
                            }
                            return;
                        }
                    }
                    _ = live.auth_notify.notified() => {
                        let cache_dir_check = cache_dir.clone();
                        let still_auth = tokio::task::spawn_blocking(move || {
                            has_cached_credentials(&cache_dir_check)
                        })
                        .await
                        .unwrap_or(true);
                        if !still_auth {
                            let _ = stream_shutdown_tx.send(true);
                            let _ = tokio::time::timeout(Duration::from_secs(2), stream_handle).await;
                            close_open_sessions(&db_path, &mut active_plays).await;
                            active_plays.clear();
                            if let Some(obs) = own_observer.take() {
                                obs.shutdown().await;
                            }
                            if let Some(session) = own_session {
                                session.shutdown();
                            }
                            break;
                        }
                    }
                    updated = wait_own_update(&mut own_updates_rx) => {
                        if !updated {
                            own_updates_rx = None;
                        } else if let Some(own_activity) = prepare_own_activity(
                            own_observer.as_ref(),
                            own_session.as_ref(),
                            &mut own_name_cache,
                            account_uri.as_deref(),
                            &active_plays,
                            &db_path,
                        )
                        .await
                        {
                            let now_ms = Utc::now().timestamp_millis();
                            known_friend_uris.insert(own_activity.user.uri.clone());
                            friends_map.insert(own_activity.user.uri.clone(), own_activity.clone());

                            if log_to_db {
                                log_activities_with_state(
                                    &db_path,
                                    vec![own_activity],
                                    &mut active_plays,
                                    now_ms,
                                    grace_ms,
                                    false,
                                )
                                .await;
                            }

                            let prev = live.snapshot().await;
                            if prev.connected && prev.status == "live" {
                                let sorted =
                                    build_sorted_live_friends(&friends_map, account_uri.as_deref());
                                live.publish(LiveSnapshot {
                                    connected: true,
                                    status: "live".to_string(),
                                    transport: current_transport.clone().or(prev.transport),
                                    message: None,
                                    account: current_account.clone(),
                                    account_uri: account_uri.clone(),
                                    account_profile: account_profile.clone(),
                                    updated_at_ms: Some(now_ms),
                                    poll_interval_secs: interval_secs,
                                    friends: sorted,
                                })
                                .await;
                            }
                        }
                    }
                    ev = event_rx.recv() => {
                        let Some(event) = ev else {
                            let _ = stream_shutdown_tx.send(true);
                            let _ = tokio::time::timeout(Duration::from_secs(2), stream_handle).await;
                            if let Some(obs) = own_observer.take() {
                                obs.shutdown().await;
                            }
                            if let Some(session) = own_session {
                                session.shutdown();
                            }
                            break;
                        };

                        match event {
                            ActivityEvent::SessionReconnected { account } => {
                                if let Some(acc) = account {
                                    account_uri = Some(format!("spotify:user:{acc}"));
                                    current_account = Some(acc);
                                }
                            }
                            ActivityEvent::TransportChanged { mode, .. } => {
                                current_transport = Some(mode.as_str().to_string());
                                let prev = live.snapshot().await;
                                if prev.connected && prev.status == "live" {
                                    live.publish(LiveSnapshot {
                                        transport: current_transport.clone(),
                                        ..prev
                                    })
                                    .await;
                                }
                            }
                            ActivityEvent::FullSnapshot { mut friends, transport } => {
                                let now_ms = Utc::now().timestamp_millis();
                                current_transport = Some(transport.as_str().to_string());

                                if let Some(own_activity) = prepare_own_activity(
                                    own_observer.as_ref(),
                                    own_session.as_ref(),
                                    &mut own_name_cache,
                                    account_uri.as_deref(),
                                    &active_plays,
                                    &db_path,
                                )
                                .await
                                {
                                    friends.retain(|a| {
                                        !a.user.uri.eq_ignore_ascii_case(&own_activity.user.uri)
                                    });
                                    friends.push(own_activity);
                                }

                                friends_map.clear();
                                for item in &friends {
                                    known_friend_uris.insert(item.user.uri.clone());
                                    friends_map.insert(item.user.uri.clone(), item.clone());
                                }

                                if log_to_db {
                                    log_activities_with_state(
                                        &db_path,
                                        friends,
                                        &mut active_plays,
                                        now_ms,
                                        grace_ms,
                                        true,
                                    )
                                    .await;
                                }

                                let sorted =
                                    build_sorted_live_friends(&friends_map, account_uri.as_deref());
                                live.publish(LiveSnapshot {
                                    connected: true,
                                    status: "live".to_string(),
                                    transport: current_transport.clone(),
                                    message: None,
                                    account: current_account.clone(),
                                    account_uri: account_uri.clone(),
                                    account_profile: account_profile.clone(),
                                    updated_at_ms: Some(now_ms),
                                    poll_interval_secs: interval_secs,
                                    friends: sorted,
                                })
                                .await;

                                // Periodic profile sync (every 30 min) when logging to SQLite
                                if log_to_db
                                    && !known_friend_uris.is_empty()
                                    && last_profile_sync
                                        .is_none_or(|t| t.elapsed() >= PROFILE_SYNC_INTERVAL)
                                    && let Some(session) = own_session.as_ref().filter(|s| !s.is_invalid())
                                {
                                    let uris_to_sync: Vec<String> =
                                        known_friend_uris.iter().cloned().collect();
                                    for uri in uris_to_sync {
                                        if *shutdown_rx.borrow() {
                                            break;
                                        }
                                        if let Ok(snap) = fetch_user_profile(session, &uri).await {
                                            if account_uri
                                                .as_deref()
                                                .is_some_and(|own| own.eq_ignore_ascii_case(&uri))
                                                && let Some(username) = &current_account
                                            {
                                                let display_name = if snap.user_name.trim().is_empty() {
                                                    username.clone()
                                                } else {
                                                    snap.user_name.clone()
                                                };
                                                let updated = AccountProfile {
                                                    user_uri: uri.clone(),
                                                    username: username.clone(),
                                                    display_name: display_name.clone(),
                                                    image_url: snap.image_url.clone(),
                                                };
                                                if let Some(obs) = own_observer.as_ref() {
                                                    obs.update_profile(
                                                        display_name,
                                                        snap.image_url.clone(),
                                                    );
                                                }
                                                account_profile = Some(updated.clone());
                                                live.set_account_profile(Some(updated)).await;
                                            }
                                            let db_path_clone = db_path.clone();
                                            let _ = tokio::task::spawn_blocking(move || -> Result<()> {
                                                let db = Database::open(&db_path_clone)?;
                                                let _ = db.upsert_profile(&snap)?;
                                                Ok(())
                                            })
                                            .await;
                                        }
                                    }
                                    last_profile_sync = Some(Instant::now());
                                }
                            }
                            ActivityEvent::FriendUpdated {
                                activity,
                                transport,
                            } => {
                                let now_ms = Utc::now().timestamp_millis();
                                current_transport = Some(transport.as_str().to_string());
                                known_friend_uris.insert(activity.user.uri.clone());
                                friends_map.insert(activity.user.uri.clone(), activity.clone());

                                if log_to_db {
                                    log_activities_with_state(
                                        &db_path,
                                        vec![activity],
                                        &mut active_plays,
                                        now_ms,
                                        grace_ms,
                                        false,
                                    )
                                    .await;
                                }

                                let sorted =
                                    build_sorted_live_friends(&friends_map, account_uri.as_deref());
                                live.publish(LiveSnapshot {
                                    connected: true,
                                    status: "live".to_string(),
                                    transport: current_transport.clone(),
                                    message: None,
                                    account: current_account.clone(),
                                    account_uri: account_uri.clone(),
                                    account_profile: account_profile.clone(),
                                    updated_at_ms: Some(now_ms),
                                    poll_interval_secs: interval_secs,
                                    friends: sorted,
                                })
                                .await;
                            }
                            ActivityEvent::Error {
                                message,
                                consecutive_errors,
                            } => {
                                if message.contains("No cached Spotify credentials") {
                                    let _ = stream_shutdown_tx.send(true);
                                    let _ = tokio::time::timeout(
                                        Duration::from_secs(2),
                                        stream_handle,
                                    )
                                    .await;
                                    close_open_sessions(&db_path, &mut active_plays).await;
                                    active_plays.clear();
                                    if let Some(obs) = own_observer.take() {
                                        obs.shutdown().await;
                                    }
                                    if let Some(session) = own_session {
                                        session.shutdown();
                                    }
                                    live.set_account_profile(None).await;
                                    live.publish(LiveSnapshot {
                                        connected: false,
                                        status: "unauthenticated".to_string(),
                                        transport: None,
                                        message: Some(
                                            "Connect your Spotify account to enable live activity"
                                                .to_string(),
                                        ),
                                        account: None,
                                        account_uri: None,
                                        account_profile: None,
                                        updated_at_ms: Some(Utc::now().timestamp_millis()),
                                        poll_interval_secs: interval_secs,
                                        friends: Vec::new(),
                                    })
                                    .await;
                                    break;
                                }

                                eprintln!(
                                    "[warn] Live feed stream error (#{consecutive_errors}): {message}"
                                );
                                let prev = live.snapshot().await;
                                live.publish(LiveSnapshot {
                                    connected: false,
                                    status: "error".to_string(),
                                    transport: prev.transport,
                                    message: Some(format!("Stream error: {message}")),
                                    account: prev.account.or_else(|| current_account.clone()),
                                    account_uri: prev.account_uri.or_else(|| account_uri.clone()),
                                    account_profile: prev
                                        .account_profile
                                        .or_else(|| account_profile.clone()),
                                    updated_at_ms: prev.updated_at_ms,
                                    poll_interval_secs: interval_secs,
                                    friends: prev.friends,
                                })
                                .await;
                            }
                        }
                    }
                }
            }
        }
    })
}

pub fn build_router(db_path: PathBuf, live: Arc<LiveHub>) -> Router {
    let cache_dir = db_path
        .parent()
        .map(|p| p.join("cache"))
        .unwrap_or_else(|| PathBuf::from("cache"));
    build_router_with_cache(db_path, cache_dir, live)
}

pub fn build_router_with_cache(db_path: PathBuf, cache_dir: PathBuf, live: Arc<LiveHub>) -> Router {
    let account_profile = live.account_profile_handle();
    let auth_in_progress = live.auth_in_progress_handle();
    let state = AppState {
        db_path: Arc::new(db_path),
        cache_dir: Arc::new(cache_dir),
        live,
        account_profile,
        auth_in_progress,
    };

    let api_router = Router::new()
        .route("/overview", get(api_overview))
        .route("/friends", get(api_friends))
        .route("/history", get(api_history))
        .route("/contexts", get(api_contexts))
        .route("/rebuild", get(api_rebuild))
        .route("/overlap", get(api_overlap))
        .route("/profiles", get(api_profiles))
        .route("/changes", get(api_changes))
        .route("/live", get(api_live))
        .route("/live/stream", get(api_live_stream))
        .route("/live/refresh", post(api_live_refresh))
        .route("/auth/status", get(api_auth_status))
        .route("/auth/login", post(api_auth_login))
        .route("/auth/logout", post(api_auth_logout))
        .fallback(api_not_found);

    Router::new()
        .nest("/api", api_router)
        .fallback(serve_static)
        .with_state(state)
}

pub async fn start_embedded_server(config: UiConfig) -> anyhow::Result<EmbeddedServerHandle> {
    // Ensure database file and schema are initialized up front
    let _ = Database::open(&config.db_path)?;

    let ip: IpAddr = config
        .host
        .parse()
        .with_context(|| format!("Invalid bind host IP address: '{}'", config.host))?;
    let bind_addr = SocketAddr::new(ip, config.port);

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("Failed to bind web server on {bind_addr}"))?;
    let addr = listener
        .local_addr()
        .context("Failed to read local address for embedded web server")?;

    let poll_interval_secs = config.poll_interval_secs.max(MIN_POLL_INTERVAL_SECS);
    let live = Arc::new(LiveHub::new(poll_interval_secs, config.offline));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let stream_handle = if !config.offline {
        Some(spawn_live_stream(
            Arc::clone(&live),
            config.cache_dir.clone(),
            config.db_path.clone(),
            poll_interval_secs,
            config.transport,
            config.debug_push,
            config.log_to_db,
            shutdown_rx.clone(),
        ))
    } else {
        None
    };

    let app = build_router_with_cache(config.db_path, config.cache_dir, live);
    let mut server_shutdown_rx = shutdown_rx;
    let task = tokio::spawn(async move {
        let shutdown_fut = async move {
            if *server_shutdown_rx.borrow() {
                return;
            }
            let _ = server_shutdown_rx.changed().await;
        };

        if let Err(err) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_fut)
            .await
        {
            eprintln!("[error] Embedded web server exited with error: {err:#}");
        }

        if let Some(handle) = stream_handle {
            let _ = tokio::time::timeout(Duration::from_secs(3), handle).await;
        }
    });

    Ok(EmbeddedServerHandle {
        addr,
        shutdown_tx,
        task,
    })
}

pub async fn run_server(config: UiConfig) -> Result<()> {
    let open_browser = config.open_browser;
    let offline = config.offline;
    let log_to_db = config.log_to_db;
    let transport = config.transport;
    let poll_interval_secs = config.poll_interval_secs.max(MIN_POLL_INTERVAL_SECS);

    let handle = start_embedded_server(config).await?;
    let ip = handle.addr.ip();
    let port = handle.addr.port();

    let display_host = match ip {
        IpAddr::V4(v4) if v4.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(v6) if v6.is_unspecified() => "[::1]".to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
        IpAddr::V4(v4) => v4.to_string(),
    };
    let url = format!("http://{display_host}:{port}");
    println!("bside UI running at {url}");
    if offline {
        println!("Mode: offline archive only");
    } else {
        println!(
            "Mode: {} (SQLite logging: {})",
            format_transport_banner(transport, poll_interval_secs),
            if log_to_db { "on" } else { "off" }
        );
    }
    println!("Press Ctrl+C to stop.");

    if open_browser && let Err(err) = open::that(&url) {
        eprintln!("Could not open browser automatically: {err}");
    }

    shutdown_signal().await;
    handle.shutdown().await;

    println!("\nbside UI stopped.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::body::Body;
    use axum::http::{Method, Request};
    use chrono::Utc;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::spotify::{
        FriendActivity, FriendTrack, FriendUser, NamedUri, PublicPlaylist, TrackContext,
        TransportMode, UserProfileSnapshot,
    };

    fn make_temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bside-web-test-{label}-{}-{nanos}.db",
            std::process::id()
        ))
    }

    fn seed_sample_db(path: &Path) {
        let db = Database::open(path).expect("open db");
        let now_ms = Utc::now().timestamp_millis();

        let alice_track_1 = FriendActivity {
            timestamp: now_ms - 60_000,
            playing: true,
            user: FriendUser {
                uri: "spotify:user:alice".to_string(),
                name: "Alice".to_string(),
                image_url: Some("https://example.com/alice.jpg".to_string()),
            },
            track: FriendTrack {
                uri: "spotify:track:111".to_string(),
                name: "Midnight City".to_string(),
                image_url: Some("https://example.com/cover1.jpg".to_string()),
                album: Some(NamedUri {
                    uri: "spotify:album:a1".to_string(),
                    name: "Hurry Up, We're Dreaming".to_string(),
                }),
                artist: Some(NamedUri {
                    uri: "spotify:artist:m83".to_string(),
                    name: "M83".to_string(),
                }),
                artists: vec![NamedUri {
                    uri: "spotify:artist:m83".to_string(),
                    name: "M83".to_string(),
                }],
                context: Some(TrackContext {
                    uri: "spotify:user:alice:collection".to_string(),
                    name: "Liked Songs".to_string(),
                    index: None,
                    context_type: "liked_songs".to_string(),
                    owner: Some("Alice".to_string()),
                    is_private: true,
                    image_url: None,
                }),
                item_type: "track".to_string(),
                duration_ms: Some(243_000),
                popularity: Some(78),
                explicit: Some(false),
                track_number: Some(2),
                disc_number: Some(1),
                isrc: Some("FR1234567890".to_string()),
                album_type: Some("album".to_string()),
                release_date: Some("2011-10-18".to_string()),
                album_label: Some("Naïve".to_string()),
                cover_url: Some("https://example.com/cover1.jpg".to_string()),
            },
        };

        let bob_track_1 = FriendActivity {
            timestamp: now_ms - 120_000,
            playing: false,
            user: FriendUser {
                uri: "spotify:user:bob".to_string(),
                name: "Bob".to_string(),
                image_url: None,
            },
            track: FriendTrack {
                uri: "spotify:track:111".to_string(),
                name: "Midnight City".to_string(),
                image_url: Some("https://example.com/cover1.jpg".to_string()),
                album: Some(NamedUri {
                    uri: "spotify:album:a1".to_string(),
                    name: "Hurry Up, We're Dreaming".to_string(),
                }),
                artist: Some(NamedUri {
                    uri: "spotify:artist:m83".to_string(),
                    name: "M83".to_string(),
                }),
                artists: vec![NamedUri {
                    uri: "spotify:artist:m83".to_string(),
                    name: "M83".to_string(),
                }],
                context: Some(TrackContext {
                    uri: "spotify:playlist:pub1".to_string(),
                    name: "Synthwave Night".to_string(),
                    index: None,
                    context_type: "playlist".to_string(),
                    owner: Some("Bob".to_string()),
                    is_private: false,
                    image_url: None,
                }),
                item_type: "track".to_string(),
                duration_ms: Some(243_000),
                popularity: Some(78),
                explicit: Some(false),
                track_number: Some(2),
                disc_number: Some(1),
                isrc: Some("FR1234567890".to_string()),
                album_type: Some("album".to_string()),
                release_date: Some("2011-10-18".to_string()),
                album_label: Some("Naïve".to_string()),
                cover_url: Some("https://example.com/cover1.jpg".to_string()),
            },
        };

        db.insert_listen(&alice_track_1).expect("insert alice");
        db.insert_listen(&bob_track_1).expect("insert bob");

        let profile_v1 = UserProfileSnapshot {
            user_uri: "spotify:user:alice".to_string(),
            user_name: "Alice".to_string(),
            image_url: Some("https://example.com/alice.jpg".to_string()),
            color: Some(123456),
            followers_count: Some(10),
            following_count: Some(20),
            recently_played_artists: vec![NamedUri {
                uri: "spotify:artist:m83".to_string(),
                name: "M83".to_string(),
            }],
            public_playlists: vec![PublicPlaylist {
                uri: "spotify:playlist:p1".to_string(),
                name: "Late Night".to_string(),
                owner_name: Some("Alice".to_string()),
                owner_uri: Some("spotify:user:alice".to_string()),
                followers_count: Some(3),
                image_url: None,
            }],
        };
        let mut profile_v2 = profile_v1.clone();
        profile_v2.followers_count = Some(12);
        profile_v2.public_playlists.push(PublicPlaylist {
            uri: "spotify:playlist:p2".to_string(),
            name: "Morning Coffee".to_string(),
            owner_name: Some("Alice".to_string()),
            owner_uri: Some("spotify:user:alice".to_string()),
            followers_count: Some(1),
            image_url: None,
        });

        db.upsert_profile(&profile_v1).expect("upsert v1");
        db.upsert_profile(&profile_v2).expect("upsert v2");
    }

    async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .expect("router response");
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let json: Value = serde_json::from_slice(&bytes).expect("valid json");
        (status, json)
    }

    async fn post_json(app: &Router, uri: &str) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router response");
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let json: Value = serde_json::from_slice(&bytes).expect("valid json");
        (status, json)
    }

    #[tokio::test]
    async fn test_api_endpoints_return_valid_json() {
        let db_path = make_temp_db_path("all-endpoints");
        seed_sample_db(&db_path);
        let live = Arc::new(LiveHub::new(15, true));
        let app = build_router(db_path.clone(), Arc::clone(&live));

        let (status, overview) = get_json(&app, "/api/overview?friend=&days=30").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(overview["stats"]["total_plays"], 2);
        assert_eq!(overview["friends_summary"].as_array().unwrap().len(), 2);
        assert_eq!(overview["recent_listens"].as_array().unwrap().len(), 2);

        let (status, friends) = get_json(&app, "/api/friends").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(friends.as_array().unwrap().len(), 2);
        assert_eq!(friends[0]["user_name"], "Alice");
        assert_eq!(friends[0]["is_live"], true);

        let (status, history) =
            get_json(&app, "/api/history?friend=alice&days=30&limit=50&offset=0&q=midnight").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(history["total"], 1);
        assert_eq!(history["limit"], 50);
        assert_eq!(history["offset"], 0);
        assert_eq!(history["has_more"], false);
        let hist_arr = history["items"].as_array().unwrap();
        assert_eq!(hist_arr.len(), 1);
        assert_eq!(
            hist_arr[0]["spotify_url"],
            "https://open.spotify.com/track/111"
        );
        assert_eq!(
            hist_arr[0]["user_image_url"],
            "https://example.com/alice.jpg"
        );

        let (status, multi_history) =
            get_json(&app, "/api/history?users=spotify:user:alice,spotify:user:bob&limit=1&offset=0")
                .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(multi_history["total"], 2);
        assert_eq!(multi_history["items"].as_array().unwrap().len(), 1);
        assert_eq!(multi_history["has_more"], true);

        let (status, contexts) = get_json(&app, "/api/contexts?private_only=true&days=30").await;
        assert_eq!(status, StatusCode::OK);
        let ctx_arr = contexts.as_array().unwrap();
        assert_eq!(ctx_arr.len(), 1);
        assert_eq!(ctx_arr[0]["is_private"], true);
        assert_eq!(ctx_arr[0]["listeners"].as_array().unwrap().len(), 1);

        let (status, rebuilt) =
            get_json(&app, "/api/rebuild?friend=alice&days=30&context=liked_songs").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rebuilt.as_array().unwrap().len(), 1);
        assert_eq!(rebuilt[0]["track_name"], "Midnight City");
        assert_eq!(rebuilt[0]["cover_url"], "https://example.com/cover1.jpg");

        let (status, overlap) = get_json(&app, "/api/overlap?days=30&friends=").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(overlap["friend_pairs"].as_array().unwrap().len(), 1);
        assert_eq!(overlap["shared_tracks"].as_array().unwrap().len(), 1);
        assert_eq!(overlap["shared_tracks"][0]["first_player_name"], "Bob");

        let (status, profiles) = get_json(&app, "/api/profiles?friend=alice").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(profiles.as_array().unwrap().len(), 1);
        assert_eq!(profiles[0]["public_playlists"].as_array().unwrap().len(), 2);
        assert_eq!(profiles[0]["is_self"], false);

        // Set connected account profile and verify is_self flags
        live.set_account_profile(Some(AccountProfile {
            user_uri: "spotify:user:alice".to_string(),
            username: "alice".to_string(),
            display_name: "Alice".to_string(),
            image_url: Some("https://example.com/alice.jpg".to_string()),
        }))
        .await;
        let (status, friends_with_self) = get_json(&app, "/api/friends").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(friends_with_self[0]["is_self"], true);
        assert_eq!(friends_with_self[1]["is_self"], false);
        let (status, profiles_with_self) = get_json(&app, "/api/profiles?users=spotify:user:alice").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(profiles_with_self[0]["is_self"], true);

        let (status, changes) = get_json(&app, "/api/changes?friend=alice&days=30&limit=20").await;
        assert_eq!(status, StatusCode::OK);
        assert!(changes.as_array().unwrap().len() >= 2);

        let (status, not_found) = get_json(&app, "/api/does-not-exist").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(not_found["error"].as_str().is_some());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_live_endpoints_offline_and_contract() {
        let db_path = make_temp_db_path("live-endpoints");
        let _ = Database::open(&db_path).expect("open db");
        let live = Arc::new(LiveHub::new(5, true));
        let app = build_router(db_path.clone(), Arc::clone(&live));

        let (status, snap) = get_json(&app, "/api/live").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(snap["connected"], false);
        assert_eq!(snap["status"], "offline");
        assert_eq!(snap["transport"], "offline");
        assert_eq!(snap["message"], "Running in offline archive mode");
        assert!(snap["account"].is_null());
        assert!(snap["updated_at_ms"].is_null());
        // Minimum poll interval is clamped to 10 seconds
        assert_eq!(snap["poll_interval_secs"], 10);
        assert_eq!(snap["friends"].as_array().unwrap().len(), 0);

        let (status, refreshed) = post_json(&app, "/api/live/refresh").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(refreshed["connected"], false);
        assert_eq!(refreshed["status"], "offline");
        assert_eq!(refreshed["transport"], "offline");
        assert_eq!(refreshed["message"], "Running in offline archive mode");

        // Publish a live snapshot with mixed playing/paused items and verify sorting + JSON keys
        let mut items = vec![
            LiveFriendItem::from(&FriendActivity {
                timestamp: 3000,
                playing: false,
                user: FriendUser {
                    uri: "spotify:user:bob".to_string(),
                    name: "Bob".to_string(),
                    image_url: None,
                },
                track: FriendTrack {
                    uri: "spotify:track:bob1".to_string(),
                    name: "Paused Newer".to_string(),
                    artist: Some(NamedUri {
                        uri: "spotify:artist:1".to_string(),
                        name: "Artist B".to_string(),
                    }),
                    ..Default::default()
                },
            }),
            LiveFriendItem::from(&FriendActivity {
                timestamp: 1000,
                playing: true,
                user: FriendUser {
                    uri: "spotify:user:carol".to_string(),
                    name: "Carol".to_string(),
                    image_url: None,
                },
                track: FriendTrack {
                    uri: "spotify:track:carol1".to_string(),
                    name: "Playing Older".to_string(),
                    artist: Some(NamedUri {
                        uri: "spotify:artist:2".to_string(),
                        name: "Artist C".to_string(),
                    }),
                    ..Default::default()
                },
            }),
            LiveFriendItem::from(&FriendActivity {
                timestamp: 2000,
                playing: true,
                user: FriendUser {
                    uri: "spotify:user:alice".to_string(),
                    name: "Alice".to_string(),
                    image_url: Some("https://example.com/alice.jpg".to_string()),
                },
                track: FriendTrack {
                    uri: "spotify:track:alice1".to_string(),
                    name: "Playing Newer".to_string(),
                    album: Some(NamedUri {
                        uri: "spotify:album:a1".to_string(),
                        name: "Album A".to_string(),
                    }),
                    artists: vec![
                        NamedUri {
                            uri: "spotify:artist:a".to_string(),
                            name: "Artist A".to_string(),
                        },
                        NamedUri {
                            uri: "spotify:artist:f".to_string(),
                            name: "Feat".to_string(),
                        },
                    ],
                    context: Some(TrackContext {
                        uri: "spotify:user:alice:collection".to_string(),
                        name: "Liked Songs".to_string(),
                        index: None,
                        context_type: "liked_songs".to_string(),
                        owner: Some("Alice".to_string()),
                        is_private: true,
                        image_url: None,
                    }),
                    item_type: "track".to_string(),
                    duration_ms: Some(210_000),
                    popularity: Some(82),
                    explicit: Some(true),
                    release_date: Some("2024-05-01".to_string()),
                    cover_url: Some("https://example.com/cover.jpg".to_string()),
                    ..Default::default()
                },
            }),
        ];
        sort_live_friends(&mut items);

        live.publish(LiveSnapshot {
            connected: true,
            status: "live".to_string(),
            transport: Some(TransportMode::DealerPush.as_str().to_string()),
            message: None,
            account: Some("tester".to_string()),
            account_uri: Some("spotify:user:tester".to_string()),
            account_profile: Some(AccountProfile {
                user_uri: "spotify:user:tester".to_string(),
                username: "tester".to_string(),
                display_name: "Tester".to_string(),
                image_url: None,
            }),
            updated_at_ms: Some(9_999),
            poll_interval_secs: 15,
            friends: items,
        })
        .await;

        let (status, live_json) = get_json(&app, "/api/live").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(live_json["connected"], true);
        assert_eq!(live_json["status"], "live");
        assert_eq!(live_json["transport"], "dealer_push");
        assert_eq!(live_json["account"], "tester");
        assert_eq!(live_json["account_uri"], "spotify:user:tester");
        assert_eq!(live_json["account_profile"]["display_name"], "Tester");
        assert_eq!(live_json["updated_at_ms"], 9_999);

        let friends = live_json["friends"].as_array().unwrap();
        assert_eq!(friends.len(), 3);
        // Playing items first sorted by timestamp DESC, then non-playing sorted by timestamp DESC
        assert_eq!(friends[0]["user_name"], "Alice");
        assert_eq!(friends[1]["user_name"], "Carol");
        assert_eq!(friends[2]["user_name"], "Bob");

        // Verify all LiveFriendItem fields exist with expected values
        let alice = &friends[0];
        assert_eq!(alice["user_uri"], "spotify:user:alice");
        assert_eq!(alice["user_image_url"], "https://example.com/alice.jpg");
        assert_eq!(alice["playing"], true);
        assert_eq!(alice["timestamp_ms"], 2000);
        assert_eq!(alice["item_type"], "track");
        assert_eq!(alice["track_uri"], "spotify:track:alice1");
        assert_eq!(alice["track_name"], "Playing Newer");
        assert_eq!(alice["all_artists"], "Artist A, Feat");
        assert_eq!(alice["album_name"], "Album A");
        assert_eq!(alice["cover_url"], "https://example.com/cover.jpg");
        assert_eq!(alice["duration_ms"], 210_000);
        assert_eq!(alice["popularity"], 82);
        assert_eq!(alice["explicit"], true);
        assert_eq!(alice["release_date"], "2024-05-01");
        assert_eq!(alice["context_name"], "Liked Songs");
        assert_eq!(alice["context_uri"], "spotify:user:alice:collection");
        assert_eq!(alice["context_type"], "liked_songs");
        assert_eq!(alice["context_owner"], "Alice");
        assert_eq!(alice["context_is_private"], true);
        assert_eq!(alice["spotify_url"], "https://open.spotify.com/track/alice1");

        // Verify SSE endpoint emits initial snapshot event
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/live/stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("sse response");
        assert_eq!(resp.status(), StatusCode::OK);
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("text/event-stream"));

        let mut body_stream = resp.into_body().into_data_stream();
        let first_chunk = tokio::time::timeout(Duration::from_secs(2), body_stream.next())
            .await
            .expect("sse timeout")
            .expect("sse chunk")
            .expect("sse bytes");
        let chunk_text = String::from_utf8(first_chunk.to_vec()).expect("utf8");
        assert!(chunk_text.contains("event: snapshot"));
        assert!(chunk_text.contains("\"status\":\"live\""));
        assert!(chunk_text.contains("\"transport\":\"dealer_push\""));

        // Verify connecting LiveHub starts with null transport and accepts transport flips
        let connecting_hub = Arc::new(LiveHub::new(15, false));
        let connecting_app = build_router(db_path.clone(), Arc::clone(&connecting_hub));
        let (status, conn_json) = get_json(&connecting_app, "/api/live").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(conn_json["status"], "connecting");
        assert!(conn_json["transport"].is_null());

        let _cfg_check = UiConfig {
            cache_dir: db_path.clone(),
            db_path: db_path.clone(),
            host: "127.0.0.1".to_string(),
            port: 8787,
            open_browser: false,
            poll_interval_secs: 15,
            offline: true,
            log_to_db: false,
            transport: TransportPreference::Auto,
            debug_push: false,
        };

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_auth_endpoints_and_embedded_server_lifecycle() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "bside-embedded-test-{}-{nanos}",
            std::process::id()
        ));
        let cache_dir = temp_dir.join("cache");
        let db_path = temp_dir.join("history.db");
        std::fs::create_dir_all(&cache_dir).expect("create cache dir");

        let default_cfg = UiConfig::desktop_default();
        assert_eq!(default_cfg.host, "127.0.0.1");
        assert_eq!(default_cfg.port, 0);
        assert_eq!(default_cfg.poll_interval_secs, 15);
        assert!(default_cfg.log_to_db);
        assert!(!default_cfg.open_browser);
        assert!(!default_cfg.offline);

        // Router auth status + logout tests
        let _ = Database::open(&db_path).expect("open db");
        let live = Arc::new(LiveHub::new(15, false));
        let app = build_router_with_cache(db_path.clone(), cache_dir.clone(), Arc::clone(&live));

        let (status, auth_status) = get_json(&app, "/api/auth/status").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(auth_status["authenticated"], false);
        assert_eq!(auth_status["in_progress"], false);
        assert!(auth_status["account"].is_null());

        // Write dummy credentials.json and verify logout removes it and sets unauthenticated
        let cred_file = cache_dir.join("credentials.json");
        std::fs::write(&cred_file, b"{}").expect("write dummy cred");
        assert!(cred_file.exists());

        let (status, logout_resp) = post_json(&app, "/api/auth/logout").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(logout_resp["ok"], true);
        assert_eq!(logout_resp["authenticated"], false);
        assert!(!cred_file.exists());
        assert_eq!(live.snapshot().await.status, "unauthenticated");

        // Embedded server binds an ephemeral port and shuts down cleanly even with wakeable poller
        let handle = start_embedded_server(UiConfig {
            cache_dir: cache_dir.clone(),
            db_path: db_path.clone(),
            host: "127.0.0.1".to_string(),
            port: 0,
            open_browser: false,
            poll_interval_secs: 15,
            offline: false,
            log_to_db: true,
            transport: TransportPreference::Auto,
            debug_push: false,
        })
        .await
        .expect("start embedded server");
        assert_ne!(handle.addr.port(), 0);
        handle.shutdown().await;

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
