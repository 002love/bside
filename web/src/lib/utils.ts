import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || ms < 0) return '--:--';
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  }
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

export function formatTotalTime(ms: number | null | undefined): string {
  if (ms == null || ms <= 0) return '0m';
  const totalMinutes = Math.floor(ms / 60000);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) {
    return `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return `${minutes}m`;
}

export function formatRelativeTime(ms: number | null | undefined): string {
  if (ms == null || ms <= 0) return 'Never';
  const diffSec = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (diffSec < 60) return 'Just now';
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay < 30) return `${diffDay}d ago`;
  return new Date(ms).toLocaleDateString();
}

export function spotifyUriToUrl(uri: string | null | undefined): string | null {
  if (!uri) return null;
  if (uri.startsWith('https://')) return uri;
  const parts = uri.split(':');
  if (parts.length === 3 && parts[0] === 'spotify' && parts[2]) {
    if (['track', 'episode', 'album', 'artist', 'playlist', 'show', 'user'].includes(parts[1])) {
      return `https://open.spotify.com/${parts[1]}/${parts[2]}`;
    }
  }
  return null;
}

export function escapeCsvCell(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n') || value.includes('\r')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

export function downloadTextFile(filename: string, content: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

export function formatContextLabel(type: string | null | undefined): string {
  switch ((type ?? '').toLowerCase()) {
    case 'liked_songs':
      return 'Liked Songs';
    case 'dj':
      return 'DJ';
    case 'playlist':
      return 'Playlist';
    case 'album':
      return 'Album';
    case 'artist':
      return 'Artist';
    case 'station':
    case 'radio':
      return 'Radio';
    case 'show':
      return 'Podcast';
    default:
      return type ? type.replace(/_/g, ' ') : 'Unknown';
  }
}
