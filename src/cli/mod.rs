mod commands;
mod watch;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::service::{self, ServiceConfig};
use crate::spotify::FriendActivity;

#[derive(Parser)]
#[command(
    name = "bside",
    version,
    about = "The flip side of Spotify — self-hosted listening analytics, skip/repeat tracking, and playlist reconstruction via librespot"
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

    /// Continuously poll Friend Activity and log listens, skips, repeats, sessions, and profile changes
    Watch {
        /// Filter to specific friend(s) by name or Spotify URI (repeatable; omit to log all)
        #[arg(short, long)]
        friend: Vec<String>,

        /// Polling interval in seconds (default 15, minimum 10)
        #[arg(short, long, default_value_t = 15)]
        interval: u64,

        /// Profile sync interval in seconds (default 1800, 0 to disable)
        #[arg(long, default_value_t = 1800)]
        profile_interval: u64,

        /// Maximum consecutive inferred repeat cycles per single continuous track play
        #[arg(long, default_value_t = 10)]
        max_inferred_repeats: u32,
    },

    /// Show stored friend profiles, public playlists, and recently played artists
    Profiles {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Fetch fresh profile snapshots from Spotify before displaying
        #[arg(long)]
        refresh: bool,
    },

    /// Show chronological changelog of friend profile and playlist changes
    Changes {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only show changes from the last N days
        #[arg(short, long)]
        days: Option<i64>,

        /// Maximum number of events to show
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// List all discovered listening contexts (Liked Songs, private/public playlists, albums, artists)
    Contexts {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only show private contexts (Liked Songs and inaccessible private playlists)
        #[arg(long)]
        private_only: bool,
    },

    /// Rebuild a hidden/private playlist or Liked Songs collection from logged listens
    Rebuild {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Context name, URI, or type substring (e.g. "liked_songs", playlist URI, or playlist name)
        #[arg(short, long)]
        context: String,

        /// Output format
        #[arg(long, value_enum, default_value_t = RebuildFormat::Table)]
        format: RebuildFormat,

        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Show rich listening statistics, streaks, charts, and 7x24 activity heatmap
    Stats {
        /// Filter by friend name or URI substring
        #[arg(short, long)]
        friend: Option<String>,

        /// Only include listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,
    },

    /// Analyze friend taste overlap, compatibility scores, and trendsetters
    Overlap {
        /// Only include listens from the last N days
        #[arg(short, long)]
        days: Option<i64>,

        /// Filter to specific friend(s) by name or Spotify URI (repeatable)
        #[arg(short, long)]
        friend: Vec<String>,
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

    /// Repair duplicate listens and false-positive skip flags in the SQLite database
    Repair,

    /// Manage the background watch daemon (macOS launchd, Linux systemd, Windows Task Scheduler)
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Launch the embedded local web dashboard
    Ui {
        /// Bind host address (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on (default: 8787)
        #[arg(long, default_value_t = 8787)]
        port: u16,

        /// Do not auto-open the dashboard in your default browser
        #[arg(long)]
        no_open: bool,

        /// Live feed polling interval in seconds (default 15, minimum 10)
        #[arg(long, default_value_t = 15)]
        poll_interval: u64,

        /// Disable live Spotify connection; read SQLite archive only
        #[arg(long)]
        offline: bool,

        /// Do not write live feed listens into SQLite from the UI process
        #[arg(long)]
        no_log: bool,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Install, enable, and start the background watch service
    Install {
        /// Filter to specific friend(s) by name or Spotify URI (repeatable; omit to log all)
        #[arg(short, long)]
        friend: Vec<String>,

        /// Polling interval in seconds (default 15, minimum 10)
        #[arg(short, long, default_value_t = 15)]
        interval: u64,

        /// Profile sync interval in seconds (default 1800, 0 to disable)
        #[arg(long, default_value_t = 1800)]
        profile_interval: u64,

        /// Maximum consecutive inferred repeat cycles per single continuous track play
        #[arg(long, default_value_t = 10)]
        max_inferred_repeats: u32,
    },

    /// Stop and remove the background watch service
    Uninstall,

    /// Start the installed background watch service
    Start,

    /// Stop the running background watch service
    Stop,

    /// Show the current status of the background watch service
    Status,

    /// Show recent log output from the background watch service
    Logs {
        /// Number of trailing log lines to display
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum ExportFormat {
    Csv,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum RebuildFormat {
    Table,
    Urls,
    M3u8,
    Csv,
    Json,
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bside")
}

fn resolve_cache_dir(arg: Option<PathBuf>) -> PathBuf {
    arg.unwrap_or_else(|| default_data_dir().join("cache"))
}

fn resolve_db_path(arg: Option<PathBuf>) -> PathBuf {
    arg.unwrap_or_else(|| default_data_dir().join("history.db"))
}

pub(super) fn format_context_badge(item: &FriendActivity) -> String {
    let ctx_name = item.context_name();
    let ctx_uri = item.context_uri();
    let is_private = item.is_private_context();
    let owner = item.context_owner();

    let label = match (ctx_name, ctx_uri) {
        (Some(name), _) => name.to_string(),
        (None, Some(uri)) if is_private => format!("Private Playlist ({uri})"),
        (None, Some(uri)) => uri.to_string(),
        (None, None) => return String::new(),
    };

    let mut details = Vec::new();
    if is_private {
        details.push("PRIVATE".to_string());
    }
    if let Some(o) = owner {
        details.push(format!("by {o}"));
    }

    if details.is_empty() {
        format!(" [{label}]")
    } else {
        format!(" [{label} ({})]", details.join(", "))
    }
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let cache_dir = resolve_cache_dir(cli.cache_dir.clone());
    let db_path = resolve_db_path(cli.db.clone());

    match cli.command {
        Commands::Login {
            no_browser,
            redirect_uri,
        } => commands::run_login(&cache_dir, no_browser, redirect_uri).await?,

        Commands::Friends { friend, raw } => commands::run_friends(&cache_dir, friend, raw).await?,

        Commands::Watch {
            friend,
            interval,
            profile_interval,
            max_inferred_repeats,
        } => {
            watch::run_watch(
                &cache_dir,
                &db_path,
                friend,
                interval,
                profile_interval,
                max_inferred_repeats,
            )
            .await?
        }

        Commands::Profiles { friend, refresh } => {
            commands::run_profiles(&cache_dir, &db_path, friend, refresh).await?
        }

        Commands::Changes {
            friend,
            days,
            limit,
        } => commands::run_changes(&db_path, friend, days, limit)?,

        Commands::Contexts {
            friend,
            private_only,
        } => commands::run_contexts(&db_path, friend, private_only)?,

        Commands::Rebuild {
            friend,
            context,
            format,
            out,
        } => commands::run_rebuild(&db_path, friend, context, format, out)?,

        Commands::Stats { friend, days } => commands::run_stats(&db_path, friend, days)?,

        Commands::Overlap { days, friend } => commands::run_overlap(&db_path, days, friend)?,

        Commands::History {
            friend,
            days,
            limit,
        } => commands::run_history(&db_path, friend, days, limit)?,

        Commands::Export {
            format,
            out,
            friend,
            days,
        } => commands::run_export(&db_path, format, out, friend, days)?,

        Commands::Repair => commands::run_repair(&db_path)?,

        Commands::Service { action } => match action {
            ServiceAction::Install {
                friend,
                interval,
                profile_interval,
                max_inferred_repeats,
            } => {
                let binary_path = std::env::current_exe()
                    .context("Failed to determine path to current bside executable")?;
                let config = ServiceConfig {
                    binary_path,
                    cache_dir: cli.cache_dir,
                    db_path: cli.db,
                    log_path: service::default_log_path(),
                    friends: friend,
                    interval,
                    profile_interval,
                    max_inferred_repeats,
                };
                service::install(&config, &cache_dir)?;
            }
            ServiceAction::Uninstall => service::uninstall()?,
            ServiceAction::Start => service::start()?,
            ServiceAction::Stop => service::stop()?,
            ServiceAction::Status => service::status()?,
            ServiceAction::Logs { lines } => service::logs(&service::default_log_path(), lines)?,
        },

        Commands::Ui {
            host,
            port,
            no_open,
            poll_interval,
            offline,
            no_log,
        } => {
            let config = crate::web::UiConfig {
                cache_dir,
                db_path,
                host,
                port,
                open_browser: !no_open,
                poll_interval_secs: poll_interval,
                offline,
                log_to_db: !no_log,
            };
            crate::web::run_server(config).await?
        }
    }

    Ok(())
}
