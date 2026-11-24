# spotify-friend-logger

Lightweight single-binary CLI utility in Rust that logs Spotify Friend Activity into SQLite using `librespot` OAuth (no browser cookies required).

## Features

- **No `sp_dc` cookie needed** — authenticates once via browser OAuth (`librespot-oauth`) and caches reusable session credentials locally.
- **Filter by one or multiple friends** (by display name or Spotify URI), or log everyone sharing activity.
- **SQLite storage** with automatic deduplication (`UNIQUE(user_uri, track_uri, played_at_ms)`).
- **Built-in CLI analytics**: recent history, top tracks, top artists, hourly activity chart, and CSV/JSON export.

## Usage

```bash
# 1. Authorize once in your browser
cargo run -- login

# 2. List friends currently sharing activity (to see exact names / URIs)
cargo run -- friends

# 3. Start logging one friend (or omit --friend to log all)
cargo run -- watch --friend "Friend Name" --interval 45

# 4. View history & stats
cargo run -- history --friend "Friend Name" --limit 30
cargo run -- stats --friend "Friend Name" --days 30

# 5. Export to CSV or JSON
cargo run -- export --format csv --out history.csv
```

## Notes

- Only friends who have enabled **"Share my listening activity on Spotify"** appear in Friend Activity (the same feed shown in the official desktop app).
- Uses Spotify's internal `spclient` live feed (`/listening-activity/v1/feed`, with fallback to `/presence-view/v1/buddylist`) via `librespot`.
