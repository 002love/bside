# spotify-friend-logger

A fast, single-binary Rust CLI that logs Spotify Friend Activity into a local SQLite database using `librespot` — **no `sp_dc` browser cookies, no headless browser, and no external database server required**.

## Why `librespot`?

Most Spotify Friend Activity tools require manually copying and refreshing `sp_dc` cookies from a web browser. `spotify-friend-logger` authenticates once through `librespot-oauth` and connects directly to Spotify's internal `spclient` live feed (`/listening-activity/v1/feed`, with fallback to `/presence-view/v1/buddylist`), caching reusable credentials locally in a single self-contained Rust binary with embedded SQLite (`rusqlite` bundled).

## Features

- **Live all-friends logger** — watches every friend sharing activity by default (or filter to specific friends by name or URI) with automatic deduplication (`UNIQUE(user_uri, track_uri, played_at_ms)`) and smart exponential backoff on transient errors.
- **Rich track & podcast metadata** — resolves batched protobuf metadata (`TRACK_V4` & `EPISODE_V4`) for all artists, duration, popularity, explicit flag, release date, album/show info, ISRC, cover art, and listening context.
- **Skip, repeat & session tracking** — stateful live watcher tracks listening sessions, detects skips vs. completed plays when tracks change, infers consecutive repeat loops on tracks played continuously past their duration, and closes open sessions cleanly on `Ctrl+C`.
- **Hidden playlist rebuilding** — captures listening contexts including **Liked Songs** (`:collection`) and inaccessible private playlists, then reconstructs them into a terminal table, paste-ready Spotify URLs (`--format urls`), `.m3u8`, CSV, or JSON.
- **Profile & playlist change tracking** — periodically snapshots friend profiles (followers, following, recently played artists, public playlists) and maintains a chronological audit log (`changes`) of display name renames, follower deltas, added/removed public playlists, and newly discovered artists.
- **Wrapped-style terminal analytics** — listening time, active days, current/longest streaks, session length, skip rate, popularity extremes (most obscure & most mainstream), top tracks/artists, release decades, weekday/hourly Unicode bar charts, and a **7×24 activity heatmap**.
- **Friend taste overlap & trendsetter detection** — computes pairwise artist/track Jaccard compatibility scores, shared artists/tracks, and a trendsetter leaderboard showing who played shared tracks first.

## Quickstart

```bash
# Build (or run directly with `cargo run -- ...`)
cargo build --release

# 1. Authorize once in your browser (credentials cached locally)
spotify-friend-logger login

# 2. See who is currently sharing listening activity
spotify-friend-logger friends

# 3. Start watching all friends (default: 15s poll, 30m profile sync)
spotify-friend-logger watch
```

By default, credentials and the SQLite database are stored in your OS local data directory (`~/.local/share/spotify-friend-logger/` on Linux, `~/Library/Application Support/spotify-friend-logger/` on macOS). Override with global flags `--cache-dir <DIR>` and `--db <PATH>`.

## Commands

### `login` — Authorize with Spotify

```bash
spotify-friend-logger login
spotify-friend-logger login --no-browser --redirect-uri http://127.0.0.1:8898/login
```

### `friends` — Live Friend Activity snapshot

Shows `[LIVE]` / `[PODCAST]` badges, all artists, duration, `[E]` explicit flag, release year, popularity, context (`[PRIVATE]` / owner), local timestamp, and relative age.

```bash
spotify-friend-logger friends
spotify-friend-logger friends --friend "Alice"
spotify-friend-logger friends --raw   # Raw JSON from spclient for debugging
```

### `watch` — Continuous logger with skip/repeat/session & profile tracking

```bash
# Watch all friends every 15s, syncing profiles every 30 minutes
spotify-friend-logger watch

# Watch specific friend(s) with custom intervals
spotify-friend-logger watch --friend "Alice" --friend "Bob" --interval 15 --profile-interval 1800

# Disable periodic profile sync or cap inferred repeat loops
spotify-friend-logger watch --profile-interval 0 --max-inferred-repeats 5
```

### `profiles` — View or refresh friend profiles

```bash
spotify-friend-logger profiles
spotify-friend-logger profiles --friend "Alice" --refresh
```

### `changes` — Profile & playlist changelog

```bash
spotify-friend-logger changes --limit 30
spotify-friend-logger changes --friend "Alice" --days 30
```

### `contexts` — Discover public & private listening contexts

```bash
spotify-friend-logger contexts
spotify-friend-logger contexts --friend "Alice" --private-only
```

### `rebuild` — Reconstruct Liked Songs or private playlists

```bash
# View as a terminal table
spotify-friend-logger rebuild --friend "Alice" --context "liked_songs"

# Export paste-ready Spotify URLs (paste directly into a Spotify Desktop playlist)
spotify-friend-logger rebuild --friend "Alice" --context "spotify:playlist:..." --format urls

# Export as M3U8, CSV, or JSON to a file
spotify-friend-logger rebuild --friend "Alice" --context "Late Night" --format m3u8 --out late-night.m3u8
spotify-friend-logger rebuild --friend "Alice" --context "liked_songs" --format csv --out liked.csv
spotify-friend-logger rebuild --friend "Alice" --context "liked_songs" --format json --out liked.json
```

### `stats` — Wrapped-style terminal analytics & 7×24 heatmap

```bash
spotify-friend-logger stats
spotify-friend-logger stats --friend "Alice" --days 30
```

### `overlap` — Friend taste compatibility & trendsetter leaderboard

```bash
spotify-friend-logger overlap --days 30
spotify-friend-logger overlap --friend "Alice" --friend "Bob" --days 90
```

### `history` — Recent listening log

```bash
spotify-friend-logger history --limit 30
spotify-friend-logger history --friend "Alice" --days 7 --limit 50
```

### `export` — Export full listening history (CSV or JSON)

Includes all enriched metadata columns (`duration_ms`, `listened_ms`, `was_skipped`, `inferred_repeat`, `popularity`, `explicit`, `release_date`, `context_type`, `context_owner`, `context_is_private`, `spotify_url`, etc.).

```bash
spotify-friend-logger export --format csv --out history.csv
spotify-friend-logger export --format json --friend "Alice" --days 30 --out alice.json
```

## Polling Limits & Disclaimer

- **Polling interval**: `watch` defaults to 15 seconds (minimum enforced: 10 seconds) and uses exponential backoff (up to 10 minutes) on transient or rate-limit errors. Avoid running multiple watchers against the same account simultaneously.
- **Privacy & visibility**: Only friends who have opted into **"Share my listening activity on Spotify"** appear in the Friend Activity feed (the exact same data displayed in the official Spotify desktop client).
- **Unofficial API**: This project uses `librespot` and Spotify's internal `spclient` endpoints, which are undocumented and may change without notice. Use responsibly and in accordance with Spotify's Terms of Service and your friends' privacy.
