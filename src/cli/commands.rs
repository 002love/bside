use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;

use crate::db::{Database, escape_csv, format_timestamp_local};
use crate::format::{
    ExportTrackRow, format_duration_ms, format_m3u8, format_relative_age_secs,
    format_spotify_urls, format_total_duration_ms, render_activity_heatmap, render_bar,
    render_hourly_chart, render_weekday_chart, spotify_uri_to_url, truncate_ellipsis,
};
use crate::spotify::{
    NameCache, connect_cached, fetch_buddylist, fetch_buddylist_raw, fetch_user_profile,
    login_oauth,
};

use super::watch::print_profile_diff;
use super::{ExportFormat, RebuildFormat, format_context_badge};

fn format_release_year(release_date: Option<&str>) -> Option<&str> {
    release_date.and_then(|d| d.get(0..4)).filter(|y| y.len() == 4)
}

pub(super) async fn run_login(
    cache_dir: &Path,
    no_browser: bool,
    redirect_uri: Option<String>,
) -> Result<()> {
    println!("Starting Spotify OAuth login...");
    let session = login_oauth(cache_dir, !no_browser, redirect_uri.as_deref()).await?;
    println!(
        "Logged in as '{}' — credentials saved to {}",
        session.username(),
        cache_dir.display()
    );
    session.shutdown();
    Ok(())
}

pub(super) async fn run_friends(
    cache_dir: &Path,
    friend: Vec<String>,
    raw: bool,
) -> Result<()> {
    let session = connect_cached(cache_dir).await?;
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

    let now_secs = Utc::now().timestamp();
    println!("Found {} friend(s) sharing activity:\n", matched.len());
    for item in matched {
        let ts = format_timestamp_local(item.timestamp);
        let age_secs = (now_secs - (item.timestamp / 1_000)).max(0);
        let age_str = format_relative_age_secs(age_secs);

        let mut badges = Vec::new();
        if item.playing {
            badges.push("[LIVE]");
        }
        if item.item_type() == "episode" {
            badges.push("[PODCAST]");
        }
        let badge_str = if badges.is_empty() {
            String::new()
        } else {
            format!(" {}", badges.join(" "))
        };

        let mut meta_parts = Vec::new();
        if let Some(dur) = item.duration_ms() {
            meta_parts.push(format_duration_ms(dur));
        }
        if item.track.explicit == Some(true) {
            meta_parts.push("[E]".to_string());
        }
        if let Some(year) = format_release_year(item.track.release_date.as_deref()) {
            meta_parts.push(year.to_string());
        }
        if let Some(pop) = item.track.popularity {
            meta_parts.push(format!("pop: {pop}"));
        }
        let meta_str = if meta_parts.is_empty() {
            String::new()
        } else {
            format!("  ({})", meta_parts.join(" · "))
        };

        let ctx = format_context_badge(&item);
        println!(
            "{} ({}){}\n  {} — {}{}{}\n  Last activity: {} ({})\n",
            item.user.name,
            item.user.uri,
            badge_str,
            item.all_artists_display(),
            item.track.name,
            meta_str,
            ctx,
            ts,
            age_str,
        );
    }

    Ok(())
}

pub(super) async fn run_profiles(
    cache_dir: &Path,
    db_path: &Path,
    friend: Option<String>,
    refresh: bool,
) -> Result<()> {
    let db = Database::open(db_path)?;

    if refresh {
        let session = connect_cached(cache_dir).await?;
        let mut name_cache = NameCache::default();
        let filters: Vec<String> = friend.iter().cloned().collect();
        let mut uris: BTreeSet<String> = BTreeSet::new();

        if let Ok(feed) = fetch_buddylist(&session, &mut name_cache).await {
            for item in feed.into_iter().filter(|f| f.matches_filter(&filters)) {
                uris.insert(item.user.uri);
            }
        }
        if let Some(f) = friend.as_deref()
            && f.starts_with("spotify:user:")
        {
            uris.insert(f.to_string());
        }
        // Also refresh any already-stored profiles matching the filter.
        if let Ok(existing) = db.query_profiles(friend.as_deref()) {
            for p in existing {
                uris.insert(p.user_uri);
            }
        }

        if uris.is_empty() {
            println!("No matching friends found to refresh.");
        } else {
            println!("Refreshing {} friend profile(s)...", uris.len());
            for uri in &uris {
                match fetch_user_profile(&session, uri).await {
                    Ok(snap) => match db.upsert_profile(&snap) {
                        Ok(diff) => print_profile_diff(&snap.user_name, &diff),
                        Err(e) => eprintln!("[error] upsert_profile failed: {e}"),
                    },
                    Err(e) => eprintln!("[warn] Failed to fetch profile for {uri}: {e}"),
                }
            }
            println!();
        }
        session.shutdown();
    }

    let profiles = db.query_profiles(friend.as_deref())?;
    if profiles.is_empty() {
        println!(
            "No stored profiles found. Run `bside profiles --refresh` or `watch` first."
        );
        return Ok(());
    }

    for p in profiles {
        let url = spotify_uri_to_url(&p.user_uri);
        let followers = p
            .followers_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        let following = p
            .following_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());

        println!("=== {} ===", p.user_name);
        println!("  URI:       {} ({})", p.user_uri, url);
        println!("  Social:    {followers} followers · {following} following");
        println!(
            "  Updated:   {} (first seen {})",
            format_timestamp_local(p.updated_at_ms),
            format_timestamp_local(p.first_seen_ms)
        );

        if !p.recently_played_artists.is_empty() {
            let artists: Vec<&str> = p
                .recently_played_artists
                .iter()
                .map(|a| a.name.as_str())
                .filter(|s| !s.is_empty())
                .collect();
            if !artists.is_empty() {
                println!("  Recent artists: {}", artists.join(", "));
            }
        }

        if !p.public_playlists.is_empty() {
            println!("  Public playlists ({}):", p.public_playlists.len());
            for pl in &p.public_playlists {
                let pl_followers = pl
                    .followers_count
                    .map(|n| format!(" ({n} followers)"))
                    .unwrap_or_default();
                println!(
                    "    • {}{} — {}",
                    pl.name,
                    pl_followers,
                    spotify_uri_to_url(&pl.uri)
                );
            }
        }
        println!();
    }

    Ok(())
}

pub(super) fn run_changes(
    db_path: &Path,
    friend: Option<String>,
    days: Option<i64>,
    limit: usize,
) -> Result<()> {
    let db = Database::open(db_path)?;
    let events = db.query_profile_events(friend.as_deref(), days, Some(limit))?;
    if events.is_empty() {
        println!("No profile or playlist changes recorded yet.");
        return Ok(());
    }

    for ev in events {
        let badge = match ev.event_type.as_str() {
            "rename" => "RENAME",
            "followers" => "FOLLOWERS",
            "playlist_added" => "+PLAYLIST",
            "playlist_removed" => "-PLAYLIST",
            "recent_artist" => "ARTIST",
            other => other,
        };
        println!(
            "[{}] [{:<9}] {} — {}",
            ev.detected_at_local, badge, ev.user_name, ev.summary
        );
    }

    Ok(())
}

pub(super) fn run_contexts(
    db_path: &Path,
    friend: Option<String>,
    private_only: bool,
) -> Result<()> {
    let db = Database::open(db_path)?;
    let contexts = db.query_contexts(friend.as_deref(), private_only)?;
    if contexts.is_empty() {
        println!("No matching listening contexts found.");
        return Ok(());
    }

    println!("Discovered {} context(s):\n", contexts.len());
    for c in contexts {
        let title = if !c.context_name.is_empty() {
            c.context_name.clone()
        } else {
            c.context_uri.clone()
        };
        let priv_badge = if c.is_private { " [PRIVATE]" } else { "" };
        let owner_str = c
            .context_owner
            .as_deref()
            .map(|o| format!(" (by {o})"))
            .unwrap_or_default();
        let url_suffix = if !c.context_uri.is_empty() {
            let u = spotify_uri_to_url(&c.context_uri);
            if u != c.context_uri {
                format!("  {u}")
            } else {
                format!("  {}", c.context_uri)
            }
        } else {
            String::new()
        };

        println!(
            "{} | [{}]{} {}{}",
            c.user_name,
            c.context_type,
            priv_badge,
            truncate_ellipsis(&title, 48),
            owner_str,
        );
        println!(
            "  {} unique tracks · {} plays · last seen {}{}",
            c.unique_tracks,
            c.total_plays,
            format_timestamp_local(c.last_seen_ms),
            url_suffix,
        );
    }

    Ok(())
}

pub(super) fn run_rebuild(
    db_path: &Path,
    friend: Option<String>,
    context: String,
    format: RebuildFormat,
    out: Option<PathBuf>,
) -> Result<()> {
    let db = Database::open(db_path)?;
    let tracks = db.rebuild_playlist(friend.as_deref(), &context)?;
    if tracks.is_empty() {
        println!("No tracks found matching context '{context}'.");
        return Ok(());
    }

    let export_rows: Vec<ExportTrackRow> = tracks
        .iter()
        .map(|t| ExportTrackRow {
            track_uri: t.track_uri.clone(),
            spotify_url: t.spotify_url.clone(),
            artist: t.all_artists.clone(),
            title: t.track_name.clone(),
            album: t.album_name.clone().unwrap_or_default(),
            duration_ms: t.duration_ms,
            play_count: t.play_count,
        })
        .collect();

    let mut writer: Box<dyn Write> = match &out {
        Some(path) => Box::new(
            File::create(path)
                .with_context(|| format!("Failed to create {}", path.display()))?,
        ),
        None => Box::new(io::stdout().lock()),
    };

    match format {
        RebuildFormat::Table => {
            writeln!(
                writer,
                "Rebuilt '{}' — {} unique track(s):\n",
                context,
                tracks.len()
            )?;
            for (i, (t, row)) in tracks.iter().zip(export_rows.iter()).enumerate() {
                let dur = row
                    .duration_ms
                    .map(format_duration_ms)
                    .unwrap_or_else(|| "?:??".to_string());
                let year = format_release_year(t.release_date.as_deref()).unwrap_or("----");
                let album_part = if !row.album.is_empty() {
                    format!(" · {}", truncate_ellipsis(&row.album, 24))
                } else {
                    String::new()
                };
                writeln!(
                    writer,
                    "{:>3}. [{:>2}x] {} — {} ({}, {}{})\n     {}",
                    i + 1,
                    row.play_count,
                    truncate_ellipsis(&row.artist, 32),
                    truncate_ellipsis(&row.title, 36),
                    dur,
                    year,
                    album_part,
                    row.spotify_url,
                )?;
            }
        }
        RebuildFormat::Urls => {
            write!(writer, "{}", format_spotify_urls(&export_rows))?;
        }
        RebuildFormat::M3u8 => {
            write!(writer, "{}", format_m3u8(&context, &export_rows))?;
        }
        RebuildFormat::Csv => {
            writeln!(
                writer,
                "play_count,artist,track,album,duration_ms,release_date,popularity,track_uri,spotify_url"
            )?;
            for t in &tracks {
                writeln!(
                    writer,
                    "{},{},{},{},{},{},{},{},{}",
                    t.play_count,
                    escape_csv(&t.all_artists),
                    escape_csv(&t.track_name),
                    escape_csv(t.album_name.as_deref().unwrap_or("")),
                    t.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
                    escape_csv(t.release_date.as_deref().unwrap_or("")),
                    t.popularity.map(|p| p.to_string()).unwrap_or_default(),
                    escape_csv(&t.track_uri),
                    escape_csv(&t.spotify_url),
                )?;
            }
        }
        RebuildFormat::Json => {
            serde_json::to_writer_pretty(&mut writer, &tracks)?;
            writeln!(writer)?;
        }
    }

    if let Some(path) = out {
        eprintln!(
            "Wrote {} rebuilt tracks for '{}' to {}",
            tracks.len(),
            context,
            path.display()
        );
    }

    Ok(())
}

pub(super) fn run_stats(db_path: &Path, friend: Option<String>, days: Option<i64>) -> Result<()> {
    let db = Database::open(db_path)?;
    let stats = db.query_stats(friend.as_deref(), days)?;

    if stats.total_plays == 0 {
        println!("No listens recorded yet for this filter.");
        return Ok(());
    }

    println!("=== Listening Stats ===");
    println!("Total plays:     {}", stats.total_plays);
    println!(
        "Listening time:  {} (from {}/{} plays with known duration)",
        format_total_duration_ms(stats.total_listened_ms),
        stats.known_duration_plays,
        stats.total_plays
    );
    println!(
        "Catalog:         {} unique tracks · {} unique artists",
        stats.unique_tracks, stats.unique_artists
    );
    println!(
        "Activity:        {} active day(s) · current streak: {}d · longest streak: {}d",
        stats.active_days, stats.current_streak_days, stats.longest_streak_days
    );
    if stats.session_count > 0 {
        let avg_str = stats
            .avg_session_ms
            .map(format_total_duration_ms)
            .unwrap_or_else(|| "n/a".to_string());
        println!(
            "Sessions:        {} session(s) · avg length: {}",
            stats.session_count, avg_str
        );
    }
    let skip_denom = stats.skipped_plays + stats.completed_plays;
    if skip_denom > 0 {
        let skip_pct = (stats.skipped_plays as f64 / skip_denom as f64) * 100.0;
        println!(
            "Skip rate:       {:.1}% ({} skipped / {} completed)",
            skip_pct, stats.skipped_plays, stats.completed_plays
        );
    }
    if let Some(avg_pop) = stats.avg_popularity {
        println!("Avg popularity:  {avg_pop:.1} / 100");
    }
    if let Some((artist, track, pop)) = &stats.most_obscure_track {
        println!("Most obscure:    {artist} — {track} (pop: {pop})");
    }
    if let Some((artist, track, pop)) = &stats.most_mainstream_track {
        println!("Most mainstream: {artist} — {track} (pop: {pop})");
    }
    if stats.explicit_plays > 0 {
        let exp_pct = (stats.explicit_plays as f64 / stats.total_plays as f64) * 100.0;
        println!(
            "Explicit tracks: {} ({:.1}%)",
            stats.explicit_plays, exp_pct
        );
    }
    if let (Some(first), Some(last)) = (stats.first_played_ms, stats.last_played_ms) {
        println!(
            "Range:           {} -> {}",
            format_timestamp_local(first),
            format_timestamp_local(last)
        );
    }

    if !stats.top_tracks.is_empty() {
        println!("\nTop Tracks:");
        for (i, (artist, track, count)) in stats.top_tracks.iter().enumerate() {
            println!(
                "  {:2}. {} — {} ({})",
                i + 1,
                truncate_ellipsis(artist, 28),
                truncate_ellipsis(track, 34),
                count
            );
        }
    }

    if !stats.top_artists.is_empty() {
        let max_cnt = stats.top_artists[0].1.max(1);
        println!("\nTop Artists:");
        for (i, (artist, count)) in stats.top_artists.iter().enumerate() {
            let bar = render_bar(*count, max_cnt, 20);
            println!(
                "  {:2}. {:<24} {:<20} ({})",
                i + 1,
                truncate_ellipsis(artist, 24),
                bar,
                count
            );
        }
    }

    if !stats.most_skipped_tracks.is_empty() {
        println!("\nMost Skipped Tracks:");
        for (i, (artist, track, count)) in stats.most_skipped_tracks.iter().enumerate() {
            println!(
                "  {:2}. {} — {} ({} skips)",
                i + 1,
                truncate_ellipsis(artist, 28),
                truncate_ellipsis(track, 34),
                count
            );
        }
    }

    if !stats.context_breakdown.is_empty() {
        let max_cnt = stats.context_breakdown[0].1.max(1);
        println!("\nContext Breakdown:");
        for (ctx, count) in &stats.context_breakdown {
            let bar = render_bar(*count, max_cnt, 20);
            println!("  {:<14} {:<20} ({})", ctx, bar, count);
        }
    }

    if !stats.release_decades.is_empty() {
        let max_cnt = stats.release_decades[0].1.max(1);
        println!("\nRelease Decades:");
        for (decade, count) in &stats.release_decades {
            let bar = render_bar(*count, max_cnt, 20);
            println!("  {:<8} {:<20} ({})", decade, bar, count);
        }
    }

    println!("\nActivity by Weekday:");
    println!("{}", render_weekday_chart(&stats.weekday, 20));

    println!("\nActivity by Hour (local time):");
    println!("{}", render_hourly_chart(&stats.hourly, 24));

    println!("\n7x24 Activity Heatmap (local time):");
    println!("{}", render_activity_heatmap(&stats.heatmap_grid));

    Ok(())
}

pub(super) fn run_overlap(db_path: &Path, days: Option<i64>, friend: Vec<String>) -> Result<()> {
    let db = Database::open(db_path)?;
    let report = db.query_overlap(days, &friend)?;

    if report.friend_pairs.is_empty() {
        println!("Need at least 2 friends with logged listens to compute taste overlap.");
        return Ok(());
    }

    println!("=== Friend Taste Compatibility ===");
    for pair in &report.friend_pairs {
        let score_pct = (pair.compatibility_score * 100.0).round() as i64;
        let artist_pct = pair.artist_jaccard * 100.0;
        let track_pct = pair.track_jaccard * 100.0;
        let bar = render_bar(score_pct, 100, 20);
        println!(
            "  {} <-> {}: {:>3}% {:<20} (artists: {:.1}% [{}/{}], tracks: {:.1}% [{}/{}])",
            truncate_ellipsis(&pair.friend_a, 18),
            truncate_ellipsis(&pair.friend_b, 18),
            score_pct,
            bar,
            artist_pct,
            pair.shared_artists,
            pair.total_union_artists,
            track_pct,
            pair.shared_tracks,
            pair.total_union_tracks,
        );
    }

    if !report.trendsetter_leaderboard.is_empty() {
        println!("\nTrendsetter Leaderboard (first to play shared tracks):");
        let max_firsts = report.trendsetter_leaderboard[0].1.max(1);
        for (i, (name, count)) in report.trendsetter_leaderboard.iter().enumerate() {
            let bar = render_bar(*count, max_firsts, 18);
            println!(
                "  {:2}. {:<20} {:<18} ({} first plays)",
                i + 1,
                truncate_ellipsis(name, 20),
                bar,
                count
            );
        }
    }

    if !report.shared_artists.is_empty() {
        println!("\nTop Shared Artists:");
        for (i, (artist, friends, plays)) in report.shared_artists.iter().take(10).enumerate() {
            println!(
                "  {:2}. {} — {} plays [{}]",
                i + 1,
                truncate_ellipsis(artist, 28),
                plays,
                friends.join(", ")
            );
        }
    }

    if !report.shared_tracks.is_empty() {
        println!("\nTop Shared Tracks:");
        for (i, t) in report.shared_tracks.iter().take(10).enumerate() {
            println!(
                "  {:2}. {} — {} ({} plays · [{}])",
                i + 1,
                truncate_ellipsis(&t.artist_name, 24),
                truncate_ellipsis(&t.track_name, 30),
                t.total_plays,
                t.friends_who_played.join(", "),
            );
            println!(
                "      First played by {} on {} ({})",
                t.first_player_name,
                format_timestamp_local(t.first_played_ms),
                spotify_uri_to_url(&t.track_uri),
            );
        }
    }

    Ok(())
}

pub(super) fn run_history(
    db_path: &Path,
    friend: Option<String>,
    days: Option<i64>,
    limit: usize,
) -> Result<()> {
    let db = Database::open(db_path)?;
    let records = db.query_history(friend.as_deref(), days, Some(limit))?;
    if records.is_empty() {
        println!("No history records found.");
        return Ok(());
    }

    for r in records {
        let artists = r
            .all_artists
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&r.artist_name);
        let dur = r
            .duration_ms
            .map(|ms| format!(" ({})", format_duration_ms(ms)))
            .unwrap_or_default();

        let mut badges = Vec::new();
        if r.item_type == "episode" {
            badges.push("[PODCAST]");
        }
        if r.explicit == Some(true) {
            badges.push("[E]");
        }
        if r.was_skipped == Some(true) {
            badges.push("[SKIP]");
        }
        if r.inferred_repeat {
            badges.push("[REPEAT]");
        }
        let badge_str = if badges.is_empty() {
            String::new()
        } else {
            format!(" {}", badges.join(" "))
        };

        let ctx = r
            .context_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|c| {
                if r.context_is_private {
                    format!(" [{c} (PRIVATE)]")
                } else {
                    format!(" [{c}]")
                }
            })
            .unwrap_or_default();

        println!(
            "[{}] {} | {} — {}{}{}{}",
            r.played_at_local, r.user_name, artists, r.track_name, badge_str, dur, ctx
        );
    }

    Ok(())
}

pub(super) fn run_export(
    db_path: &Path,
    format: ExportFormat,
    out: Option<PathBuf>,
    friend: Option<String>,
    days: Option<i64>,
) -> Result<()> {
    let db = Database::open(db_path)?;
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
                "played_at_local,played_at_ms,user_name,user_uri,item_type,artist_name,all_artists,track_name,album_name,duration_ms,listened_ms,was_skipped,inferred_repeat,popularity,explicit,release_date,context_name,context_type,context_owner,context_is_private,track_uri,spotify_url"
            )?;
            for r in &records {
                let spotify_url = spotify_uri_to_url(&r.track_uri);
                writeln!(
                    writer,
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    escape_csv(&r.played_at_local),
                    r.played_at_ms,
                    escape_csv(&r.user_name),
                    escape_csv(&r.user_uri),
                    escape_csv(&r.item_type),
                    escape_csv(&r.artist_name),
                    escape_csv(r.all_artists.as_deref().unwrap_or("")),
                    escape_csv(&r.track_name),
                    escape_csv(r.album_name.as_deref().unwrap_or("")),
                    r.duration_ms.map(|v| v.to_string()).unwrap_or_default(),
                    r.listened_ms.map(|v| v.to_string()).unwrap_or_default(),
                    r.was_skipped.map(|v| v.to_string()).unwrap_or_default(),
                    r.inferred_repeat,
                    r.popularity.map(|v| v.to_string()).unwrap_or_default(),
                    r.explicit.map(|v| v.to_string()).unwrap_or_default(),
                    escape_csv(r.release_date.as_deref().unwrap_or("")),
                    escape_csv(r.context_name.as_deref().unwrap_or("")),
                    escape_csv(r.context_type.as_deref().unwrap_or("")),
                    escape_csv(r.context_owner.as_deref().unwrap_or("")),
                    r.context_is_private,
                    escape_csv(&r.track_uri),
                    escape_csv(&spotify_url),
                )?;
            }
        }
    }

    if let Some(path) = out {
        eprintln!("Exported {} records to {}", records.len(), path.display());
    }

    Ok(())
}

pub(super) fn run_repair(db_path: &Path) -> Result<()> {
    let db = Database::open(db_path)?;
    let report = db.repair_duplicate_listens_and_skips()?;
    println!(
        "Database repair complete: merged_duplicates={}, fixed_skips={}, remaining_listens={}",
        report.merged_duplicates, report.fixed_skips, report.remaining_listens
    );
    Ok(())
}
