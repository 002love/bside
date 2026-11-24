use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
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
use crate::spotify::{FriendActivity, NameCache, connect_cached, fetch_buddylist, open_cache};

const MIN_POLL_INTERVAL_SECS: u64 = 10;
const MANUAL_REFRESH_COOLDOWN: Duration = Duration::from_secs(5);
const MANUAL_REFRESH_WAIT: Duration = Duration::from_secs(3);
const SSE_KEEP_ALIVE_SECS: u64 = 15;

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
    pub spotify_url: String,
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
            spotify_url: crate::format::spotify_uri_to_url(&a.track.uri),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveSnapshot {
    pub connected: bool,
    pub status: String, // "live" | "connecting" | "offline" | "unauthenticated" | "error"
    pub message: Option<String>,
    pub account: Option<String>,
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
    tx: broadcast::Sender<LiveSnapshot>,
    refresh_notify: Arc<Notify>,
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
                message: Some("Running in offline archive mode".to_string()),
                account: None,
                updated_at_ms: None,
                poll_interval_secs: interval,
                friends: Vec::new(),
            }
        } else {
            LiveSnapshot {
                connected: false,
                status: "connecting".to_string(),
                message: Some("Connecting to Spotify...".to_string()),
                account: None,
                updated_at_ms: None,
                poll_interval_secs: interval,
                friends: Vec::new(),
            }
        };

        let (tx, _rx) = broadcast::channel(32);
        Self {
            latest: RwLock::new(initial),
            tx,
            refresh_notify: Arc::new(Notify::new()),
            last_manual_refresh: Mutex::new(None),
            offline,
        }
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
    live: Arc<LiveHub>,
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
}

impl From<ListenRecord> for ApiListenRecord {
    fn from(record: ListenRecord) -> Self {
        let spotify_url = spotify_uri_to_url(&record.track_uri);
        Self {
            record,
            spotify_url,
        }
    }
}

#[derive(Debug, Serialize)]
struct OverviewResponse {
    stats: StatsSummary,
    friends_summary: Vec<FriendSummary>,
    recent_listens: Vec<ApiListenRecord>,
}

#[derive(Debug, Deserialize)]
struct OverviewQuery {
    friend: Option<String>,
    days: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    friend: Option<String>,
    days: Option<String>,
    limit: Option<String>,
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContextsQuery {
    friend: Option<String>,
    private_only: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RebuildQuery {
    friend: Option<String>,
    context: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OverlapQuery {
    days: Option<String>,
    friends: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProfilesQuery {
    friend: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChangesQuery {
    friend: Option<String>,
    days: Option<String>,
    limit: Option<String>,
}

fn clean_opt_str(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn parse_opt_i64(v: Option<String>) -> Option<i64> {
    clean_opt_str(v).and_then(|s| s.parse::<i64>().ok()).filter(|&n| n > 0)
}

fn parse_opt_usize(v: Option<String>) -> Option<usize> {
    clean_opt_str(v).and_then(|s| s.parse::<usize>().ok()).filter(|&n| n > 0)
}

fn parse_bool(v: Option<String>) -> bool {
    clean_opt_str(v)
        .map(|s| matches!(s.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
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
    let friend = clean_opt_str(query.friend);
    let days = parse_opt_i64(query.days);

    let data = run_db(state.db_path, move |db| {
        let stats = db.query_stats(friend.as_deref(), days)?;
        let friends_summary = db.query_friends_summary()?;
        let recent_listens = db
            .query_history_search(friend.as_deref(), days, Some(10), None)?
            .into_iter()
            .map(ApiListenRecord::from)
            .collect();
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
) -> Result<Json<Vec<FriendSummary>>, (StatusCode, Json<ApiError>)> {
    let friends = run_db(state.db_path, |db| db.query_friends_summary()).await?;
    Ok(Json(friends))
}

async fn api_history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<ApiListenRecord>>, (StatusCode, Json<ApiError>)> {
    let friend = clean_opt_str(query.friend);
    let days = parse_opt_i64(query.days);
    let limit = parse_opt_usize(query.limit).unwrap_or(200).min(1000);
    let q = clean_opt_str(query.q);

    let rows = run_db(state.db_path, move |db| {
        let records = db.query_history_search(
            friend.as_deref(),
            days,
            Some(limit),
            q.as_deref(),
        )?;
        Ok(records.into_iter().map(ApiListenRecord::from).collect())
    })
    .await?;

    Ok(Json(rows))
}

async fn api_contexts(
    State(state): State<AppState>,
    Query(query): Query<ContextsQuery>,
) -> Result<Json<Vec<ContextSummary>>, (StatusCode, Json<ApiError>)> {
    let friend = clean_opt_str(query.friend);
    let private_only = parse_bool(query.private_only);

    let rows = run_db(state.db_path, move |db| {
        db.query_contexts(friend.as_deref(), private_only)
    })
    .await?;

    Ok(Json(rows))
}

async fn api_rebuild(
    State(state): State<AppState>,
    Query(query): Query<RebuildQuery>,
) -> Result<Json<Vec<RebuiltTrack>>, (StatusCode, Json<ApiError>)> {
    let friend = clean_opt_str(query.friend);
    let Some(context) = clean_opt_str(query.context) else {
        return Ok(Json(Vec::new()));
    };

    let rows = run_db(state.db_path, move |db| {
        db.rebuild_playlist(friend.as_deref(), &context)
    })
    .await?;

    Ok(Json(rows))
}

async fn api_overlap(
    State(state): State<AppState>,
    Query(query): Query<OverlapQuery>,
) -> Result<Json<OverlapReport>, (StatusCode, Json<ApiError>)> {
    let days = parse_opt_i64(query.days);
    let friends: Vec<String> = clean_opt_str(query.friends)
        .map(|s| {
            s.split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let report = run_db(state.db_path, move |db| db.query_overlap(days, &friends)).await?;
    Ok(Json(report))
}

async fn api_profiles(
    State(state): State<AppState>,
    Query(query): Query<ProfilesQuery>,
) -> Result<Json<Vec<StoredProfile>>, (StatusCode, Json<ApiError>)> {
    let friend = clean_opt_str(query.friend);
    let profiles = run_db(state.db_path, move |db| db.query_profiles(friend.as_deref())).await?;
    Ok(Json(profiles))
}

async fn api_changes(
    State(state): State<AppState>,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<Vec<ProfileEvent>>, (StatusCode, Json<ApiError>)> {
    let friend = clean_opt_str(query.friend);
    let days = parse_opt_i64(query.days);
    let limit = parse_opt_usize(query.limit).unwrap_or(100).min(1000);

    let events = run_db(state.db_path, move |db| {
        db.query_profile_events(friend.as_deref(), days, Some(limit))
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

fn spawn_live_poller(
    live: Arc<LiveHub>,
    cache_dir: PathBuf,
    db_path: PathBuf,
    poll_interval_secs: u64,
    log_to_db: bool,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let interval_secs = poll_interval_secs.max(MIN_POLL_INTERVAL_SECS);

    tokio::spawn(async move {
        let has_credentials = open_cache(&cache_dir)
            .ok()
            .and_then(|cache| cache.credentials())
            .is_some();

        if !has_credentials {
            live.publish(LiveSnapshot {
                connected: false,
                status: "unauthenticated".to_string(),
                message: Some("Run `bside login` to enable live feed".to_string()),
                account: None,
                updated_at_ms: Some(Utc::now().timestamp_millis()),
                poll_interval_secs: interval_secs,
                friends: Vec::new(),
            })
            .await;
            return;
        }

        let mut name_cache = NameCache::default();
        let mut connect_errors: u32 = 0;

        loop {
            if *shutdown_rx.borrow() {
                return;
            }

            let session = match connect_cached(&cache_dir).await {
                Ok(session) => {
                    connect_errors = 0;
                    session
                }
                Err(err) => {
                    let err_msg = format!("{err:#}");
                    if err_msg.contains("No cached Spotify credentials") {
                        live.publish(LiveSnapshot {
                            connected: false,
                            status: "unauthenticated".to_string(),
                            message: Some("Run `bside login` to enable live feed".to_string()),
                            account: None,
                            updated_at_ms: Some(Utc::now().timestamp_millis()),
                            poll_interval_secs: interval_secs,
                            friends: Vec::new(),
                        })
                        .await;
                        return;
                    }

                    connect_errors = connect_errors.saturating_add(1);
                    eprintln!("[warn] Live feed connect failed (#{connect_errors}): {err_msg}");
                    let prev = live.snapshot().await;
                    live.publish(LiveSnapshot {
                        connected: false,
                        status: "error".to_string(),
                        message: Some(format!("Connection failed: {err_msg}")),
                        account: prev.account,
                        updated_at_ms: prev.updated_at_ms,
                        poll_interval_secs: interval_secs,
                        friends: prev.friends,
                    })
                    .await;

                    let backoff_secs = (5u64 * (1u64 << connect_errors.min(5))).min(300);
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                        _ = live.refresh_notify.notified() => {}
                        _ = shutdown_rx.changed() => {
                            return;
                        }
                    }
                    continue;
                }
            };

            let account = {
                let username = session.username();
                if username.is_empty() {
                    None
                } else {
                    Some(username)
                }
            };
            let mut consecutive_errors: u32 = 0;

            loop {
                if *shutdown_rx.borrow() {
                    session.shutdown();
                    return;
                }

                if session.is_invalid() {
                    let prev = live.snapshot().await;
                    live.publish(LiveSnapshot {
                        connected: false,
                        status: "connecting".to_string(),
                        message: Some("Reconnecting to Spotify...".to_string()),
                        account: account.clone(),
                        updated_at_ms: prev.updated_at_ms,
                        poll_interval_secs: interval_secs,
                        friends: prev.friends,
                    })
                    .await;
                    break;
                }

                match fetch_buddylist(&session, &mut name_cache).await {
                    Ok(activities) => {
                        consecutive_errors = 0;

                        if log_to_db && !activities.is_empty() {
                            let db_path_clone = db_path.clone();
                            let activities_to_log = activities.clone();
                            let log_res = tokio::task::spawn_blocking(move || -> Result<()> {
                                let db = Database::open(&db_path_clone)?;
                                for item in &activities_to_log {
                                    db.insert_listen(item)?;
                                }
                                Ok(())
                            })
                            .await;
                            match log_res {
                                Ok(Ok(())) => {}
                                Ok(Err(err)) => {
                                    eprintln!("[warn] Failed to log live listens to SQLite: {err:#}");
                                }
                                Err(err) => {
                                    eprintln!("[warn] Live DB logging task panicked: {err}");
                                }
                            }
                        }

                        let mut friends: Vec<LiveFriendItem> =
                            activities.iter().map(LiveFriendItem::from).collect();
                        sort_live_friends(&mut friends);

                        live.publish(LiveSnapshot {
                            connected: true,
                            status: "live".to_string(),
                            message: None,
                            account: account.clone(),
                            updated_at_ms: Some(Utc::now().timestamp_millis()),
                            poll_interval_secs: interval_secs,
                            friends,
                        })
                        .await;
                    }
                    Err(err) => {
                        consecutive_errors = consecutive_errors.saturating_add(1);
                        let err_msg = format!("{err:#}");
                        eprintln!("[warn] Live feed poll failed (#{consecutive_errors}): {err_msg}");

                        let prev = live.snapshot().await;
                        live.publish(LiveSnapshot {
                            connected: false,
                            status: "error".to_string(),
                            message: Some(format!("Poll failed: {err_msg}")),
                            account: account.clone(),
                            updated_at_ms: prev.updated_at_ms,
                            poll_interval_secs: interval_secs,
                            friends: prev.friends,
                        })
                        .await;

                        if consecutive_errors >= 3 {
                            session.shutdown();
                            let backoff_secs = (interval_secs * 2).min(120);
                            tokio::select! {
                                _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                                _ = live.refresh_notify.notified() => {}
                                _ = shutdown_rx.changed() => {
                                    return;
                                }
                            }
                            break;
                        }
                    }
                }

                let wait_secs = if consecutive_errors > 0 {
                    (interval_secs * (1u64 << consecutive_errors.min(4))).min(300)
                } else {
                    interval_secs
                };

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(wait_secs)) => {}
                    _ = live.refresh_notify.notified() => {}
                    _ = shutdown_rx.changed() => {
                        session.shutdown();
                        return;
                    }
                }
            }
        }
    })
}

pub fn build_router(db_path: PathBuf, live: Arc<LiveHub>) -> Router {
    let state = AppState {
        db_path: Arc::new(db_path),
        live,
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
        .fallback(api_not_found);

    Router::new()
        .nest("/api", api_router)
        .fallback(serve_static)
        .with_state(state)
}

pub async fn run_server(config: UiConfig) -> Result<()> {
    // Ensure database file and schema are initialized up front
    let _ = Database::open(&config.db_path)?;

    let ip: IpAddr = config
        .host
        .parse()
        .with_context(|| format!("Invalid bind host IP address: '{}'", config.host))?;
    let addr = SocketAddr::new(ip, config.port);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind web server on {addr}"))?;

    let poll_interval_secs = config.poll_interval_secs.max(MIN_POLL_INTERVAL_SECS);
    let live = Arc::new(LiveHub::new(poll_interval_secs, config.offline));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let poller_handle = if !config.offline {
        Some(spawn_live_poller(
            Arc::clone(&live),
            config.cache_dir.clone(),
            config.db_path.clone(),
            poll_interval_secs,
            config.log_to_db,
            shutdown_rx,
        ))
    } else {
        None
    };

    let display_host = match ip {
        IpAddr::V4(v4) if v4.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(v6) if v6.is_unspecified() => "[::1]".to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
        IpAddr::V4(v4) => v4.to_string(),
    };
    let url = format!("http://{display_host}:{}", config.port);
    println!("bside UI running at {url}");
    if config.offline {
        println!("Mode: offline archive only");
    } else {
        println!(
            "Mode: live feed every {poll_interval_secs}s (SQLite logging: {})",
            if config.log_to_db { "on" } else { "off" }
        );
    }
    println!("Press Ctrl+C to stop.");

    if config.open_browser && let Err(err) = open::that(&url) {
        eprintln!("Could not open browser automatically: {err}");
    }

    let app = build_router(config.db_path.clone(), live);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Web server exited with error")?;

    let _ = shutdown_tx.send(true);
    if let Some(handle) = poller_handle {
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

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
        UserProfileSnapshot,
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
        let app = build_router(db_path.clone(), live);

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
            get_json(&app, "/api/history?friend=alice&days=30&limit=50&q=midnight").await;
        assert_eq!(status, StatusCode::OK);
        let hist_arr = history.as_array().unwrap();
        assert_eq!(hist_arr.len(), 1);
        assert_eq!(
            hist_arr[0]["spotify_url"],
            "https://open.spotify.com/track/111"
        );

        let (status, contexts) = get_json(&app, "/api/contexts?private_only=true").await;
        assert_eq!(status, StatusCode::OK);
        let ctx_arr = contexts.as_array().unwrap();
        assert_eq!(ctx_arr.len(), 1);
        assert_eq!(ctx_arr[0]["is_private"], true);

        let (status, rebuilt) =
            get_json(&app, "/api/rebuild?friend=alice&context=liked_songs").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rebuilt.as_array().unwrap().len(), 1);
        assert_eq!(rebuilt[0]["track_name"], "Midnight City");

        let (status, overlap) = get_json(&app, "/api/overlap?days=30&friends=").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(overlap["friend_pairs"].as_array().unwrap().len(), 1);
        assert_eq!(overlap["shared_tracks"].as_array().unwrap().len(), 1);
        assert_eq!(overlap["shared_tracks"][0]["first_player_name"], "Bob");

        let (status, profiles) = get_json(&app, "/api/profiles?friend=alice").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(profiles.as_array().unwrap().len(), 1);
        assert_eq!(profiles[0]["public_playlists"].as_array().unwrap().len(), 2);

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
            message: None,
            account: Some("tester".to_string()),
            updated_at_ms: Some(9_999),
            poll_interval_secs: 15,
            friends: items,
        })
        .await;

        let (status, live_json) = get_json(&app, "/api/live").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(live_json["connected"], true);
        assert_eq!(live_json["status"], "live");
        assert_eq!(live_json["account"], "tester");
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

        let _ = std::fs::remove_file(&db_path);
    }
}
