use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use rust_embed::Embed;
use serde::{Deserialize, Serialize};

use crate::db::{
    ContextSummary, Database, FriendSummary, ListenRecord, OverlapReport, ProfileEvent,
    RebuiltTrack, StatsSummary, StoredProfile, spotify_uri_to_url,
};

#[derive(Embed)]
#[folder = "web/dist"]
struct WebAssets;

#[derive(Clone)]
struct AppState {
    db_path: Arc<PathBuf>,
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

pub fn build_router(db_path: PathBuf) -> Router {
    let state = AppState {
        db_path: Arc::new(db_path),
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
        .fallback(api_not_found);

    Router::new()
        .nest("/api", api_router)
        .fallback(serve_static)
        .with_state(state)
}

pub async fn run_server(db_path: &Path, host: &str, port: u16, open_browser: bool) -> Result<()> {
    // Ensure database file and schema are initialized up front
    let _ = Database::open(db_path)?;

    let ip: IpAddr = host
        .parse()
        .with_context(|| format!("Invalid bind host IP address: '{host}'"))?;
    let addr = SocketAddr::new(ip, port);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind web server on {addr}"))?;

    let display_host = match ip {
        IpAddr::V4(v4) if v4.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(v6) if v6.is_unspecified() => "[::1]".to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
        IpAddr::V4(v4) => v4.to_string(),
    };
    let url = format!("http://{display_host}:{port}");
    println!("bside UI running at {url}");
    println!("Press Ctrl+C to stop.");

    if open_browser && let Err(err) = open::that(&url) {
        eprintln!("Could not open browser automatically: {err}");
    }

    let app = build_router(db_path.to_path_buf());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Web server exited with error")?;

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

    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::body::Body;
    use axum::http::Request;
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

    #[tokio::test]
    async fn test_api_endpoints_return_valid_json() {
        let db_path = make_temp_db_path("all-endpoints");
        seed_sample_db(&db_path);
        let app = build_router(db_path.clone());

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
}
