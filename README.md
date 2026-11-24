# bside

[![CI](https://github.com/002love/bside/actions/workflows/ci.yml/badge.svg)](https://github.com/002love/bside/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The flip side of Spotify — a lightweight, single-binary Rust CLI for continuous friend-circle listening analytics, skip/repeat tracking, profile change logs, and hidden playlist reconstruction. Built on [`librespot`](https://github.com/librespot-org/librespot) and embedded SQLite.

**No `sp_dc` cookies. No headless browser. No external database.**

---

## Highlights

- **Zero-cookie OAuth via `librespot`** — authenticates once in your browser, caches reusable credentials locally, and queries Spotify's live `/listening-activity/v1/feed` directly through `spclient`.
- **Full track & podcast metadata** — resolves batched protobuf metadata (`TRACK_V4` & `EPISODE_V4`) for all artists, duration, popularity, explicit flag, release date, album/show info, ISRC, cover art, and playback source context.
- **Sessions, skips & repeat loops** — tracks live `isPlaying` transitions to separate skips from full plays, measure real listening time, and infer consecutive loops when a track plays continuously past its duration.
- **Hidden playlist reconstruction** — captures playback contexts including **Liked Songs** (`:collection`) and inaccessible private playlists, then rebuilds observed tracklists into a table, paste-ready Spotify URLs, `.m3u8`, CSV, or JSON.
- **Profile & playlist changelog** — snapshots friend profiles (followers, public playlists, recently played artists) and records a chronological audit log (`changes`) of display name changes, follower deltas, and added/removed playlists.
- **Terminal analytics** — listening time, streaks, skip rate, popularity extremes, release decades, Unicode bar charts, a **7×24 activity heatmap**, and pairwise friend taste **overlap** with a **trendsetter leaderboard** (who played shared tracks first).

## Quickstart

```bash
git clone https://github.com/002love/bside.git
cd bside
cargo build --release

# 1. Authorize once in your browser
./target/release/bside login

# 2. Check who is sharing activity right now
./target/release/bside friends

# 3. Start logging all friends (15s poll, 30m profile sync)
./target/release/bside watch
```

Credentials and the SQLite database live in your OS data directory by default (`~/Library/Application Support/bside/` on macOS, `~/.local/share/bside/` on Linux). Override with `--cache-dir <DIR>` and `--db <PATH>`.

## Commands

| Command | Description |
| --- | --- |
| `bside login` | One-time browser OAuth via `librespot` (`--no-browser` for headless/VPS) |
| `bside friends` | Live Friend Activity snapshot (`--friend`, `--raw`) |
| `bside watch` | Continuous logger with skip/repeat/session tracking & profile sync |
| `bside profiles` | View or refresh (`--refresh`) friend profiles, followers & public playlists |
| `bside changes` | Chronological audit log of profile, follower & playlist changes |
| `bside contexts` | List all discovered contexts (`--private-only` for Liked Songs & private playlists) |
| `bside rebuild` | Reconstruct a playlist/context into `table`, `urls`, `m3u8`, `csv`, or `json` |
| `bside stats` | Listening overview, streaks, skip rate, charts & 7×24 activity heatmap |
| `bside overlap` | Pairwise friend taste compatibility, shared tracks & trendsetter leaderboard |
| `bside history` | Recent listening log with duration, skip & repeat badges |
| `bside export` | Export full enriched listening history to CSV or JSON |

### Examples

```bash
# Watch all friends every 15s (or narrow to specific friends with --friend)
bside watch --interval 15

# Inspect all private contexts (Liked Songs & private playlists)
bside contexts --private-only

# Rebuild a friend's Liked Songs as paste-ready Spotify URLs
bside rebuild --friend "Alice" --context "liked_songs" --format urls

# Export a reconstructed playlist as M3U8
bside rebuild --friend "Alice" --context "Late Night" --format m3u8 --out late-night.m3u8

# Terminal Wrapped & 7x24 heatmap for the last 30 days
bside stats --days 30
bside stats --friend "Alice" --days 30

# Compare taste across your friend circle
bside overlap --days 30

# Check profile & playlist changes over time
bside changes --days 30
```

## Notes & Limitations

- **Polling frequency**: `watch` defaults to 15 seconds (minimum enforced: 10s) with exponential backoff on transient or rate-limit errors. Avoid running multiple watchers on the same account.
- **Visibility**: Only friends who have enabled **"Share my listening activity on Spotify"** appear in the feed — the same data visible in the official Spotify desktop app.
- **Hidden playlist reconstruction**: Only tracks actually played while `bside watch` is running can be captured; unplayed tracks in private playlists or Liked Songs are never accessible.
- **Unofficial API**: `bside` relies on `librespot` and Spotify's internal `spclient` endpoints, which are undocumented and may change without notice. Use responsibly.

## License

[MIT](LICENSE)
