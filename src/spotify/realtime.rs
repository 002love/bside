#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};
use http::{Method, Request};
use librespot_core::{
    dealer::{
        Subscription,
        protocol::{Message as DealerMessage, PayloadValue},
    },
    mercury::MercuryResponse,
    session::Session,
    spclient::CLIENT_TOKEN,
};
use protobuf::CodedInputStream;
use serde::Serialize;
use tokio::sync::{Notify, mpsc, watch};
use tokio::time::{Instant as TokioInstant, timeout};

use super::auth::connect_cached;
use super::feed::{enrich_live_entries, fetch_buddylist, normalize_legacy_friends, parse_live_feed};
use super::models::{
    BuddyListResponse, FriendActivity, LiveEntry, NameCache, canonical_spotify_dj_uri,
    is_spotify_dj_context, normalize_context_uri, user_id_from_uri,
};
use super::profile::json_str;

const DEALER_CONNECTIONS_URI: &str = "hm://pusher/v1/connections/";
const DEALER_PRESENCE2_URI: &str = "hm://presence2/user/";
const DEALER_PRESENCE_VIEW_URI: &str = "hm://presence-view/";
const DEALER_LISTENING_ACTIVITY_URI: &str = "hm://listening-activity/";
const LISTENING_ACTIVITY_SUBSCRIBE_PATH: &str = "/listening-activity/v1/subscribe";
const LISTENING_ACTIVITY_UNSUBSCRIBE_PATH: &str = "/listening-activity/v1/unsubscribe";

const DEALER_START_TIMEOUT: Duration = Duration::from_secs(15);
const DEALER_CONN_ID_TIMEOUT: Duration = Duration::from_secs(8);
const LISTENING_ACTIVITY_SUB_TIMEOUT: Duration = Duration::from_secs(10);
const LISTENING_ACTIVITY_UNSUB_TIMEOUT: Duration = Duration::from_secs(3);
const SUBSCRIPTION_RENEW_LEAD_SECS: u64 = 60;
const MIN_SUBSCRIPTION_RENEW_SECS: u64 = 60;
const FALLBACK_SUBSCRIPTION_RENEW_SECS: u64 = 600;

const MERCURY_MAX_SUBSCRIPTIONS: usize = 50;
const MERCURY_SUB_TIMEOUT: Duration = Duration::from_secs(4);
const MERCURY_SUB_CONCURRENCY: usize = 10;

const MIN_POLL_INTERVAL_SECS: u64 = 10;
const DEFAULT_RESYNC_INTERVAL_SECS: u64 = 60;
const MAX_POLL_BACKOFF_SECS: u64 = 600;
const PUSH_UPGRADE_INTERVAL: Duration = Duration::from_secs(300);
const INVALIDATE_DEBOUNCE: Duration = Duration::from_millis(350);
const RECONNECT_MAX_BACKOFF_SECS: u64 = 60;
const CONSECUTIVE_ERRORS_RECONNECT_THRESHOLD: u32 = 3;

const PLAUSIBLE_UNIX_MS_MIN: u64 = 1_400_000_000_000;
const PLAUSIBLE_UNIX_MS_MAX: u64 = 4_100_000_000_000;
const PLAUSIBLE_UNIX_SECS_MIN: u64 = 1_400_000_000;
const PLAUSIBLE_UNIX_SECS_MAX: u64 = 4_100_000_000;
const PLAUSIBLE_UNIX_MICROS_MIN: u64 = 1_400_000_000_000_000;
const PLAUSIBLE_UNIX_MICROS_MAX: u64 = 4_100_000_000_000_000;
const DEALER_STALE_RESYNC_THRESHOLD: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportMode {
    DealerPush,
    MercuryPush,
    HttpPolling,
}

impl TransportMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DealerPush => "dealer_push",
            Self::MercuryPush => "mercury_push",
            Self::HttpPolling => "http_polling",
        }
    }
}

impl std::fmt::Display for TransportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportPreference {
    #[default]
    Auto,
    Dealer,
    Mercury,
    Poll,
}

#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub poll_interval_secs: u64,
    pub resync_interval_secs: u64,
    pub transport: TransportPreference,
    pub debug_push: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 30,
            resync_interval_secs: DEFAULT_RESYNC_INTERVAL_SECS,
            transport: TransportPreference::Auto,
            debug_push: false,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ActivityEvent {
    FullSnapshot {
        friends: Vec<FriendActivity>,
        transport: TransportMode,
    },
    FriendUpdated {
        activity: FriendActivity,
        transport: TransportMode,
    },
    TransportChanged {
        mode: TransportMode,
        reason: Option<String>,
    },
    SessionReconnected {
        account: Option<String>,
    },
    Error {
        message: String,
        consecutive_errors: u32,
    },
}

#[derive(Debug)]
pub(crate) enum ParsedPushPayload {
    Entries(Vec<LiveEntry>),
    LegacyFriends(Vec<FriendActivity>),
    InvalidateHint,
}

/// RFC 3986 percent-encoder for a single URL path segment.
/// Leaves unreserved characters (`ALPHA / DIGIT / "-" / "." / "_" / "~"`) untouched
/// and percent-encodes every other byte using uppercase hexadecimal digits.
pub(crate) fn percent_encode_path_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            let _ = write!(&mut out, "%{byte:02X}");
        }
    }
    out
}

fn percent_decode_path_segment(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_invalidation_topic(uri: &str) -> bool {
    uri.starts_with(DEALER_PRESENCE_VIEW_URI)
        || uri == "hm://presence-view"
        || uri.starts_with(DEALER_LISTENING_ACTIVITY_URI)
        || uri == "hm://listening-activity"
}

fn infer_user_uri_from_topic(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix(DEALER_PRESENCE2_URI)?;
    let raw_username = rest.split(['/', '?']).next()?.trim();
    if raw_username.is_empty() {
        return None;
    }
    let decoded = percent_decode_path_segment(raw_username);
    let username = decoded.trim();
    if username.is_empty() {
        return None;
    }
    if username.starts_with("spotify:user:") {
        Some(username.to_string())
    } else {
        Some(format!("spotify:user:{username}"))
    }
}

fn normalize_plausible_unix_timestamp(v: u64) -> Option<i64> {
    if (PLAUSIBLE_UNIX_MS_MIN..=PLAUSIBLE_UNIX_MS_MAX).contains(&v) {
        Some(v as i64)
    } else if (PLAUSIBLE_UNIX_SECS_MIN..=PLAUSIBLE_UNIX_SECS_MAX).contains(&v) {
        Some((v as i64) * 1000)
    } else if (PLAUSIBLE_UNIX_MICROS_MIN..=PLAUSIBLE_UNIX_MICROS_MAX).contains(&v) {
        Some((v / 1000) as i64)
    } else {
        None
    }
}

fn parse_json_timestamp_ms(val: &serde_json::Value) -> Option<i64> {
    if let Some(n) = val.as_u64() {
        return normalize_plausible_unix_timestamp(n);
    }
    if let Some(s) = val.as_str() {
        let s = s.trim();
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp_millis());
        }
        if let Ok(n) = s.parse::<u64>() {
            return normalize_plausible_unix_timestamp(n);
        }
    }
    None
}

fn parse_single_friend_json(uri: &str, json: &serde_json::Value) -> Option<LiveEntry> {
    let obj = json.as_object()?;
    if obj.is_empty() {
        return None;
    }

    let activity = json.get("activity");

    let user_uri = json
        .get("user")
        .and_then(|u| u.get("uri"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with("spotify:user:") {
                s.to_string()
            } else {
                format!("spotify:user:{s}")
            }
        })
        .or_else(|| {
            json_str(json, &["user_uri", "userUri", "username"]).map(|s| {
                if s.starts_with("spotify:user:") {
                    s
                } else {
                    format!("spotify:user:{s}")
                }
            })
        })
        .or_else(|| json_str(json, &["uri"]).filter(|s| s.starts_with("spotify:user:")))
        .or_else(|| infer_user_uri_from_topic(uri))?;

    let track_uri = json
        .get("track")
        .and_then(|t| t.get("uri"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            activity.and_then(|a| {
                json_str(
                    a,
                    &["uri", "entityUri", "entity_uri", "trackUri", "track_uri"],
                )
            })
        })
        .or_else(|| json_str(json, &["track_uri", "trackUri", "entityUri", "entity_uri"]))
        .filter(|s| s.starts_with("spotify:track:") || s.starts_with("spotify:episode:"))?;

    let context_uri = json
        .get("track")
        .and_then(|t| t.get("context"))
        .and_then(|c| c.get("uri"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            json.get("context")
                .and_then(|c| c.get("uri"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .or_else(|| activity.and_then(|a| json_str(a, &["contextUri", "context_uri"])))
        .or_else(|| json_str(json, &["context_uri", "contextUri"]))
        .map(|s| normalize_context_uri(&s))
        .filter(|s| !s.is_empty())
        .map(|uri| {
            if is_spotify_dj_context(Some(&uri), None) {
                canonical_spotify_dj_uri(&user_uri)
            } else {
                uri
            }
        });

    let timestamp_ms = json
        .get("timestamp")
        .or_else(|| json.get("timestamp_ms"))
        .or_else(|| json.get("timestampMs"))
        .or_else(|| activity.and_then(|a| a.get("timestamp")))
        .and_then(parse_json_timestamp_ms)
        .unwrap_or_else(|| Utc::now().timestamp_millis());

    // Proto3 JSON in `listening-activity/v1` omits `isPlaying` when false; legacy `presence2`
    // messages omit it because they only fire on play starts.
    let default_playing = activity.is_none() && !uri.starts_with(DEALER_LISTENING_ACTIVITY_URI);
    let playing = json
        .get("playing")
        .or_else(|| json.get("isPlaying"))
        .or_else(|| json.get("is_playing"))
        .or_else(|| {
            activity.and_then(|a| {
                a.get("isPlaying")
                    .or_else(|| a.get("playing"))
                    .or_else(|| a.get("is_playing"))
            })
        })
        .and_then(|v| v.as_bool())
        .unwrap_or(default_playing);

    Some(LiveEntry {
        user_uri,
        track_uri,
        context_uri,
        playing,
        timestamp_ms,
    })
}

fn is_context_uri(s: &str) -> bool {
    s.starts_with("spotify:playlist:")
        || s.starts_with("spotify:album:")
        || s.starts_with("spotify:artist:")
        || s.starts_with("spotify:show:")
        || s.starts_with("spotify:list:")
        || s.eq_ignore_ascii_case("spotify:dj")
        || (s.starts_with("spotify:user:")
            && (s.contains(":collection")
                || s.contains(":playlist:")
                || s.ends_with(":dj")
                || s.contains(":dj:")))
}

#[derive(Default)]
struct ProtoScanResult {
    user_uri: Option<String>,
    track_uri: Option<String>,
    context_uri: Option<String>,
    timestamp_ms: Option<i64>,
}

fn scan_protobuf_fields(bytes: &[u8], depth: usize, out: &mut ProtoScanResult) -> bool {
    if depth > 4 || bytes.is_empty() {
        return false;
    }

    let mut is = CodedInputStream::from_bytes(bytes);
    let mut parsed_any = false;
    loop {
        match is.read_raw_tag_or_eof() {
            Ok(None) => return parsed_any,
            Err(_) => return false,
            Ok(Some(tag)) => {
                let field_number = tag >> 3;
                if field_number == 0 {
                    return false;
                }
                parsed_any = true;
                let wire_type = tag & 0x7;
                match wire_type {
                    0 => {
                        let Ok(v) = is.read_raw_varint64() else {
                            return false;
                        };
                        if out.timestamp_ms.is_none()
                            && let Some(ts) = normalize_plausible_unix_timestamp(v)
                        {
                            out.timestamp_ms = Some(ts);
                        }
                    }
                    1 => {
                        let Ok(v) = is.read_raw_little_endian64() else {
                            return false;
                        };
                        if out.timestamp_ms.is_none()
                            && let Some(ts) = normalize_plausible_unix_timestamp(v)
                        {
                            out.timestamp_ms = Some(ts);
                        }
                    }
                    2 => {
                        let Ok(buf) = is.read_bytes() else {
                            return false;
                        };
                        if let Ok(s) = std::str::from_utf8(&buf) {
                            let s = s.trim();
                            if s.starts_with("spotify:track:") || s.starts_with("spotify:episode:")
                            {
                                if out.track_uri.is_none() {
                                    out.track_uri = Some(s.to_string());
                                }
                                continue;
                            }
                            if is_context_uri(s) {
                                if out.context_uri.is_none() {
                                    out.context_uri = Some(normalize_context_uri(s));
                                }
                                continue;
                            }
                            if s.starts_with("spotify:user:")
                                && !s.contains(":collection")
                                && !s.contains(":playlist:")
                                && !s.ends_with(":dj")
                                && !s.contains(":dj:")
                            {
                                if out.user_uri.is_none() {
                                    out.user_uri = Some(s.to_string());
                                }
                                continue;
                            }
                        }

                        // Only merge nested length-delimited bytes if the buffer is itself a
                        // syntactically valid protobuf submessage from start to EOF.
                        if depth < 4 && buf.len() >= 2 {
                            let mut nested = ProtoScanResult::default();
                            if scan_protobuf_fields(&buf, depth + 1, &mut nested) {
                                if out.user_uri.is_none() {
                                    out.user_uri = nested.user_uri;
                                }
                                if out.track_uri.is_none() {
                                    out.track_uri = nested.track_uri;
                                }
                                if out.context_uri.is_none() {
                                    out.context_uri = nested.context_uri;
                                }
                                if out.timestamp_ms.is_none() {
                                    out.timestamp_ms = nested.timestamp_ms;
                                }
                            }
                        }
                    }
                    5 => {
                        if is.read_raw_little_endian32().is_err() {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
        }
    }
}

pub(crate) fn parse_push_payload(uri: &str, payload: &[u8]) -> Option<ParsedPushPayload> {
    let trimmed = payload.trim_ascii();
    if trimmed.is_empty() || trimmed == b"{}" || trimmed == b"null" {
        return is_invalidation_topic(uri).then_some(ParsedPushPayload::InvalidateHint);
    }

    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(trimmed) {
        if let Some(obj) = json.as_object()
            && obj.is_empty()
        {
            return is_invalidation_topic(uri).then_some(ParsedPushPayload::InvalidateHint);
        }

        if json.get("entities").and_then(|v| v.as_array()).is_some()
            && let Ok(entries) = parse_live_feed(&json)
        {
            return Some(ParsedPushPayload::Entries(entries));
        }

        if json.get("friends").and_then(|v| v.as_array()).is_some()
            && let Ok(mut resp) = serde_json::from_value::<BuddyListResponse>(json.clone())
        {
            normalize_legacy_friends(&mut resp.friends);
            return Some(ParsedPushPayload::LegacyFriends(resp.friends));
        }

        if let Some(activities) = json.get("activities").and_then(|v| v.as_array()) {
            let entries: Vec<LiveEntry> = activities
                .iter()
                .filter_map(|item| parse_single_friend_json(uri, item))
                .collect();
            if !entries.is_empty() {
                return Some(ParsedPushPayload::Entries(entries));
            }
        }

        if let Some(entry) = parse_single_friend_json(uri, &json) {
            return Some(ParsedPushPayload::Entries(vec![entry]));
        }

        // A structured listening-activity push whose entityUri isn't a track/episode (e.g. local
        // track or cleared activity) should be ignored rather than triggering a full HTTP resync.
        if json.get("username").is_some() && json.get("activity").is_some() {
            return None;
        }

        return is_invalidation_topic(uri).then_some(ParsedPushPayload::InvalidateHint);
    }

    // Raw binary protobuf fallback.
    let mut scanned = ProtoScanResult::default();
    let _ = scan_protobuf_fields(payload, 0, &mut scanned);

    let Some(track_uri) = scanned.track_uri else {
        return is_invalidation_topic(uri).then_some(ParsedPushPayload::InvalidateHint);
    };
    let user_uri = scanned
        .user_uri
        .or_else(|| infer_user_uri_from_topic(uri))?;
    let timestamp_ms = scanned
        .timestamp_ms
        .unwrap_or_else(|| Utc::now().timestamp_millis());

    let context_uri = scanned.context_uri.map(|uri| {
        if is_spotify_dj_context(Some(&uri), None) {
            canonical_spotify_dj_uri(&user_uri)
        } else {
            uri
        }
    });

    Some(ParsedPushPayload::Entries(vec![LiveEntry {
        user_uri,
        track_uri,
        context_uri,
        playing: true,
        timestamp_ms,
    }]))
}

fn extract_connection_id(headers: &HashMap<String, String>) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("spotify-connection-id"))
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[derive(Debug, Default)]
pub(crate) struct ListeningActivitySubscribeResponse {
    pub(crate) entries: Vec<LiveEntry>,
    pub(crate) expiration_ms: Option<i64>,
}

pub(crate) fn build_listening_activity_subscription_body(
    usernames: &[String],
    connection_id: &str,
) -> String {
    let subscriptions: Vec<serde_json::Value> = usernames
        .iter()
        .map(|u| u.trim())
        .filter(|u| !u.is_empty())
        .map(|username| {
            serde_json::json!({
                "username": username,
                "pusher": {
                    "connectionId": connection_id,
                }
            })
        })
        .collect();
    serde_json::json!({ "subscriptions": subscriptions }).to_string()
}

pub(crate) fn parse_listening_activity_subscribe_response(
    bytes: &[u8],
) -> Result<ListeningActivitySubscribeResponse> {
    let trimmed = bytes.trim_ascii();
    if trimmed.is_empty() {
        return Ok(ListeningActivitySubscribeResponse::default());
    }
    let json: serde_json::Value = serde_json::from_slice(trimmed)
        .map_err(|e| anyhow!("Failed to parse listening-activity subscribe JSON: {e}"))?;

    let entries = json
        .get("activities")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| parse_single_friend_json(DEALER_LISTENING_ACTIVITY_URI, item))
                .collect()
        })
        .unwrap_or_default();

    let expiration_ms = json
        .get("subscriptionExpirationTimestamp")
        .or_else(|| json.get("subscription_expiration_timestamp"))
        .and_then(parse_json_timestamp_ms);

    Ok(ListeningActivitySubscribeResponse {
        entries,
        expiration_ms,
    })
}

pub(crate) fn compute_subscription_renew_delay(
    expiration_ms: Option<i64>,
    now_ms: i64,
) -> Duration {
    let Some(exp) = expiration_ms else {
        return Duration::from_secs(FALLBACK_SUBSCRIPTION_RENEW_SECS);
    };
    let lead_ms = (SUBSCRIPTION_RENEW_LEAD_SECS * 1000) as i64;
    let min_ms = (MIN_SUBSCRIPTION_RENEW_SECS * 1000) as i64;
    let delay_ms = (exp - now_ms - lead_ms).max(min_ms);
    Duration::from_millis(delay_ms as u64)
}

async fn post_listening_activity_subscription(
    session: &Session,
    path: &str,
    connection_id: &str,
    usernames: &[String],
) -> Result<Bytes> {
    let base_url = session
        .spclient()
        .base_url()
        .await
        .map_err(|e| anyhow!("Failed to resolve spclient access point: {e}"))?;
    let url = format!("{base_url}{path}");
    let body = build_listening_activity_subscription_body(usernames, connection_id);
    let body_bytes = Bytes::from(body);

    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|e| anyhow!("Failed to obtain login5 auth token: {e}"))?;

    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(&url)
        .header(ACCEPT, HeaderValue::from_static("application/json"))
        .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .header(CONTENT_LENGTH, body_bytes.len())
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("{} {}", token.token_type, token.access_token))?,
        );

    if let Ok(client_token) = session.spclient().client_token().await
        && let Ok(val) = HeaderValue::from_str(&client_token)
    {
        builder = builder.header(CLIENT_TOKEN, val);
    }

    let request = builder.body(body_bytes)?;
    session
        .http_client()
        .request_body(request)
        .await
        .map_err(|e| anyhow!("Failed to POST {path}: {e}"))
}

async fn subscribe_listening_activity(
    session: &Session,
    connection_id: &str,
    usernames: &[String],
    debug_push: bool,
) -> Result<ListeningActivitySubscribeResponse> {
    let raw = post_listening_activity_subscription(
        session,
        LISTENING_ACTIVITY_SUBSCRIBE_PATH,
        connection_id,
        usernames,
    )
    .await?;

    let parsed = parse_listening_activity_subscribe_response(&raw)?;
    if debug_push {
        eprintln!(
            "[debug-push] source=listening-activity-subscribe users={} payload_len={} activities={} expiration_ms={:?}",
            usernames.len(),
            raw.len(),
            parsed.entries.len(),
            parsed.expiration_ms
        );
    }
    Ok(parsed)
}

async fn unsubscribe_listening_activity(
    session: &Session,
    connection_id: &str,
    usernames: &[String],
    debug_push: bool,
) {
    if usernames.is_empty() || connection_id.is_empty() {
        return;
    }
    let res = timeout(
        LISTENING_ACTIVITY_UNSUB_TIMEOUT,
        post_listening_activity_subscription(
            session,
            LISTENING_ACTIVITY_UNSUBSCRIBE_PATH,
            connection_id,
            usernames,
        ),
    )
    .await;
    if debug_push {
        eprintln!(
            "[debug-push] source=listening-activity-unsubscribe users={} ok={}",
            usernames.len(),
            matches!(res, Ok(Ok(_)))
        );
    }
}

struct DealerState {
    connections: Subscription,
    presence2: Subscription,
    presence_view: Subscription,
    listening_activity: Subscription,
    connection_id: String,
    subscribed_users: BTreeSet<String>,
    renew_deadline: Option<TokioInstant>,
}

enum DealerEvent {
    Connection(DealerMessage),
    Activity(DealerMessage),
    Closed,
}

impl DealerState {
    async fn next_event(&mut self) -> DealerEvent {
        tokio::select! {
            msg = self.connections.next() => match msg {
                Some(m) => DealerEvent::Connection(m),
                None => DealerEvent::Closed,
            },
            msg = self.presence2.next() => match msg {
                Some(m) => DealerEvent::Activity(m),
                None => DealerEvent::Closed,
            },
            msg = self.presence_view.next() => match msg {
                Some(m) => DealerEvent::Activity(m),
                None => DealerEvent::Closed,
            },
            msg = self.listening_activity.next() => match msg {
                Some(m) => DealerEvent::Activity(m),
                None => DealerEvent::Closed,
            },
        }
    }
}

enum MercuryEvent {
    Message(MercuryResponse),
    SubClosed(String),
}

struct MercuryState {
    tx: mpsc::UnboundedSender<MercuryEvent>,
    rx: mpsc::UnboundedReceiver<MercuryEvent>,
    subscribed_users: HashSet<String>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl MercuryState {
    fn teardown(&mut self) {
        for handle in self.tasks.drain(..) {
            handle.abort();
        }
        self.subscribed_users.clear();
    }
}

enum ActiveTransport {
    Dealer(DealerState),
    Mercury(MercuryState),
    HttpPolling,
}

impl ActiveTransport {
    fn mode(&self) -> TransportMode {
        match self {
            Self::Dealer(_) => TransportMode::DealerPush,
            Self::Mercury(_) => TransportMode::MercuryPush,
            Self::HttpPolling => TransportMode::HttpPolling,
        }
    }
}

enum TransportTickEvent {
    Dealer(DealerEvent),
    Mercury(Option<MercuryEvent>),
}

async fn next_transport_event(transport: &mut ActiveTransport) -> TransportTickEvent {
    match transport {
        ActiveTransport::Dealer(dealer) => TransportTickEvent::Dealer(dealer.next_event().await),
        ActiveTransport::Mercury(mercury) => TransportTickEvent::Mercury(mercury.rx.recv().await),
        ActiveTransport::HttpPolling => std::future::pending().await,
    }
}

async fn teardown_transport(session: &Session, transport: &mut ActiveTransport, debug_push: bool) {
    match transport {
        ActiveTransport::Dealer(dealer) => {
            let conn_id = dealer.connection_id.clone();
            let users: Vec<String> = dealer.subscribed_users.iter().cloned().collect();
            *transport = ActiveTransport::HttpPolling;
            unsubscribe_listening_activity(session, &conn_id, &users, debug_push).await;
            session.dealer().close().await;
        }
        ActiveTransport::Mercury(mercury) => {
            mercury.teardown();
            *transport = ActiveTransport::HttpPolling;
        }
        ActiveTransport::HttpPolling => {}
    }
}

fn dealer_renew_deadline(transport: &ActiveTransport) -> Option<TokioInstant> {
    match transport {
        ActiveTransport::Dealer(dealer) => dealer.renew_deadline,
        ActiveTransport::Mercury(_) | ActiveTransport::HttpPolling => None,
    }
}

async fn wait_for_shutdown(shutdown_rx: &mut watch::Receiver<bool>) {
    if *shutdown_rx.borrow() {
        return;
    }
    loop {
        if shutdown_rx.changed().await.is_err() || *shutdown_rx.borrow() {
            return;
        }
    }
}

async fn wait_for_extra_targets_change(
    extra_targets_rx: &mut watch::Receiver<BTreeSet<String>>,
    closed: bool,
) -> Result<(), watch::error::RecvError> {
    if closed {
        std::future::pending().await
    } else {
        extra_targets_rx.changed().await
    }
}

async fn sleep_until_opt(deadline: Option<TokioInstant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

fn is_missing_credentials_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("No cached Spotify credentials")
}

fn normalize_watch_username(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let stripped_url = trimmed
        .strip_prefix("https://open.spotify.com/user/")
        .or_else(|| trimmed.strip_prefix("http://open.spotify.com/user/"));
    if let Some(rest) = stripped_url {
        let uid = rest.split(['/', '?', '#']).next().unwrap_or_default().trim();
        return (!uid.is_empty()).then(|| uid.to_string());
    }

    let uid = user_id_from_uri(trimmed);
    let uid = uid.trim();
    if uid.is_empty() {
        None
    } else {
        Some(uid.to_string())
    }
}

fn normalize_extra_targets(targets: &BTreeSet<String>) -> BTreeSet<String> {
    targets
        .iter()
        .filter_map(|raw| normalize_watch_username(raw))
        .collect()
}

pub(crate) fn merged_watch_users(
    known_friends: &BTreeSet<String>,
    extra_targets: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut merged = BTreeSet::new();
    for raw in known_friends.iter().chain(extra_targets.iter()) {
        if let Some(uid) = normalize_watch_username(raw) {
            merged.insert(uid);
        }
    }
    merged
}

fn record_known_friends(known_friends: &mut BTreeSet<String>, friends: &[FriendActivity]) {
    for friend in friends {
        if let Some(uid) = normalize_watch_username(&friend.user.uri) {
            known_friends.insert(uid);
        }
    }
}

fn emit_transport_changed(
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
    last_mode: &mut Option<TransportMode>,
    mode: TransportMode,
    reason: Option<String>,
) {
    if *last_mode != Some(mode) {
        *last_mode = Some(mode);
        let _ = event_tx.send(ActivityEvent::TransportChanged { mode, reason });
    }
}

async fn try_start_dealer(
    session: &Session,
    dealer_consumed: &mut bool,
    known_friends: &BTreeSet<String>,
    debug_push: bool,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<(DealerState, ListeningActivitySubscribeResponse)> {
    if *dealer_consumed {
        return Err(anyhow!("Dealer manager already consumed on this session"));
    }
    *dealer_consumed = true;

    let mut connections = session
        .dealer()
        .add_listen_for(DEALER_CONNECTIONS_URI)
        .map_err(|e| anyhow!("Failed to subscribe to Dealer connections: {e}"))?;
    let presence2 = session
        .dealer()
        .add_listen_for(DEALER_PRESENCE2_URI)
        .map_err(|e| anyhow!("Failed to subscribe to Dealer presence2: {e}"))?;
    let presence_view = session
        .dealer()
        .add_listen_for(DEALER_PRESENCE_VIEW_URI)
        .map_err(|e| anyhow!("Failed to subscribe to Dealer presence-view: {e}"))?;
    let listening_activity = session
        .dealer()
        .add_listen_for(DEALER_LISTENING_ACTIVITY_URI)
        .map_err(|e| anyhow!("Failed to subscribe to Dealer listening-activity: {e}"))?;

    let start_res = tokio::select! {
        _ = wait_for_shutdown(shutdown_rx) => return Err(anyhow!("Shutdown requested")),
        res = timeout(DEALER_START_TIMEOUT, session.dealer().start()) => res,
    };

    match start_res {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(anyhow!("Failed to start Dealer WebSocket: {e}")),
        Err(_) => return Err(anyhow!("Timed out starting Dealer WebSocket")),
    }

    let deadline = TokioInstant::now() + DEALER_CONN_ID_TIMEOUT;
    let connection_id = loop {
        let remaining = deadline.saturating_duration_since(TokioInstant::now());
        if remaining.is_zero() {
            session.dealer().close().await;
            return Err(anyhow!(
                "Timed out waiting for Dealer Spotify-Connection-Id"
            ));
        }

        let next_msg = tokio::select! {
            _ = wait_for_shutdown(shutdown_rx) => {
                session.dealer().close().await;
                return Err(anyhow!("Shutdown requested"));
            }
            res = timeout(remaining, connections.next()) => res,
        };

        match next_msg {
            Ok(Some(msg)) => {
                let conn_id = extract_connection_id(&msg.headers);
                if debug_push {
                    eprintln!(
                        "[debug-push] source=dealer-conn uri={} headers={:?} connection_id={:?}",
                        msg.uri, msg.headers, conn_id
                    );
                }
                if let Some(id) = conn_id {
                    break id;
                }
            }
            Ok(None) => {
                session.dealer().close().await;
                return Err(anyhow!(
                    "Dealer connections stream closed before Spotify-Connection-Id arrived"
                ));
            }
            Err(_) => {
                session.dealer().close().await;
                return Err(anyhow!(
                    "Timed out waiting for Dealer Spotify-Connection-Id"
                ));
            }
        }
    };

    let usernames: Vec<String> = known_friends.iter().cloned().collect();
    let sub_res = tokio::select! {
        _ = wait_for_shutdown(shutdown_rx) => {
            session.dealer().close().await;
            return Err(anyhow!("Shutdown requested"));
        }
        res = timeout(
            LISTENING_ACTIVITY_SUB_TIMEOUT,
            subscribe_listening_activity(session, &connection_id, &usernames, debug_push),
        ) => res,
    };

    let sub_resp = match sub_res {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            session.dealer().close().await;
            return Err(e);
        }
        Err(_) => {
            session.dealer().close().await;
            return Err(anyhow!(
                "Timed out subscribing to listening-activity via Dealer"
            ));
        }
    };

    let subscribed_users: BTreeSet<String> = usernames.into_iter().collect();
    let renew_deadline = if subscribed_users.is_empty() {
        None
    } else {
        Some(
            TokioInstant::now()
                + compute_subscription_renew_delay(
                    sub_resp.expiration_ms,
                    Utc::now().timestamp_millis(),
                ),
        )
    };

    Ok((
        DealerState {
            connections,
            presence2,
            presence_view,
            listening_activity,
            connection_id,
            subscribed_users,
            renew_deadline,
        },
        sub_resp,
    ))
}

async fn subscribe_mercury_usernames(
    session: &Session,
    usernames: Vec<String>,
    tx: &mpsc::UnboundedSender<MercuryEvent>,
    subscribed_users: &mut HashSet<String>,
    tasks: &mut Vec<tokio::task::JoinHandle<()>>,
    shutdown_rx: &mut watch::Receiver<bool>,
) {
    tasks.retain(|h| !h.is_finished());
    if usernames.is_empty() || *shutdown_rx.borrow() {
        return;
    }

    let sub_stream = futures_util::stream::iter(usernames)
        .map(|username| {
            let encoded = percent_encode_path_segment(&username);
            let topic = format!("{DEALER_PRESENCE2_URI}{encoded}");
            let sub_fut = session.mercury().subscribe(topic);
            async move {
                match timeout(MERCURY_SUB_TIMEOUT, sub_fut).await {
                    Ok(Ok(rx)) => (username, Some(rx)),
                    _ => (username, None),
                }
            }
        })
        .buffer_unordered(MERCURY_SUB_CONCURRENCY)
        .collect::<Vec<_>>();

    let results = tokio::select! {
        _ = wait_for_shutdown(shutdown_rx) => return,
        res = sub_stream => res,
    };

    for (username, maybe_rx) in results {
        if let Some(mut rx) = maybe_rx
            && subscribed_users.insert(username.clone())
        {
            let tx_clone = tx.clone();
            let handle = tokio::spawn(async move {
                while let Some(resp) = rx.recv().await {
                    if tx_clone.send(MercuryEvent::Message(resp)).is_err() {
                        return;
                    }
                }
                let _ = tx_clone.send(MercuryEvent::SubClosed(username));
            });
            tasks.push(handle);
        }
    }
}

async fn try_start_mercury(
    session: &Session,
    known_friends: &BTreeSet<String>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<MercuryState> {
    if *shutdown_rx.borrow() {
        return Err(anyhow!("Shutdown requested"));
    }

    let usernames: Vec<String> = known_friends
        .iter()
        .take(MERCURY_MAX_SUBSCRIPTIONS)
        .cloned()
        .collect();
    if usernames.is_empty() {
        return Err(anyhow!("No known friends to subscribe to via Mercury"));
    }

    let (tx, rx) = mpsc::unbounded_channel();
    let mut subscribed_users = HashSet::new();
    let mut tasks = Vec::new();

    // Probe the first batch first so blocked/timed-out Mercury fails fast in <=4s instead of 20s.
    let mut iter = usernames.into_iter();
    let first_batch: Vec<String> = iter.by_ref().take(MERCURY_SUB_CONCURRENCY).collect();
    let remaining: Vec<String> = iter.collect();

    subscribe_mercury_usernames(
        session,
        first_batch,
        &tx,
        &mut subscribed_users,
        &mut tasks,
        shutdown_rx,
    )
    .await;

    if subscribed_users.is_empty() {
        return Err(anyhow!(
            "Failed to subscribe to any friend presence topics via Mercury"
        ));
    }

    if !remaining.is_empty() && !*shutdown_rx.borrow() {
        subscribe_mercury_usernames(
            session,
            remaining,
            &tx,
            &mut subscribed_users,
            &mut tasks,
            shutdown_rx,
        )
        .await;
    }

    Ok(MercuryState {
        tx,
        rx,
        subscribed_users,
        tasks,
    })
}

async fn subscribe_new_mercury_friends(
    session: &Session,
    mercury: &mut MercuryState,
    known_friends: &BTreeSet<String>,
    shutdown_rx: &mut watch::Receiver<bool>,
) {
    mercury.tasks.retain(|h| !h.is_finished());
    let remaining_capacity = MERCURY_MAX_SUBSCRIPTIONS.saturating_sub(mercury.subscribed_users.len());
    if remaining_capacity == 0 {
        return;
    }

    let new_usernames: Vec<String> = known_friends
        .iter()
        .filter(|u| !mercury.subscribed_users.contains(u.as_str()))
        .take(remaining_capacity)
        .cloned()
        .collect();

    if new_usernames.is_empty() {
        return;
    }

    subscribe_mercury_usernames(
        session,
        new_usernames,
        &mercury.tx,
        &mut mercury.subscribed_users,
        &mut mercury.tasks,
        shutdown_rx,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn emit_subscribe_entries(
    session: &Session,
    entries: Vec<LiveEntry>,
    name_cache: &mut NameCache,
    known_friends: &mut BTreeSet<String>,
    activity_signatures: &mut HashMap<String, (String, i64)>,
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
) -> bool {
    if entries.is_empty() {
        return false;
    }

    if let Ok(friends) = enrich_live_entries(session, entries, name_cache).await
        && !friends.is_empty()
    {
        record_known_friends(known_friends, &friends);
        let _ = record_activity_signatures(activity_signatures, &friends, None);
        let _ = event_tx.send(ActivityEvent::FullSnapshot {
            friends,
            transport: TransportMode::DealerPush,
        });
        return true;
    }

    false
}

fn record_activity_signatures(
    signatures: &mut HashMap<String, (String, i64)>,
    friends: &[FriendActivity],
    tracked_usernames: Option<&BTreeSet<String>>,
) -> bool {
    let mut changed = false;
    let had_existing = !signatures.is_empty();
    for friend in friends {
        let sig = (friend.track.uri.clone(), friend.timestamp);
        if signatures.get(&friend.user.uri) != Some(&sig) {
            if had_existing {
                let is_tracked = match tracked_usernames {
                    Some(tracked) => tracked.contains(&user_id_from_uri(&friend.user.uri)),
                    None => true,
                };
                if is_tracked {
                    changed = true;
                }
            }
            signatures.insert(friend.user.uri.clone(), sig);
        }
    }
    changed
}

async fn sync_dealer_subscriptions(
    session: &Session,
    dealer: &mut DealerState,
    watch_users: &BTreeSet<String>,
    debug_push: bool,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Option<ListeningActivitySubscribeResponse> {
    let has_new_users = watch_users
        .iter()
        .any(|u| !dealer.subscribed_users.contains(u));
    if !has_new_users || *shutdown_rx.borrow() {
        return None;
    }

    let usernames: Vec<String> = watch_users.iter().cloned().collect();
    let sub_res = tokio::select! {
        _ = wait_for_shutdown(shutdown_rx) => return None,
        res = timeout(
            LISTENING_ACTIVITY_SUB_TIMEOUT,
            subscribe_listening_activity(session, &dealer.connection_id, &usernames, debug_push),
        ) => res,
    };

    if let Ok(Ok(resp)) = sub_res {
        dealer.subscribed_users = usernames.into_iter().collect();
        dealer.renew_deadline = if dealer.subscribed_users.is_empty() {
            None
        } else {
            Some(
                TokioInstant::now()
                    + compute_subscription_renew_delay(
                        resp.expiration_ms,
                        Utc::now().timestamp_millis(),
                    ),
            )
        };
        return Some(resp);
    }

    None
}

#[allow(clippy::too_many_arguments)]
async fn emit_friend_updates_from_entries(
    session: &Session,
    entries: Vec<LiveEntry>,
    transport: TransportMode,
    name_cache: &mut NameCache,
    activity_signatures: &mut HashMap<String, (String, i64)>,
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
) {
    if entries.is_empty() {
        return;
    }

    if let Ok(activities) = enrich_live_entries(session, entries, name_cache).await {
        let _ = record_activity_signatures(activity_signatures, &activities, None);
        for activity in activities {
            let _ = event_tx.send(ActivityEvent::FriendUpdated {
                activity,
                transport,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn perform_full_sync(
    session: &Session,
    transport: &mut ActiveTransport,
    name_cache: &mut NameCache,
    known_friends: &mut BTreeSet<String>,
    extra_targets: &BTreeSet<String>,
    activity_signatures: &mut HashMap<String, (String, i64)>,
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
    consecutive_errors: &mut u32,
    debug_push: bool,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> bool {
    match fetch_buddylist(session, name_cache).await {
        Ok(friends) => {
            *consecutive_errors = 0;
            record_known_friends(known_friends, &friends);
            let tracked = match transport {
                ActiveTransport::Dealer(dealer) => Some(&dealer.subscribed_users),
                _ => None,
            };
            let changed = record_activity_signatures(activity_signatures, &friends, tracked);
            let mode = transport.mode();
            let _ = event_tx.send(ActivityEvent::FullSnapshot {
                friends,
                transport: mode,
            });
            let watch_users = merged_watch_users(known_friends, extra_targets);
            match transport {
                ActiveTransport::Dealer(dealer) => {
                    if let Some(resp) = sync_dealer_subscriptions(
                        session,
                        dealer,
                        &watch_users,
                        debug_push,
                        shutdown_rx,
                    )
                    .await
                    {
                        emit_friend_updates_from_entries(
                            session,
                            resp.entries,
                            TransportMode::DealerPush,
                            name_cache,
                            activity_signatures,
                            event_tx,
                        )
                        .await;
                    }
                }
                ActiveTransport::Mercury(mercury) => {
                    subscribe_new_mercury_friends(session, mercury, &watch_users, shutdown_rx)
                        .await;
                }
                ActiveTransport::HttpPolling => {}
            }
            changed
        }
        Err(e) => {
            *consecutive_errors = consecutive_errors.saturating_add(1);
            let _ = event_tx.send(ActivityEvent::Error {
                message: e.to_string(),
                consecutive_errors: *consecutive_errors,
            });
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_parsed_push(
    session: &Session,
    parsed: ParsedPushPayload,
    transport_mode: TransportMode,
    name_cache: &mut NameCache,
    known_friends: &mut BTreeSet<String>,
    activity_signatures: &mut HashMap<String, (String, i64)>,
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
    invalidate_deadline: &mut Option<TokioInstant>,
    consecutive_errors: &mut u32,
) {
    match parsed {
        ParsedPushPayload::InvalidateHint => {
            if invalidate_deadline.is_none() {
                *invalidate_deadline = Some(TokioInstant::now() + INVALIDATE_DEBOUNCE);
            }
        }
        ParsedPushPayload::LegacyFriends(friends) => {
            *consecutive_errors = 0;
            record_known_friends(known_friends, &friends);
            let _ = record_activity_signatures(activity_signatures, &friends, None);
            let _ = event_tx.send(ActivityEvent::FullSnapshot {
                friends,
                transport: transport_mode,
            });
        }
        ParsedPushPayload::Entries(entries) => {
            if entries.is_empty() {
                return;
            }
            match enrich_live_entries(session, entries, name_cache).await {
                Ok(activities) => {
                    *consecutive_errors = 0;
                    record_known_friends(known_friends, &activities);
                    let _ = record_activity_signatures(activity_signatures, &activities, None);
                    for activity in activities {
                        let _ = event_tx.send(ActivityEvent::FriendUpdated {
                            activity,
                            transport: transport_mode,
                        });
                    }
                }
                Err(e) => {
                    *consecutive_errors = consecutive_errors.saturating_add(1);
                    let _ = event_tx.send(ActivityEvent::Error {
                        message: e.to_string(),
                        consecutive_errors: *consecutive_errors,
                    });
                }
            }
        }
    }
}

fn parse_mercury_response(resp: &MercuryResponse, debug_push: bool) -> Option<ParsedPushPayload> {
    let total_len: usize = resp.payload.iter().map(Vec::len).sum();
    let parsed = if resp.payload.is_empty() {
        parse_push_payload(&resp.uri, &[])
    } else if resp.payload.len() == 1 {
        parse_push_payload(&resp.uri, &resp.payload[0])
    } else {
        let mut collected_entries = Vec::new();
        let mut collected_legacy = Vec::new();
        let mut saw_invalidate = false;
        for part in &resp.payload {
            match parse_push_payload(&resp.uri, part) {
                Some(ParsedPushPayload::Entries(entries)) => collected_entries.extend(entries),
                Some(ParsedPushPayload::LegacyFriends(friends)) => collected_legacy.extend(friends),
                Some(ParsedPushPayload::InvalidateHint) => saw_invalidate = true,
                None => {}
            }
        }

        if !collected_entries.is_empty() {
            Some(ParsedPushPayload::Entries(collected_entries))
        } else if !collected_legacy.is_empty() {
            Some(ParsedPushPayload::LegacyFriends(collected_legacy))
        } else {
            let mut combined = Vec::with_capacity(total_len);
            for part in &resp.payload {
                combined.extend_from_slice(part);
            }
            parse_push_payload(&resp.uri, &combined)
                .or_else(|| saw_invalidate.then_some(ParsedPushPayload::InvalidateHint))
        }
    };

    if debug_push {
        eprintln!(
            "[debug-push] source=mercury uri={} status={} parts={} payload_len={} parsed={:?}",
            resp.uri,
            resp.status_code,
            resp.payload.len(),
            total_len,
            parsed
        );
    }

    parsed
}

fn parse_dealer_message(msg: &DealerMessage, debug_push: bool) -> Option<ParsedPushPayload> {
    let payload_bytes: &[u8] = match &msg.payload {
        PayloadValue::Empty => &[],
        PayloadValue::Raw(bytes) => bytes.as_slice(),
        PayloadValue::Json(s) => s.as_bytes(),
    };
    let parsed = parse_push_payload(&msg.uri, payload_bytes);
    if debug_push {
        let preview: String = String::from_utf8_lossy(payload_bytes)
            .chars()
            .take(300)
            .collect();
        eprintln!(
            "[debug-push] source=dealer uri={} headers={:?} payload_len={} preview={:?} parsed={:?}",
            msg.uri,
            msg.headers,
            payload_bytes.len(),
            preview,
            parsed
        );
    }
    parsed
}

#[allow(clippy::too_many_arguments)]
async fn establish_transport(
    session: &Session,
    dealer_consumed: &mut bool,
    config: &StreamConfig,
    name_cache: &mut NameCache,
    known_friends: &mut BTreeSet<String>,
    extra_targets: &BTreeSet<String>,
    activity_signatures: &mut HashMap<String, (String, i64)>,
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
    last_mode: &mut Option<TransportMode>,
    consecutive_errors: &mut u32,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> ActiveTransport {
    let mut fallback_reason: Option<String> = None;
    let mut prefetched_friends: Option<Vec<FriendActivity>> = None;
    let mut initial_emitted = false;

    if matches!(
        config.transport,
        TransportPreference::Auto | TransportPreference::Dealer
    ) {
        // Fetch `/listening-activity/v1/feed` first so we know which friend usernames to
        // register with `/listening-activity/v1/subscribe` on the Dealer connection.
        if known_friends.is_empty() {
            match fetch_buddylist(session, name_cache).await {
                Ok(friends) => {
                    *consecutive_errors = 0;
                    record_known_friends(known_friends, &friends);
                    let _ = record_activity_signatures(activity_signatures, &friends, None);
                    prefetched_friends = Some(friends);
                }
                Err(e) => {
                    *consecutive_errors = consecutive_errors.saturating_add(1);
                    let _ = event_tx.send(ActivityEvent::Error {
                        message: e.to_string(),
                        consecutive_errors: *consecutive_errors,
                    });
                }
            }
        }

        if *shutdown_rx.borrow() {
            return ActiveTransport::HttpPolling;
        }

        let watch_users = merged_watch_users(known_friends, extra_targets);
        match try_start_dealer(
            session,
            dealer_consumed,
            &watch_users,
            config.debug_push,
            shutdown_rx,
        )
        .await
        {
            Ok((dealer_state, sub_resp)) => {
                *consecutive_errors = 0;
                emit_transport_changed(event_tx, last_mode, TransportMode::DealerPush, None);
                let mut transport = ActiveTransport::Dealer(dealer_state);

                if let Some(friends) = prefetched_friends.take() {
                    let _ = event_tx.send(ActivityEvent::FullSnapshot {
                        friends,
                        transport: TransportMode::DealerPush,
                    });
                    // Apply any fresher activity updates returned directly by `/subscribe`.
                    if !sub_resp.entries.is_empty()
                        && let Ok(activities) =
                            enrich_live_entries(session, sub_resp.entries, name_cache).await
                    {
                        record_known_friends(known_friends, &activities);
                        let _ = record_activity_signatures(activity_signatures, &activities, None);
                        for activity in activities {
                            let _ = event_tx.send(ActivityEvent::FriendUpdated {
                                activity,
                                transport: TransportMode::DealerPush,
                            });
                        }
                    }
                } else {
                    let emitted = emit_subscribe_entries(
                        session,
                        sub_resp.entries,
                        name_cache,
                        known_friends,
                        activity_signatures,
                        event_tx,
                    )
                    .await;
                    if !emitted {
                        let _ = perform_full_sync(
                            session,
                            &mut transport,
                            name_cache,
                            known_friends,
                            extra_targets,
                            activity_signatures,
                            event_tx,
                            consecutive_errors,
                            config.debug_push,
                            shutdown_rx,
                        )
                        .await;
                    }
                }
                return transport;
            }
            Err(e) => {
                fallback_reason = Some(format!("Dealer push unavailable: {e}"));
            }
        }
    }

    if *shutdown_rx.borrow() {
        return ActiveTransport::HttpPolling;
    }

    if matches!(
        config.transport,
        TransportPreference::Auto | TransportPreference::Mercury
    ) {
        if let Some(friends) = prefetched_friends.take() {
            // Emit immediately so UI/CLI hydrate right away while Mercury probes.
            let _ = event_tx.send(ActivityEvent::FullSnapshot {
                friends,
                transport: TransportMode::HttpPolling,
            });
            initial_emitted = true;
        } else if known_friends.is_empty() {
            match fetch_buddylist(session, name_cache).await {
                Ok(friends) => {
                    *consecutive_errors = 0;
                    record_known_friends(known_friends, &friends);
                    let _ = record_activity_signatures(activity_signatures, &friends, None);
                    let _ = event_tx.send(ActivityEvent::FullSnapshot {
                        friends,
                        transport: TransportMode::HttpPolling,
                    });
                    initial_emitted = true;
                }
                Err(e) => {
                    *consecutive_errors = consecutive_errors.saturating_add(1);
                    let _ = event_tx.send(ActivityEvent::Error {
                        message: e.to_string(),
                        consecutive_errors: *consecutive_errors,
                    });
                }
            }
        }

        if *shutdown_rx.borrow() {
            return ActiveTransport::HttpPolling;
        }

        let watch_users = merged_watch_users(known_friends, extra_targets);
        match try_start_mercury(session, &watch_users, shutdown_rx).await {
            Ok(mercury_state) => {
                emit_transport_changed(
                    event_tx,
                    last_mode,
                    TransportMode::MercuryPush,
                    fallback_reason,
                );
                let mut transport = ActiveTransport::Mercury(mercury_state);
                if !initial_emitted {
                    let _ = perform_full_sync(
                        session,
                        &mut transport,
                        name_cache,
                        known_friends,
                        extra_targets,
                        activity_signatures,
                        event_tx,
                        consecutive_errors,
                        config.debug_push,
                        shutdown_rx,
                    )
                    .await;
                }
                return transport;
            }
            Err(e) => {
                fallback_reason = Some(match fallback_reason {
                    Some(prev) => format!("{prev}; Mercury push unavailable: {e}"),
                    None => format!("Mercury push unavailable: {e}"),
                });
            }
        }
    }

    if *shutdown_rx.borrow() {
        return ActiveTransport::HttpPolling;
    }

    emit_transport_changed(
        event_tx,
        last_mode,
        TransportMode::HttpPolling,
        fallback_reason,
    );
    let mut transport = ActiveTransport::HttpPolling;
    if let Some(friends) = prefetched_friends.take() {
        let _ = event_tx.send(ActivityEvent::FullSnapshot {
            friends,
            transport: TransportMode::HttpPolling,
        });
    } else if !initial_emitted {
        let _ = perform_full_sync(
            session,
            &mut transport,
            name_cache,
            known_friends,
            extra_targets,
            activity_signatures,
            event_tx,
            consecutive_errors,
            config.debug_push,
            shutdown_rx,
        )
        .await;
    }
    transport
}

async fn connect_with_backoff(
    cache_dir: &Path,
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
    consecutive_errors: &mut u32,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Option<Session> {
    let mut attempt: u32 = 0;
    loop {
        if *shutdown_rx.borrow() || event_tx.is_closed() {
            return None;
        }

        match connect_cached(cache_dir).await {
            Ok(session) => {
                *consecutive_errors = 0;
                return Some(session);
            }
            Err(e) => {
                *consecutive_errors = consecutive_errors.saturating_add(1);
                let _ = event_tx.send(ActivityEvent::Error {
                    message: e.to_string(),
                    consecutive_errors: *consecutive_errors,
                });
                if is_missing_credentials_error(&e) {
                    return None;
                }
            }
        }

        let backoff_secs = (2u64 << attempt.min(5)).min(RECONNECT_MAX_BACKOFF_SECS);
        attempt = attempt.saturating_add(1);
        tokio::select! {
            _ = wait_for_shutdown(shutdown_rx) => return None,
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
        }
    }
}

fn next_tick_instant(
    transport: &ActiveTransport,
    poll_interval_secs: u64,
    resync_interval_secs: u64,
    consecutive_errors: u32,
) -> TokioInstant {
    let secs = match transport {
        ActiveTransport::Dealer(_) | ActiveTransport::Mercury(_) => resync_interval_secs,
        ActiveTransport::HttpPolling => {
            if consecutive_errors > 0 {
                (poll_interval_secs * (1u64 << consecutive_errors.min(4))).min(MAX_POLL_BACKOFF_SECS)
            } else {
                poll_interval_secs
            }
        }
    };
    TokioInstant::now() + Duration::from_secs(secs)
}

fn next_upgrade_instant(
    transport: &ActiveTransport,
    preference: TransportPreference,
) -> Option<TokioInstant> {
    if matches!(transport, ActiveTransport::HttpPolling) && preference != TransportPreference::Poll
    {
        Some(TokioInstant::now() + PUSH_UPGRADE_INTERVAL)
    } else {
        None
    }
}

async fn run_activity_stream(
    cache_dir: PathBuf,
    config: StreamConfig,
    refresh_notify: Arc<Notify>,
    mut shutdown_rx: watch::Receiver<bool>,
    mut extra_targets_rx: watch::Receiver<BTreeSet<String>>,
    event_tx: mpsc::UnboundedSender<ActivityEvent>,
) {
    let poll_interval_secs = config.poll_interval_secs.max(MIN_POLL_INTERVAL_SECS);
    let resync_interval_secs = if config.resync_interval_secs == 0 {
        DEFAULT_RESYNC_INTERVAL_SECS
    } else {
        config.resync_interval_secs.max(MIN_POLL_INTERVAL_SECS)
    };

    let mut consecutive_errors: u32 = 0;
    let Some(mut session) = connect_with_backoff(
        &cache_dir,
        &event_tx,
        &mut consecutive_errors,
        &mut shutdown_rx,
    )
    .await
    else {
        return;
    };

    let mut dealer_consumed = false;
    let mut name_cache = NameCache::default();
    let mut known_friends: BTreeSet<String> = BTreeSet::new();
    let mut extra_targets: BTreeSet<String> =
        normalize_extra_targets(&extra_targets_rx.borrow_and_update());
    let mut extra_targets_closed = false;
    let mut activity_signatures: HashMap<String, (String, i64)> = HashMap::new();
    let mut last_mode: Option<TransportMode> = None;
    let mut dealer_push_seen_since_resync = false;
    let mut dealer_stale_resyncs: u32 = 0;

    let mut transport = establish_transport(
        &session,
        &mut dealer_consumed,
        &config,
        &mut name_cache,
        &mut known_friends,
        &extra_targets,
        &mut activity_signatures,
        &event_tx,
        &mut last_mode,
        &mut consecutive_errors,
        &mut shutdown_rx,
    )
    .await;

    let mut next_tick = next_tick_instant(
        &transport,
        poll_interval_secs,
        resync_interval_secs,
        consecutive_errors,
    );
    let mut next_upgrade = next_upgrade_instant(&transport, config.transport);
    let mut invalidate_deadline: Option<TokioInstant> = None;

    loop {
        if *shutdown_rx.borrow() || event_tx.is_closed() {
            break;
        }

        if session.is_invalid() || consecutive_errors >= CONSECUTIVE_ERRORS_RECONNECT_THRESHOLD {
            teardown_transport(&session, &mut transport, config.debug_push).await;
            session.shutdown();
            invalidate_deadline = None;
            dealer_push_seen_since_resync = false;
            dealer_stale_resyncs = 0;

            let Some(new_session) = connect_with_backoff(
                &cache_dir,
                &event_tx,
                &mut consecutive_errors,
                &mut shutdown_rx,
            )
            .await
            else {
                return;
            };

            session = new_session;
            dealer_consumed = false;
            let account = Some(session.username()).filter(|s| !s.is_empty());
            let _ = event_tx.send(ActivityEvent::SessionReconnected { account });

            transport = establish_transport(
                &session,
                &mut dealer_consumed,
                &config,
                &mut name_cache,
                &mut known_friends,
                &extra_targets,
                &mut activity_signatures,
                &event_tx,
                &mut last_mode,
                &mut consecutive_errors,
                &mut shutdown_rx,
            )
            .await;
            next_tick = next_tick_instant(
                &transport,
                poll_interval_secs,
                resync_interval_secs,
                consecutive_errors,
            );
            next_upgrade = next_upgrade_instant(&transport, config.transport);
            continue;
        }

        let renew_deadline = dealer_renew_deadline(&transport);

        tokio::select! {
            _ = wait_for_shutdown(&mut shutdown_rx) => {
                break;
            }
            changed_res = wait_for_extra_targets_change(&mut extra_targets_rx, extra_targets_closed) => {
                match changed_res {
                    Ok(()) => {
                        extra_targets = normalize_extra_targets(&extra_targets_rx.borrow_and_update());
                        let watch_users = merged_watch_users(&known_friends, &extra_targets);
                        match &mut transport {
                            ActiveTransport::Dealer(dealer) => {
                                if let Some(resp) = sync_dealer_subscriptions(
                                    &session,
                                    dealer,
                                    &watch_users,
                                    config.debug_push,
                                    &mut shutdown_rx,
                                )
                                .await
                                {
                                    emit_friend_updates_from_entries(
                                        &session,
                                        resp.entries,
                                        TransportMode::DealerPush,
                                        &mut name_cache,
                                        &mut activity_signatures,
                                        &event_tx,
                                    )
                                    .await;
                                }
                            }
                            ActiveTransport::Mercury(mercury) => {
                                subscribe_new_mercury_friends(
                                    &session,
                                    mercury,
                                    &watch_users,
                                    &mut shutdown_rx,
                                )
                                .await;
                            }
                            ActiveTransport::HttpPolling => {
                                if !extra_targets.is_empty() {
                                    let _ = perform_full_sync(
                                        &session,
                                        &mut transport,
                                        &mut name_cache,
                                        &mut known_friends,
                                        &extra_targets,
                                        &mut activity_signatures,
                                        &event_tx,
                                        &mut consecutive_errors,
                                        config.debug_push,
                                        &mut shutdown_rx,
                                    )
                                    .await;
                                    next_tick = next_tick_instant(
                                        &transport,
                                        poll_interval_secs,
                                        resync_interval_secs,
                                        consecutive_errors,
                                    );
                                }
                            }
                        }
                    }
                    Err(_) => {
                        extra_targets_closed = true;
                    }
                }
            }
            _ = refresh_notify.notified() => {
                invalidate_deadline = None;
                let _ = perform_full_sync(
                    &session,
                    &mut transport,
                    &mut name_cache,
                    &mut known_friends,
                    &extra_targets,
                    &mut activity_signatures,
                    &event_tx,
                    &mut consecutive_errors,
                    config.debug_push,
                    &mut shutdown_rx,
                )
                .await;
                next_tick = next_tick_instant(
                    &transport,
                    poll_interval_secs,
                    resync_interval_secs,
                    consecutive_errors,
                );
            }
            _ = sleep_until_opt(invalidate_deadline) => {
                invalidate_deadline = None;
                let _ = perform_full_sync(
                    &session,
                    &mut transport,
                    &mut name_cache,
                    &mut known_friends,
                    &extra_targets,
                    &mut activity_signatures,
                    &event_tx,
                    &mut consecutive_errors,
                    config.debug_push,
                    &mut shutdown_rx,
                )
                .await;
                next_tick = next_tick_instant(
                    &transport,
                    poll_interval_secs,
                    resync_interval_secs,
                    consecutive_errors,
                );
            }
            _ = sleep_until_opt(renew_deadline) => {
                if let ActiveTransport::Dealer(dealer) = &mut transport {
                    let watch_users = merged_watch_users(&known_friends, &extra_targets);
                    let usernames: Vec<String> = watch_users.iter().cloned().collect();
                    if usernames.is_empty() {
                        dealer.renew_deadline = None;
                    } else {
                        let conn_id = dealer.connection_id.clone();
                        let renew_res = tokio::select! {
                            _ = wait_for_shutdown(&mut shutdown_rx) => break,
                            res = timeout(
                                LISTENING_ACTIVITY_SUB_TIMEOUT,
                                subscribe_listening_activity(
                                    &session,
                                    &conn_id,
                                    &usernames,
                                    config.debug_push,
                                ),
                            ) => res,
                        };
                        match renew_res {
                            Ok(Ok(sub_resp)) => {
                                consecutive_errors = 0;
                                dealer.subscribed_users = usernames.into_iter().collect();
                                dealer.renew_deadline = Some(
                                    TokioInstant::now()
                                        + compute_subscription_renew_delay(
                                            sub_resp.expiration_ms,
                                            Utc::now().timestamp_millis(),
                                        ),
                                );
                            }
                            Ok(Err(e)) => {
                                teardown_transport(&session, &mut transport, config.debug_push).await;
                                let reason = format!("Dealer subscription renewal failed: {e}");
                                transport = downgrade_after_dealer(
                                    &session,
                                    config.transport,
                                    &watch_users,
                                    &event_tx,
                                    &mut last_mode,
                                    reason,
                                    &mut shutdown_rx,
                                )
                                .await;
                                next_tick = next_tick_instant(
                                    &transport,
                                    poll_interval_secs,
                                    resync_interval_secs,
                                    consecutive_errors,
                                );
                                next_upgrade = next_upgrade_instant(&transport, config.transport);
                            }
                            Err(_) => {
                                teardown_transport(&session, &mut transport, config.debug_push).await;
                                let reason = "Dealer subscription renewal timed out".to_string();
                                transport = downgrade_after_dealer(
                                    &session,
                                    config.transport,
                                    &watch_users,
                                    &event_tx,
                                    &mut last_mode,
                                    reason,
                                    &mut shutdown_rx,
                                )
                                .await;
                                next_tick = next_tick_instant(
                                    &transport,
                                    poll_interval_secs,
                                    resync_interval_secs,
                                    consecutive_errors,
                                );
                                next_upgrade = next_upgrade_instant(&transport, config.transport);
                            }
                        }
                    }
                }
            }
            _ = tokio::time::sleep_until(next_tick) => {
                let changed = perform_full_sync(
                    &session,
                    &mut transport,
                    &mut name_cache,
                    &mut known_friends,
                    &extra_targets,
                    &mut activity_signatures,
                    &event_tx,
                    &mut consecutive_errors,
                    config.debug_push,
                    &mut shutdown_rx,
                )
                .await;

                if matches!(transport, ActiveTransport::Dealer(_)) {
                    if dealer_push_seen_since_resync {
                        dealer_push_seen_since_resync = false;
                        dealer_stale_resyncs = 0;
                    } else if changed {
                        dealer_stale_resyncs = dealer_stale_resyncs.saturating_add(1);
                        if dealer_stale_resyncs >= DEALER_STALE_RESYNC_THRESHOLD {
                            dealer_stale_resyncs = 0;
                            teardown_transport(&session, &mut transport, config.debug_push).await;
                            let watch_users = merged_watch_users(&known_friends, &extra_targets);
                            transport = downgrade_after_dealer(
                                &session,
                                config.transport,
                                &watch_users,
                                &event_tx,
                                &mut last_mode,
                                format!(
                                    "Dealer silent across {DEALER_STALE_RESYNC_THRESHOLD} consecutive resyncs with track changes"
                                ),
                                &mut shutdown_rx,
                            )
                            .await;
                            next_upgrade = next_upgrade_instant(&transport, config.transport);
                        }
                    }
                }

                next_tick = next_tick_instant(
                    &transport,
                    poll_interval_secs,
                    resync_interval_secs,
                    consecutive_errors,
                );
            }
            _ = sleep_until_opt(next_upgrade) => {
                dealer_push_seen_since_resync = false;
                dealer_stale_resyncs = 0;
                match config.transport {
                    TransportPreference::Auto | TransportPreference::Dealer => {
                        if dealer_consumed {
                            match connect_cached(&cache_dir).await {
                                Ok(new_session) => {
                                    consecutive_errors = 0;
                                    teardown_transport(&session, &mut transport, config.debug_push).await;
                                    session.shutdown();
                                    session = new_session;
                                    dealer_consumed = false;
                                    let account = Some(session.username()).filter(|s| !s.is_empty());
                                    let _ = event_tx.send(ActivityEvent::SessionReconnected { account });
                                    transport = establish_transport(
                                        &session,
                                        &mut dealer_consumed,
                                        &config,
                                        &mut name_cache,
                                        &mut known_friends,
                                        &extra_targets,
                                        &mut activity_signatures,
                                        &event_tx,
                                        &mut last_mode,
                                        &mut consecutive_errors,
                                        &mut shutdown_rx,
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    consecutive_errors = consecutive_errors.saturating_add(1);
                                    let _ = event_tx.send(ActivityEvent::Error {
                                        message: format!("Push upgrade reconnect failed: {e}"),
                                        consecutive_errors,
                                    });
                                    if is_missing_credentials_error(&e) {
                                        break;
                                    }
                                }
                            }
                        } else {
                            transport = establish_transport(
                                &session,
                                &mut dealer_consumed,
                                &config,
                                &mut name_cache,
                                &mut known_friends,
                                &extra_targets,
                                &mut activity_signatures,
                                &event_tx,
                                &mut last_mode,
                                &mut consecutive_errors,
                                &mut shutdown_rx,
                            )
                            .await;
                        }
                    }
                    TransportPreference::Mercury => {
                        let watch_users = merged_watch_users(&known_friends, &extra_targets);
                        if let Ok(mercury_state) = try_start_mercury(&session, &watch_users, &mut shutdown_rx).await {
                            transport = ActiveTransport::Mercury(mercury_state);
                            emit_transport_changed(
                                &event_tx,
                                &mut last_mode,
                                TransportMode::MercuryPush,
                                Some("Upgraded back to Mercury push".to_string()),
                            );
                        }
                    }
                    TransportPreference::Poll => {}
                }
                next_tick = next_tick_instant(
                    &transport,
                    poll_interval_secs,
                    resync_interval_secs,
                    consecutive_errors,
                );
                next_upgrade = next_upgrade_instant(&transport, config.transport);
            }
            ev = next_transport_event(&mut transport) => {
                match ev {
                    TransportTickEvent::Dealer(DealerEvent::Connection(msg)) => {
                        let conn_id = extract_connection_id(&msg.headers);
                        if config.debug_push {
                            eprintln!(
                                "[debug-push] source=dealer-reconn uri={} headers={:?} connection_id={:?}",
                                msg.uri, msg.headers, conn_id
                            );
                        }
                        if let Some(conn_id) = conn_id {
                            let (old_conn_id, old_users) = match &transport {
                                ActiveTransport::Dealer(dealer) if dealer.connection_id != conn_id => (
                                    Some(dealer.connection_id.clone()),
                                    dealer.subscribed_users.iter().cloned().collect::<Vec<_>>(),
                                ),
                                _ => (None, Vec::new()),
                            };
                            if let Some(old_id) = old_conn_id {
                                unsubscribe_listening_activity(
                                    &session,
                                    &old_id,
                                    &old_users,
                                    config.debug_push,
                                )
                                .await;
                            }

                            let watch_users = merged_watch_users(&known_friends, &extra_targets);
                            let usernames: Vec<String> = watch_users.iter().cloned().collect();
                            let reg_res = tokio::select! {
                                _ = wait_for_shutdown(&mut shutdown_rx) => break,
                                res = timeout(
                                    LISTENING_ACTIVITY_SUB_TIMEOUT,
                                    subscribe_listening_activity(
                                        &session,
                                        &conn_id,
                                        &usernames,
                                        config.debug_push,
                                    ),
                                ) => res,
                            };
                            match reg_res {
                                Ok(Ok(sub_resp)) => {
                                    consecutive_errors = 0;
                                    dealer_stale_resyncs = 0;
                                    if let ActiveTransport::Dealer(dealer) = &mut transport {
                                        dealer.connection_id = conn_id;
                                        dealer.subscribed_users = usernames.into_iter().collect();
                                        dealer.renew_deadline = if dealer.subscribed_users.is_empty() {
                                            None
                                        } else {
                                            Some(
                                                TokioInstant::now()
                                                    + compute_subscription_renew_delay(
                                                        sub_resp.expiration_ms,
                                                        Utc::now().timestamp_millis(),
                                                    ),
                                            )
                                        };
                                    }
                                    let emitted = emit_subscribe_entries(
                                        &session,
                                        sub_resp.entries,
                                        &mut name_cache,
                                        &mut known_friends,
                                        &mut activity_signatures,
                                        &event_tx,
                                    )
                                    .await;
                                    if !emitted {
                                        let _ = perform_full_sync(
                                            &session,
                                            &mut transport,
                                            &mut name_cache,
                                            &mut known_friends,
                                            &extra_targets,
                                            &mut activity_signatures,
                                            &event_tx,
                                            &mut consecutive_errors,
                                            config.debug_push,
                                            &mut shutdown_rx,
                                        )
                                        .await;
                                    }
                                }
                                Ok(Err(e)) => {
                                    teardown_transport(&session, &mut transport, config.debug_push).await;
                                    let reason = format!("Dealer re-subscription failed: {e}");
                                    transport = downgrade_after_dealer(
                                        &session,
                                        config.transport,
                                        &watch_users,
                                        &event_tx,
                                        &mut last_mode,
                                        reason,
                                        &mut shutdown_rx,
                                    )
                                    .await;
                                    next_tick = next_tick_instant(
                                        &transport,
                                        poll_interval_secs,
                                        resync_interval_secs,
                                        consecutive_errors,
                                    );
                                    next_upgrade = next_upgrade_instant(&transport, config.transport);
                                }
                                Err(_) => {
                                    teardown_transport(&session, &mut transport, config.debug_push).await;
                                    let reason = "Dealer re-subscription timed out".to_string();
                                    transport = downgrade_after_dealer(
                                        &session,
                                        config.transport,
                                        &watch_users,
                                        &event_tx,
                                        &mut last_mode,
                                        reason,
                                        &mut shutdown_rx,
                                    )
                                    .await;
                                    next_tick = next_tick_instant(
                                        &transport,
                                        poll_interval_secs,
                                        resync_interval_secs,
                                        consecutive_errors,
                                    );
                                    next_upgrade = next_upgrade_instant(&transport, config.transport);
                                }
                            }
                        }
                    }
                    TransportTickEvent::Dealer(DealerEvent::Activity(msg)) => {
                        dealer_push_seen_since_resync = true;
                        dealer_stale_resyncs = 0;
                        if let Some(parsed) = parse_dealer_message(&msg, config.debug_push) {
                            handle_parsed_push(
                                &session,
                                parsed,
                                TransportMode::DealerPush,
                                &mut name_cache,
                                &mut known_friends,
                                &mut activity_signatures,
                                &event_tx,
                                &mut invalidate_deadline,
                                &mut consecutive_errors,
                            )
                            .await;
                        }
                    }
                    TransportTickEvent::Dealer(DealerEvent::Closed) => {
                        teardown_transport(&session, &mut transport, config.debug_push).await;
                        let watch_users = merged_watch_users(&known_friends, &extra_targets);
                        transport = downgrade_after_dealer(
                            &session,
                            config.transport,
                            &watch_users,
                            &event_tx,
                            &mut last_mode,
                            "Dealer WebSocket stream closed".to_string(),
                            &mut shutdown_rx,
                        )
                        .await;
                        next_tick = next_tick_instant(
                            &transport,
                            poll_interval_secs,
                            resync_interval_secs,
                            consecutive_errors,
                        );
                        next_upgrade = next_upgrade_instant(&transport, config.transport);
                    }
                    TransportTickEvent::Mercury(Some(MercuryEvent::Message(resp))) => {
                        if let Some(parsed) = parse_mercury_response(&resp, config.debug_push) {
                            handle_parsed_push(
                                &session,
                                parsed,
                                TransportMode::MercuryPush,
                                &mut name_cache,
                                &mut known_friends,
                                &mut activity_signatures,
                                &event_tx,
                                &mut invalidate_deadline,
                                &mut consecutive_errors,
                            )
                            .await;
                        }
                    }
                    TransportTickEvent::Mercury(Some(MercuryEvent::SubClosed(username))) => {
                        let all_closed = if let ActiveTransport::Mercury(mercury) = &mut transport {
                            mercury.subscribed_users.remove(&username);
                            mercury.tasks.retain(|h| !h.is_finished());
                            mercury.subscribed_users.is_empty()
                        } else {
                            false
                        };
                        if all_closed {
                            teardown_transport(&session, &mut transport, config.debug_push).await;
                            emit_transport_changed(
                                &event_tx,
                                &mut last_mode,
                                TransportMode::HttpPolling,
                                Some("All Mercury subscriptions closed".to_string()),
                            );
                            next_tick = next_tick_instant(
                                &transport,
                                poll_interval_secs,
                                resync_interval_secs,
                                consecutive_errors,
                            );
                            next_upgrade = next_upgrade_instant(&transport, config.transport);
                        }
                    }
                    TransportTickEvent::Mercury(None) => {
                        teardown_transport(&session, &mut transport, config.debug_push).await;
                        emit_transport_changed(
                            &event_tx,
                            &mut last_mode,
                            TransportMode::HttpPolling,
                            Some("Mercury push channel closed".to_string()),
                        );
                        next_tick = next_tick_instant(
                            &transport,
                            poll_interval_secs,
                            resync_interval_secs,
                            consecutive_errors,
                        );
                        next_upgrade = next_upgrade_instant(&transport, config.transport);
                    }
                }
            }
        }
    }

    teardown_transport(&session, &mut transport, config.debug_push).await;
    session.shutdown();
}

async fn downgrade_after_dealer(
    session: &Session,
    preference: TransportPreference,
    watch_users: &BTreeSet<String>,
    event_tx: &mpsc::UnboundedSender<ActivityEvent>,
    last_mode: &mut Option<TransportMode>,
    dealer_reason: String,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> ActiveTransport {
    if preference == TransportPreference::Auto && !*shutdown_rx.borrow() {
        match try_start_mercury(session, watch_users, shutdown_rx).await {
            Ok(mercury_state) => {
                emit_transport_changed(
                    event_tx,
                    last_mode,
                    TransportMode::MercuryPush,
                    Some(dealer_reason),
                );
                return ActiveTransport::Mercury(mercury_state);
            }
            Err(e) => {
                emit_transport_changed(
                    event_tx,
                    last_mode,
                    TransportMode::HttpPolling,
                    Some(format!("{dealer_reason}; Mercury push unavailable: {e}")),
                );
                return ActiveTransport::HttpPolling;
            }
        }
    }

    emit_transport_changed(
        event_tx,
        last_mode,
        TransportMode::HttpPolling,
        Some(dealer_reason),
    );
    ActiveTransport::HttpPolling
}

pub fn spawn_activity_stream_with_targets(
    cache_dir: PathBuf,
    config: StreamConfig,
    refresh_notify: Arc<Notify>,
    shutdown_rx: watch::Receiver<bool>,
    extra_targets_rx: watch::Receiver<BTreeSet<String>>,
) -> (
    mpsc::UnboundedReceiver<ActivityEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let handle = tokio::spawn(run_activity_stream(
        cache_dir,
        config,
        refresh_notify,
        shutdown_rx,
        extra_targets_rx,
        event_tx,
    ));
    (event_rx, handle)
}

pub fn spawn_activity_stream(
    cache_dir: PathBuf,
    config: StreamConfig,
    refresh_notify: Arc<Notify>,
    shutdown_rx: watch::Receiver<bool>,
) -> (
    mpsc::UnboundedReceiver<ActivityEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (_extra_tx, extra_rx) = watch::channel(BTreeSet::new());
    spawn_activity_stream_with_targets(cache_dir, config, refresh_notify, shutdown_rx, extra_rx)
}

#[cfg(test)]
mod listen_along_target_tests {
    use super::*;

    #[test]
    fn normalize_watch_username_handles_uri_url_bare_and_empty() {
        assert_eq!(
            normalize_watch_username("spotify:user:alice").as_deref(),
            Some("alice")
        );
        assert_eq!(
            normalize_watch_username("  spotify:user:bob_42  ").as_deref(),
            Some("bob_42")
        );
        assert_eq!(normalize_watch_username("carol").as_deref(), Some("carol"));
        assert_eq!(
            normalize_watch_username("https://open.spotify.com/user/dave?si=xyz").as_deref(),
            Some("dave")
        );
        assert_eq!(
            normalize_watch_username("http://open.spotify.com/user/eve/").as_deref(),
            Some("eve")
        );
        assert_eq!(normalize_watch_username(""), None);
        assert_eq!(normalize_watch_username("   "), None);
        assert_eq!(normalize_watch_username("spotify:user:"), None);
    }

    #[test]
    fn merged_watch_users_combines_and_deduplicates_normalized_usernames() {
        let friends = BTreeSet::from([
            "alice".to_string(),
            "spotify:user:bob".to_string(),
            "   ".to_string(),
        ]);
        let extras = BTreeSet::from([
            "spotify:user:bob".to_string(),
            "charlie".to_string(),
            "https://open.spotify.com/user/dave?si=123".to_string(),
            "spotify:user:".to_string(),
        ]);

        let merged = merged_watch_users(&friends, &extras);
        assert_eq!(
            merged,
            BTreeSet::from([
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string(),
                "dave".to_string(),
            ])
        );
    }

    #[test]
    fn normalize_extra_targets_drops_invalid_and_normalizes() {
        let raw = BTreeSet::from([
            "spotify:user:target1".to_string(),
            "target2".to_string(),
            "".to_string(),
            "  spotify:user:target1  ".to_string(),
        ]);

        assert_eq!(
            normalize_extra_targets(&raw),
            BTreeSet::from(["target1".to_string(), "target2".to_string()])
        );
    }
}
