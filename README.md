# bside

[![CI](https://github.com/002love/bside/actions/workflows/ci.yml/badge.svg)](https://github.com/002love/bside/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The flip side of Spotify — a lightweight, single-binary Rust CLI & embedded local web dashboard for continuous friend-circle listening analytics, skip/repeat tracking, profile change logs, and hidden playlist reconstruction. Built on [`librespot`](https://github.com/librespot-org/librespot) and embedded SQLite.

**No `sp_dc` cookies. No headless browser. No external database.**

---

## Highlights

- **Zero-cookie OAuth via `librespot`** — authenticates once in your browser, caches reusable credentials locally, and queries Spotify's live `/listening-activity/v1/feed` directly through `spclient`.
- **Full track & podcast metadata** — resolves batched protobuf metadata (`TRACK_V4` & `EPISODE_V4`) for all artists, duration, popularity, explicit flag, release date, album/show info, ISRC, cover art, and playback source context.
- **Sessions, skips & repeat loops** — tracks live `isPlaying` transitions to separate skips from full plays, measure real listening time, and infer consecutive loops when a track plays continuously past its duration.
- **Hidden playlist reconstruction** — captures playback contexts including **Liked Songs** (`:collection`) and inaccessible private playlists, then rebuilds observed tracklists into a table, paste-ready Spotify URLs, `.m3u8`, CSV, or JSON.
- **Profile & playlist changelog** — snapshots friend profiles (followers, public playlists, recently played artists) and records a chronological audit log (`changes`) of display name changes, follower deltas, and added/removed playlists.
- **Embedded local Web UI (`bside ui`)** — dark-mode `shadcn/ui` dashboard bundled directly inside the single binary (`axum` + `rust-embed`) with interactive 7×24 activity heatmaps, charts, searchable history with cover art, one-click playlist rebuild exports, and taste overlap views.
- **Cross-platform background daemon (`bside service`)** — one-command persistent background logging via `launchd` (macOS), `systemd --user` (Linux), and Task Scheduler (Windows).

## Install

### macOS & Linux

```bash
curl -fsSL https://raw.githubusercontent.com/002love/bside/main/scripts/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/002love/bside/main/scripts/install.ps1 | iex
```

### Build from source

```bash
git clone https://github.com/002love/bside.git
cd bside
(cd web && npm ci && npm run build)
cargo build --release
```

## Quickstart

```bash
# 1. Authorize once in your browser (credentials cached locally)
bside login

# 2. Check who is sharing activity right now
bside friends

# 3. Run continuously in the background (auto-starts on login)
bside service install

# 4. Open the local web dashboard at http://127.0.0.1:8787
bside ui
```

Credentials and the SQLite database live in your OS data directory by default (`~/Library/Application Support/bside/` on macOS, `~/.local/share/bside/` on Linux, `%LOCALAPPDATA%\bside\` on Windows). Override with `--cache-dir <DIR>` and `--db <PATH>`.

## Commands

| Command | Description |
| --- | --- |
| `bside login` | One-time browser OAuth via `librespot` (`--no-browser` for headless/VPS) |
| `bside friends` | Live Friend Activity snapshot (`--friend`, `--raw`) |
| `bside watch` | Foreground logger with skip/repeat/session tracking & profile sync |
| `bside service` | Background daemon (`install`, `start`, `stop`, `status`, `logs`, `uninstall`) |
| `bside ui` | Launch the embedded local web dashboard (`--port 8787`, `--no-open`) |
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
# Run in foreground every 15s (or narrow to specific friends with --friend)
bside watch --interval 15

# Manage background daemon
bside service install --interval 15
bside service status
bside service logs -n 50

# Launch embedded local web UI
bside ui --port 8787

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

## Roadmap

- [x] Single-binary Rust CLI with zero-cookie `librespot` OAuth and embedded SQLite
- [x] Full track & podcast metadata, skip/repeat/session tracking, and profile/playlist changelog
- [x] Hidden playlist reconstruction (`Liked Songs` & private playlists) with URL/M3U8/CSV/JSON export
- [x] Terminal analytics: streaks, popularity extremes, 7×24 activity heatmap, and friend taste overlap
- [x] Prebuilt cross-platform releases (Windows, macOS, Linux) with one-line `curl` / PowerShell installers
- [x] Background service installer (`bside service install` via `launchd`, `systemd --user`, and Windows Task Scheduler)
- [x] Embedded local Web UI (`bside ui`) built with `shadcn/ui` and served directly from the single binary
- [ ] Lightweight desktop tray app (Tauri) wrapping the local daemon and dashboard
- [ ] Personal listening history & local Wrapped export alongside friend-circle analytics

## Notes & Limitations

- **Polling frequency**: `watch` defaults to 15 seconds (minimum enforced: 10s) with exponential backoff on transient or rate-limit errors. Avoid running multiple watchers on the same account.
- **Visibility**: Only friends who have enabled **"Share my listening activity on Spotify"** appear in the feed — the same data visible in the official Spotify desktop app.
- **Hidden playlist reconstruction**: Only tracks actually played while `bside watch` is running can be captured; unplayed tracks in private playlists or Liked Songs are never accessible.
- **Unofficial API**: `bside` relies on `librespot` and Spotify's internal `spclient` endpoints, which are undocumented and may change without notice. Use responsibly.

## License

[MIT](LICENSE)
