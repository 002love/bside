//! Pure-Rust formatting, Unicode visualization, and playlist export helpers.

use std::fmt::Write as _;

const WEEKDAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const PARTIAL_BLOCKS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// Track metadata row used by playlist exporters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportTrackRow {
    pub track_uri: String,
    pub spotify_url: String,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration_ms: Option<i64>,
    pub play_count: i64,
}

/// Formats a track duration in milliseconds as `M:SS` or `H:MM:SS`.
///
/// Negative or zero durations return `"0:00"`.
pub fn format_duration_ms(ms: i64) -> String {
    if ms <= 0 {
        return "0:00".to_string();
    }
    let total_secs = ms / 1_000;
    let hours = total_secs / 3_600;
    let mins = (total_secs % 3_600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{secs:02}")
    } else {
        format!("{mins}:{secs:02}")
    }
}

/// Formats a cumulative listening duration in milliseconds in a compact
/// human-readable form such as `"<1m"`, `"42m"`, `"3h 15m"`, or `"2d 5h 10m"`.
pub fn format_total_duration_ms(ms: i64) -> String {
    if ms <= 0 {
        return "0m".to_string();
    }
    let total_mins = ms / 60_000;
    if total_mins == 0 {
        return "<1m".to_string();
    }

    let days = total_mins / (24 * 60);
    let hours = (total_mins % (24 * 60)) / 60;
    let mins = total_mins % 60;

    let mut parts = Vec::with_capacity(3);
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if mins > 0 {
        parts.push(format!("{mins}m"));
    }
    parts.join(" ")
}

/// Formats a relative age in seconds as `"just now"`, `"3m ago"`, `"2h ago"`, or `"4d ago"`.
pub fn format_relative_age_secs(age_secs: i64) -> String {
    if age_secs < 60 {
        "just now".to_string()
    } else if age_secs < 3_600 {
        format!("{}m ago", age_secs / 60)
    } else if age_secs < 86_400 {
        format!("{}h ago", age_secs / 3_600)
    } else {
        format!("{}d ago", age_secs / 86_400)
    }
}

/// Converts a Spotify URI (e.g. `"spotify:track:abc"`) into an `open.spotify.com` URL.
///
/// Unrecognized or malformed inputs are returned unchanged.
pub fn spotify_uri_to_url(uri: &str) -> String {
    let mut parts = uri.split(':');
    let first = parts.next();
    let kind = parts.next();
    let id = parts.next();
    let extra = parts.next();

    if first == Some("spotify")
        && extra.is_none()
        && let (Some(kind), Some(id)) = (kind, id)
        && !id.is_empty()
        && matches!(
            kind,
            "track" | "episode" | "playlist" | "album" | "user" | "artist" | "show"
        )
    {
        return format!("https://open.spotify.com/{kind}/{id}");
    }

    uri.to_string()
}

/// Renders a smooth horizontal Unicode bar using full (`█`) and eighth (`▏`..`▉`) blocks.
///
/// The returned string has at most `width` Unicode scalar values.
pub fn render_bar(value: i64, max_value: i64, width: usize) -> String {
    if width == 0 || value <= 0 || max_value <= 0 {
        return String::new();
    }

    let clamped = value.min(max_value);
    let max_eighths = width.saturating_mul(8);
    let raw_eighths = ((clamped as f64 / max_value as f64) * max_eighths as f64).round() as usize;
    let eighths = raw_eighths.clamp(1, max_eighths);

    let full = eighths / 8;
    let rem = eighths % 8;

    let mut out = String::with_capacity(full * 3 + if rem > 0 { 3 } else { 0 });
    for _ in 0..full {
        out.push('█');
    }
    if rem > 0 {
        out.push(PARTIAL_BLOCKS[rem - 1]);
    }
    out
}

/// Renders a 24-hour (`00`..`23`) horizontal bar chart.
///
/// If all 24 buckets are zero (or negative), returns a compact empty indicator.
pub fn render_hourly_chart(hourly: &[i64; 24], bar_width: usize) -> String {
    let max_val = hourly.iter().copied().max().unwrap_or(0).max(0);
    if max_val == 0 {
        return "(no activity)".to_string();
    }

    let count_width = max_val.to_string().len();
    let mut out = String::new();
    for (hour, &count) in hourly.iter().enumerate() {
        let count = count.max(0);
        let bar = render_bar(count, max_val, bar_width);
        let pad = bar_width.saturating_sub(bar.chars().count());
        if hour > 0 {
            out.push('\n');
        }
        let _ = write!(
            out,
            "{hour:02} │ {bar}{spaces} {count:>count_width$}",
            spaces = " ".repeat(pad),
        );
    }
    out
}

/// Renders a Monday-through-Sunday horizontal bar chart.
pub fn render_weekday_chart(weekday: &[i64; 7], bar_width: usize) -> String {
    let max_val = weekday.iter().copied().max().unwrap_or(0).max(0);
    let count_width = max_val.to_string().len();
    let mut out = String::new();
    for (idx, (&label, &count)) in WEEKDAY_LABELS.iter().zip(weekday.iter()).enumerate() {
        let count = count.max(0);
        let bar = render_bar(count, max_val, bar_width);
        let pad = bar_width.saturating_sub(bar.chars().count());
        if idx > 0 {
            out.push('\n');
        }
        let _ = write!(
            out,
            "{label} │ {bar}{spaces} {count:>count_width$}",
            spaces = " ".repeat(pad),
        );
    }
    out
}

/// Renders a compact 7×24 (Mon..Sun × 00..23) terminal heatmap using `·`, `░`, `▒`, `▓`, `█`.
pub fn render_activity_heatmap(grid: &[[i64; 24]; 7]) -> String {
    let max_val = grid
        .iter()
        .flat_map(|row| row.iter().copied())
        .max()
        .unwrap_or(0)
        .max(0);

    let mut out = String::new();
    out.push_str("    00 03 06 09 12 15 18 21\n");

    for (label, row) in WEEKDAY_LABELS.iter().zip(grid.iter()) {
        out.push_str(label);
        out.push(' ');
        for &val in row {
            out.push(heatmap_glyph(val, max_val));
        }
        out.push('\n');
    }

    out.push_str("    Less · ░ ▒ ▓ █ More");
    out
}

fn heatmap_glyph(value: i64, max_value: i64) -> char {
    if value <= 0 || max_value <= 0 {
        return '·';
    }
    let ratio = value as f64 / max_value as f64;
    if ratio <= 0.25 {
        '░'
    } else if ratio <= 0.50 {
        '▒'
    } else if ratio <= 0.75 {
        '▓'
    } else {
        '█'
    }
}

/// Formats tracks as an extended M3U8 playlist (`#EXTM3U`, `#PLAYLIST`, `#EXTINF`, URL).
pub fn format_m3u8(playlist_title: &str, tracks: &[ExportTrackRow]) -> String {
    let mut out = String::new();
    out.push_str("#EXTM3U\n");
    let _ = writeln!(out, "#PLAYLIST:{}", sanitize_single_line(playlist_title));

    for track in tracks {
        let secs = match track.duration_ms {
            Some(ms) if ms >= 0 => ms / 1_000,
            _ => -1,
        };
        let artist = sanitize_single_line(&track.artist);
        let title = sanitize_single_line(&track.title);
        let url = resolve_track_url(track);
        let _ = writeln!(out, "#EXTINF:{secs},{artist} - {title}");
        let _ = writeln!(out, "{url}");
    }

    out
}

/// Formats tracks as one `https://open.spotify.com/...` URL per line,
/// ready to paste into Spotify Desktop to rebuild a playlist.
pub fn format_spotify_urls(tracks: &[ExportTrackRow]) -> String {
    if tracks.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (idx, track) in tracks.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(&resolve_track_url(track));
    }
    out.push('\n');
    out
}

fn resolve_track_url(track: &ExportTrackRow) -> String {
    if !track.spotify_url.is_empty() {
        track.spotify_url.clone()
    } else {
        spotify_uri_to_url(&track.track_uri)
    }
}

fn sanitize_single_line(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
        .collect()
}

/// Safely truncates a UTF-8 string to at most `max_chars` Unicode scalar values,
/// appending `…` when truncation occurs.
pub fn truncate_ellipsis(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }

    let mut out: String = s.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tracks() -> Vec<ExportTrackRow> {
        vec![
            ExportTrackRow {
                track_uri: "spotify:track:abc123".to_string(),
                spotify_url: "https://open.spotify.com/track/abc123".to_string(),
                artist: "Daft Punk".to_string(),
                title: "Veridis Quo".to_string(),
                album: "Discovery".to_string(),
                duration_ms: Some(345_000),
                play_count: 14,
            },
            ExportTrackRow {
                track_uri: "spotify:track:xyz789".to_string(),
                spotify_url: String::new(),
                artist: "Кино".to_string(),
                title: "Группа крови".to_string(),
                album: "Группа крови".to_string(),
                duration_ms: None,
                play_count: 7,
            },
        ]
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(-5_000), "0:00");
        assert_eq!(format_duration_ms(0), "0:00");
        assert_eq!(format_duration_ms(999), "0:00");
        assert_eq!(format_duration_ms(5_000), "0:05");
        assert_eq!(format_duration_ms(204_000), "3:24");
        assert_eq!(format_duration_ms(3_725_000), "1:02:05");
    }

    #[test]
    fn test_format_total_duration_ms() {
        assert_eq!(format_total_duration_ms(-100), "0m");
        assert_eq!(format_total_duration_ms(0), "0m");
        assert_eq!(format_total_duration_ms(45_000), "<1m");
        assert_eq!(format_total_duration_ms(42 * 60_000), "42m");
        assert_eq!(format_total_duration_ms((3 * 60 + 15) * 60_000), "3h 15m");
        assert_eq!(
            format_total_duration_ms(((2 * 24 + 5) * 60 + 10) * 60_000),
            "2d 5h 10m"
        );
        assert_eq!(format_total_duration_ms(24 * 60 * 60_000), "1d");
    }

    #[test]
    fn test_format_relative_age_secs() {
        assert_eq!(format_relative_age_secs(-10), "just now");
        assert_eq!(format_relative_age_secs(0), "just now");
        assert_eq!(format_relative_age_secs(59), "just now");
        assert_eq!(format_relative_age_secs(60), "1m ago");
        assert_eq!(format_relative_age_secs(185), "3m ago");
        assert_eq!(format_relative_age_secs(7_200), "2h ago");
        assert_eq!(format_relative_age_secs(4 * 86_400 + 123), "4d ago");
    }

    #[test]
    fn test_spotify_uri_to_url() {
        assert_eq!(
            spotify_uri_to_url("spotify:track:abc"),
            "https://open.spotify.com/track/abc"
        );
        assert_eq!(
            spotify_uri_to_url("spotify:episode:xyz"),
            "https://open.spotify.com/episode/xyz"
        );
        assert_eq!(
            spotify_uri_to_url("spotify:playlist:123"),
            "https://open.spotify.com/playlist/123"
        );
        assert_eq!(
            spotify_uri_to_url("spotify:album:456"),
            "https://open.spotify.com/album/456"
        );
        assert_eq!(
            spotify_uri_to_url("spotify:user:bob"),
            "https://open.spotify.com/user/bob"
        );
        assert_eq!(
            spotify_uri_to_url("spotify:artist:789"),
            "https://open.spotify.com/artist/789"
        );
        assert_eq!(
            spotify_uri_to_url("spotify:show:s01"),
            "https://open.spotify.com/show/s01"
        );
        assert_eq!(
            spotify_uri_to_url("https://open.spotify.com/track/abc"),
            "https://open.spotify.com/track/abc"
        );
        assert_eq!(spotify_uri_to_url("spotify:track:"), "spotify:track:");
        assert_eq!(
            spotify_uri_to_url("spotify:unknown:abc"),
            "spotify:unknown:abc"
        );
        assert_eq!(
            spotify_uri_to_url("spotify:track:abc:extra"),
            "spotify:track:abc:extra"
        );
    }

    #[test]
    fn test_render_bar_full_and_fractional() {
        assert_eq!(render_bar(0, 10, 10), "");
        assert_eq!(render_bar(-5, 10, 10), "");
        assert_eq!(render_bar(5, 0, 10), "");
        assert_eq!(render_bar(5, 10, 0), "");

        assert_eq!(render_bar(10, 10, 5), "█████");
        assert_eq!(render_bar(20, 10, 5), "█████"); // clamped
        assert_eq!(render_bar(1, 2, 5), "██▌"); // 2.5 blocks = 2 full + 4/8 (▌)
        assert_eq!(render_bar(1, 8, 1), "▏");
        assert_eq!(render_bar(3, 8, 1), "▍");
        assert_eq!(render_bar(7, 8, 1), "▉");
        // Tiny positive value still renders minimum 1/8th hairline block.
        assert_eq!(render_bar(1, 10_000, 10), "▏");
    }

    #[test]
    fn test_render_hourly_chart() {
        let empty = [0i64; 24];
        assert_eq!(render_hourly_chart(&empty, 10), "(no activity)");

        let mut hourly = [0i64; 24];
        hourly[0] = 5;
        hourly[12] = 10;
        hourly[23] = 2;

        let chart = render_hourly_chart(&hourly, 10);
        let lines: Vec<&str> = chart.lines().collect();
        assert_eq!(lines.len(), 24);
        assert!(lines[0].starts_with("00 │ █████"));
        assert!(lines[0].ends_with(" 5"));
        assert!(lines[12].starts_with("12 │ ██████████"));
        assert!(lines[12].ends_with("10"));
        assert!(lines[23].starts_with("23 │ ██"));
        assert!(lines[23].ends_with(" 2"));
    }

    #[test]
    fn test_render_weekday_chart() {
        let counts = [10, 5, 0, 8, 2, 10, 4];
        let chart = render_weekday_chart(&counts, 10);
        let lines: Vec<&str> = chart.lines().collect();
        assert_eq!(lines.len(), 7);
        assert!(lines[0].starts_with("Mon │ ██████████ 10"));
        assert!(lines[1].starts_with("Tue │ █████       5"));
        assert!(lines[2].starts_with("Wed │             0"));
        assert!(lines[6].starts_with("Sun │ ████        4"));
    }

    #[test]
    fn test_render_activity_heatmap() {
        let mut grid = [[0i64; 24]; 7];
        grid[0][0] = 1; // 0.25 -> ░
        grid[0][1] = 2; // 0.50 -> ▒
        grid[0][2] = 3; // 0.75 -> ▓
        grid[0][3] = 4; // 1.00 -> █

        let map = render_activity_heatmap(&grid);
        let lines: Vec<&str> = map.lines().collect();
        assert_eq!(lines.len(), 9);
        assert_eq!(lines[0], "    00 03 06 09 12 15 18 21");
        assert!(lines[1].starts_with("Mon ░▒▓█····················"));
        assert_eq!(lines[2], "Tue ························");
        assert_eq!(lines[7], "Sun ························");
        assert_eq!(lines[8], "    Less · ░ ▒ ▓ █ More");
    }

    #[test]
    fn test_format_m3u8() {
        let tracks = sample_tracks();
        let m3u8 = format_m3u8("Late Night Mix", &tracks);
        let expected = "\
#EXTM3U
#PLAYLIST:Late Night Mix
#EXTINF:345,Daft Punk - Veridis Quo
https://open.spotify.com/track/abc123
#EXTINF:-1,Кино - Группа крови
https://open.spotify.com/track/xyz789
";
        assert_eq!(m3u8, expected);
    }

    #[test]
    fn test_format_spotify_urls() {
        assert_eq!(format_spotify_urls(&[]), "");

        let tracks = sample_tracks();
        let urls = format_spotify_urls(&tracks);
        assert_eq!(
            urls,
            "https://open.spotify.com/track/abc123\nhttps://open.spotify.com/track/xyz789\n"
        );
    }

    #[test]
    fn test_truncate_ellipsis_utf8_cyrillic_and_emoji() {
        assert_eq!(truncate_ellipsis("hello", 0), "");
        assert_eq!(truncate_ellipsis("hello", 1), "…");
        assert_eq!(truncate_ellipsis("hello", 5), "hello");
        assert_eq!(truncate_ellipsis("hello world", 5), "hell…");

        // Cyrillic
        assert_eq!(truncate_ellipsis("Группа крови", 7), "Группа…");
        assert_eq!(truncate_ellipsis("Кино", 4), "Кино");

        // Emoji
        assert_eq!(truncate_ellipsis("🎵🎶🎸🎹🥁", 3), "🎵🎶…");
        assert_eq!(truncate_ellipsis("🎵🎶", 2), "🎵🎶");
    }
}
