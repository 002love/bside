mod db;
mod spotify;

use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tokio::time::sleep;

use crate::db::{Database, escape_csv, format_timestamp_local};
use crate::spotify::{NameCache, connect_cached, fetch_buddylist, fetch_buddylist_raw, login_oauth};

#[derive(Parser)]
#[command(
    name = "spotify-friend-logger",
    version,
    about = "Logs Spotify Friend Activity via librespot (no browser cookies required)"
)]
struct Cli {
    /// Directory for cached Spotify session credentials
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,

    /// Path to SQLite database file
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authorize with Spotify via browser OAuth (saved locally for future runs)
    Login {
        /// Do not auto-open browser; only print the OAuth URL
        #[arg(long)]
        no_browser: bool,

        /// Custom OAuth redirect URI (default: http://127.0.0.1:8898/login)
        #[arg(long)]
        redirect_uri: Option<String>,
    },

    /// List all friends currently visible in your Spotify Friend Activity
    Friends {
        /// Optional filter by friend name or URI substring
        #[arg(short, long)]
        friend: Vec<String>,

        /// Print raw JSON response from Spotify spclient (for debugging)
        #[arg(long)]
        raw: bool,
    },

    /// Continuously poll Friend Activity and log new listens to SQLite
    Watch {
        /// Filter to specific friend(s) by name or Spotify URI (repeatable; omit to log all)
        #[arg(short, long)]
        friend: Vec<String>,

        /// Polling interval in seconds (minimum 15)
        #[arg(short, long, default_value_t = 45)]
        interval: u64,
    },

    /// Show logged listening history from SQLite
    History {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only show listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,

        /// Maximum number of rows to show
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// Show listening statistics (top tracks, top artists, hourly activity)
    Stats {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only include listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,
    },

    /// Export listening history to CSV or JSON
    Export {
        /// Output format
        #[arg(short, long, value_enum, default_value_t = ExportFormat::Csv)]
        format: ExportFormat,

        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Filter by friend name or URI substring
        #[arg(short = 'F', long)]
        friend: Option<String>,

        /// Only include listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ExportFormat {
    Csv,
    Json,
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("spotify-friend-logger")
}

fn resolve_cache_dir(arg: Option<PathBuf>) -> PathBuf {
    arg.unwrap_or_else(|| default_data_dir().join("cache"))
}

fn resolve_db_path(arg: Option<PathBuf>) -> PathBuf {
    arg.unwrap_or_else(|| default_data_dir().join("history.db"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cache_dir = resolve_cache_dir(cli.cache_dir);
    let db_path = resolve_db_path(cli.db);

    match cli.command {
        Commands::Login {
            no_browser,
            redirect_uri,
        } => {
            println!("Starting Spotify OAuth login...");
            let session = login_oauth(&cache_dir, !no_browser, redirect_uri.as_deref()).await?;
            println!(
                "Logged in as '{}' — credentials saved to {}",
                session.username(),
                cache_dir.display()
            );
            session.shutdown();
        }

        Commands::Friends { friend, raw } => {
            let session = connect_cached(&cache_dir).await?;
            if raw {
                let base_url = session.spclient().base_url().await.unwrap_or_default();
                eprintln!(
                    "Account: {} | Country: {} | SpClient: {}",
                    session.username(),
                    session.country(),
                    base_url
                );
                let body = fetch_buddylist_raw(&session).await?;
                session.shutdown();
                println!("{body}");
                return Ok(());
            }
            let mut name_cache = NameCache::default();
            let friends = fetch_buddylist(&session, &mut name_cache).await?;
            session.shutdown();

            let matched: Vec<_> = friends
                .into_iter()
                .filter(|f| f.matches_filter(&friend))
                .collect();

            if matched.is_empty() {
                println!("No matching friends found in Friend Activity.");
                return Ok(());
            }

            println!("Found {} friend(s) sharing activity:\n", matched.len());
            for item in matched {
                let ts = format_timestamp_local(item.timestamp);
                let status = if item.playing { " [LIVE]" } else { "" };
                let ctx = item
                    .context_name()
                    .map(|c| format!("  [{}]", c))
                    .unwrap_or_default();
                println!(
                    "{} ({}){}\n  {} — {}{}\n  Last activity: {}\n",
                    item.user.name,
                    item.user.uri,
                    status,
                    item.artist_name(),
                    item.track.name,
                    ctx,
                    ts
                );
            }
        }

        Commands::Watch { friend, interval } => {
            let interval_secs = interval.max(15);
            let db = Database::open(&db_path)?;
            let mut session = connect_cached(&cache_dir).await?;

            if friend.is_empty() {
                println!(
                    "Watching ALL friends every {}s -> {}",
                    interval_secs,
                    db_path.display()
                );
            } else {
                println!(
                    "Watching [{}] every {}s -> {}",
                    friend.join(", "),
                    interval_secs,
                    db_path.display()
                );
            }
            println!("Press Ctrl+C to stop.\n");

            let mut consecutive_errors: u32 = 0;
            let mut name_cache = NameCache::default();

            loop {
                if session.is_invalid() {
                    eprintln!("[warn] Spotify session disconnected, reconnecting...");
                    match connect_cached(&cache_dir).await {
                        Ok(new_session) => {
                            session = new_session;
                            eprintln!("[info] Reconnected.");
                        }
                        Err(e) => {
                            eprintln!("[error] Reconnect failed: {e}");
                        }
                    }
                }

                match fetch_buddylist(&session, &mut name_cache).await {
                    Ok(list) => {
                        consecutive_errors = 0;
                        for item in list.into_iter().filter(|f| f.matches_filter(&friend)) {
                            match db.insert_listen(&item) {
                                Ok(true) => {
                                    let ts = format_timestamp_local(item.timestamp);
                                    let ctx = item
                                        .context_name()
                                        .map(|c| format!(" [{}]", c))
                                        .unwrap_or_default();
                                    println!(
                                        "[{}] {} | {} — {}{}",
                                        ts,
                                        item.user.name,
                                        item.artist_name(),
                                        item.track.name,
                                        ctx
                                    );
                                }
                                Ok(false) => {}
                                Err(e) => eprintln!("[error] DB insert failed: {e}"),
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors = consecutive_errors.saturating_add(1);
                        eprintln!("[error] Poll failed (#{consecutive_errors}): {e}");
                        if consecutive_errors >= 3 {
                            session.shutdown();
                        }
                    }
                }

                let backoff_secs = if consecutive_errors > 0 {
                    (interval_secs * (1u64 << consecutive_errors.min(4))).min(600)
                } else {
                    interval_secs
                };

                tokio::select! {
                    _ = sleep(Duration::from_secs(backoff_secs)) => {}
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nStopping watcher...");
                        break;
                    }
                }
            }

            session.shutdown();
        }

        Commands::History {
            friend,
            days,
            limit,
        } => {
            let db = Database::open(&db_path)?;
            let records = db.query_history(friend.as_deref(), days, Some(limit))?;
            if records.is_empty() {
                println!("No history records found.");
                return Ok(());
            }

            for r in records {
                let ctx = r
                    .context_name
                    .as_deref()
                    .map(|c| format!(" [{}]", c))
                    .unwrap_or_default();
                println!(
                    "[{}] {} | {} — {}{}",
                    r.played_at_local, r.user_name, r.artist_name, r.track_name, ctx
                );
            }
        }

        Commands::Stats { friend, days } => {
            let db = Database::open(&db_path)?;
            let stats = db.query_stats(friend.as_deref(), days)?;

            if stats.total_plays == 0 {
                println!("No listens recorded yet for this filter.");
                return Ok(());
            }

            println!("=== Listening Stats ===");
            println!("Total plays:    {}", stats.total_plays);
            println!("Unique tracks:  {}", stats.unique_tracks);
            println!("Unique artists: {}", stats.unique_artists);
            if let (Some(first), Some(last)) = (stats.first_played_ms, stats.last_played_ms) {
                println!(
                    "Range:          {} -> {}",
                    format_timestamp_local(first),
                    format_timestamp_local(last)
                );
            }

            println!("\nTop Tracks:");
            for (i, (artist, track, count)) in stats.top_tracks.iter().enumerate() {
                println!("  {:2}. {} — {} ({})", i + 1, artist, track, count);
            }

            println!("\nTop Artists:");
            for (i, (artist, count)) in stats.top_artists.iter().enumerate() {
                println!("  {:2}. {} ({})", i + 1, artist, count);
            }

            let max_hour = stats.hourly.iter().copied().max().unwrap_or(1).max(1);
            println!("\nActivity by Hour (local time):");
            for (hour, &count) in stats.hourly.iter().enumerate() {
                if count == 0 {
                    continue;
                }
                let bar_len = ((count as f64 / max_hour as f64) * 24.0).round() as usize;
                let bar = "#".repeat(bar_len.max(1));
                println!("  {:02}:00 | {:<24} ({})", hour, bar, count);
            }
        }

        Commands::Export {
            format,
            out,
            friend,
            days,
        } => {
            let db = Database::open(&db_path)?;
            let records = db.query_history(friend.as_deref(), days, None)?;

            let mut writer: Box<dyn Write> = match &out {
                Some(path) => Box::new(
                    File::create(path)
                        .with_context(|| format!("Failed to create {}", path.display()))?,
                ),
                None => Box::new(io::stdout().lock()),
            };

            match format {
                ExportFormat::Json => {
                    serde_json::to_writer_pretty(&mut writer, &records)?;
                    writeln!(writer)?;
                }
                ExportFormat::Csv => {
                    writeln!(
                        writer,
                        "played_at_local,played_at_ms,user_name,user_uri,artist_name,track_name,album_name,context_name,track_uri"
                    )?;
                    for r in &records {
                        writeln!(
                            writer,
                            "{},{},{},{},{},{},{},{},{}",
                            escape_csv(&r.played_at_local),
                            r.played_at_ms,
                            escape_csv(&r.user_name),
                            escape_csv(&r.user_uri),
                            escape_csv(&r.artist_name),
                            escape_csv(&r.track_name),
                            escape_csv(r.album_name.as_deref().unwrap_or("")),
                            escape_csv(r.context_name.as_deref().unwrap_or("")),
                            escape_csv(&r.track_uri),
                        )?;
                    }
                }
            }

            if let Some(path) = out {
                eprintln!("Exported {} records to {}", records.len(), path.display());
            }
        }
    }

    Ok(())
}
