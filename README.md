<p align="center">
  <img src="favicon.svg" width="84" height="84" alt="bside logo" />
</p>

<h1 align="center">bside</h1>

<p align="center">
  <a href="https://github.com/002love/bside/actions/workflows/ci.yml"><img src="https://github.com/002love/bside/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT" /></a>
</p>

The flip side of Spotify — a lightweight, single-binary Rust CLI & embedded local web dashboard for continuous friend-circle listening analytics, skip/repeat tracking, profile change logs, and hidden playlist reconstruction. Built on [`librespot`](https://github.com/librespot-org/librespot) and embedded SQLite.

**No `sp_dc` cookies. No headless browser. No external database.**

---

## Highlights

- **Zero-cookie OAuth via `librespot`** — authenticates once in your browser, caches reusable credentials locally, and streams Friend Activity in realtime over Spotify's Dealer WebSocket / Mercury push channels with automatic fallback to `spclient` HTTP polling.
- **Full track & podcast metadata** — resolves batched protobuf metadata (`TRACK_V4` & `EPISODE_V4`) for all artists, duration, popularity, explicit flag, release date, album/show info, ISRC, cover art, and playback source context.
- **Sessions, skips & repeat loops** — tracks live `isPlaying` transitions to separate skips from full plays, measure real listening time, and infer consecutive loops when a track plays continuously past its duration.
- **Hidden playlist reconstruction** — captures playback contexts including **Liked Songs** (`:collection`) and inaccessible private playlists, then rebuilds observed tracklists into a table, paste-ready Spotify URLs, `.m3u8`, CSV, or JSON.
- **Profile & playlist changelog** — snapshots friend profiles (followers, public playlists, recently played artists) and records a chronological audit log (`changes`) of display name changes, follower deltas, and added/removed playlists.
- **Embedded local Web UI (`bside ui`)** — dark editorial dashboard bundled directly inside the single binary (`axum` + `rust-embed`) with a realtime **On Air** live feed (SSE), spinning vinyl sleeves, interactive 7×24 activity heatmaps, charts, searchable history with cover art, one-click playlist rebuild exports, and taste overlap views.
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
| `bside watch` | Foreground logger with realtime Dealer/Mercury push (+ polling fallback), skip/repeat/session tracking & profile sync (`--transport`, `--debug-push`) |
| `bside service` | Background daemon (`install`, `start`, `stop`, `status`, `logs`, `uninstall`) |
| `bside ui` | Launch the embedded local web dashboard with realtime SSE live feed (`--port 8787`, `--transport`, `--debug-push`, `--no-log`, `--offline`) |
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
# Run in foreground with automatic realtime push (Dealer WS -> Mercury -> 15s HTTP poll fallback)
bside watch --interval 15

# Override transport mode (auto, dealer, mercury, poll) or inspect raw push frames
bside watch --transport dealer --debug-push
bside watch --transport poll --interval 15

# Manage background daemon
bside service install --interval 15
bside service status
bside service logs -n 50

# Launch embedded local web UI (live feed + logging, or --no-log if `bside service` is running)
bside ui --port 8787
bside ui --transport auto
bside ui --no-log
bside ui --offline

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

- **Realtime transport & polling fallback**: `watch` and `ui` default to `--transport auto`, which prefers Spotify's realtime Dealer WebSocket push, falls back to Mercury presence subscriptions, and finally falls back to `spclient` HTTP polling (`--interval` / `--poll-interval`, default 15s, minimum enforced 10s) with exponential backoff on transient or rate-limit errors. Use `--transport <auto|dealer|mercury|poll>` to pin a transport and `--debug-push` to log raw push frames to stderr. Avoid running multiple watchers on the same account.
- **Visibility**: Only friends who have enabled **"Share my listening activity on Spotify"** appear in the feed — the same data visible in the official Spotify desktop app.
- **Hidden playlist reconstruction**: Only tracks actually played while `bside watch` is running can be captured; unplayed tracks in private playlists or Liked Songs are never accessible.
- **Unofficial API**: `bside` relies on `librespot` and Spotify's internal `spclient` endpoints, which are undocumented and may change without notice. Use responsibly.

## License

[MIT](LICENSE)
