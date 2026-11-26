import { useEffect, useState } from 'react';

export interface AccentPalette {
  /** Vivid, legible accent color for progress bars, dancing EQ bars, active highlights */
  accent: string;
  /** Soft translucent tint for pills/buttons/glows */
  soft: string;
  /** Rich saturated top gradient color for full-bleed hero washes */
  wash: string;
  /** Mid-tone fade color blending hero into #09090b */
  washMid: string;
  /** RGB tuple string "r, g, b" for custom alpha composition */
  rgb: string;
}

const DEFAULT_PALETTE: AccentPalette = {
  accent: '#e88b5a',
  soft: 'rgba(232, 139, 90, 0.2)',
  wash: 'rgba(138, 66, 36, 0.55)',
  washMid: 'rgba(62, 30, 18, 0.28)',
  rgb: '232, 139, 90',
};

const FALLBACK_HUES = [18, 345, 212, 152, 268, 36, 188, 326] as const;

const colorCache = new Map<string, AccentPalette>();
const inflight = new Map<string, Promise<AccentPalette>>();

function hashString(input: string): number {
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) >>> 0;
  }
  return hash;
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;

  if (max === min) return [0, 0, l];

  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  switch (max) {
    case rn:
      h = (gn - bn) / d + (gn < bn ? 6 : 0);
      break;
    case gn:
      h = (bn - rn) / d + 2;
      break;
    default:
      h = (rn - gn) / d + 4;
      break;
  }
  return [h * 60, s, l];
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const hn = ((h % 360) + 360) % 360;
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((hn / 60) % 2) - 1));
  const m = l - c / 2;

  let rp = 0;
  let gp = 0;
  let bp = 0;

  if (hn < 60) [rp, gp, bp] = [c, x, 0];
  else if (hn < 120) [rp, gp, bp] = [x, c, 0];
  else if (hn < 180) [rp, gp, bp] = [0, c, x];
  else if (hn < 240) [rp, gp, bp] = [0, x, c];
  else if (hn < 300) [rp, gp, bp] = [x, 0, c];
  else [rp, gp, bp] = [c, 0, x];

  return [
    Math.round((rp + m) * 255),
    Math.round((gp + m) * 255),
    Math.round((bp + m) * 255),
  ];
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

export function buildPaletteFromHsl(h: number, s: number, l: number): AccentPalette {
  const isNeutral = s < 0.08;
  // Vivid, high-contrast accent for bars, progress fills, and active text
  const accentS = isNeutral ? 0.08 : clamp(Math.max(s * 1.15, 0.62), 0.56, 0.9);
  const accentL = isNeutral ? 0.78 : clamp(l, 0.58, 0.68);
  const [ar, ag, ab] = hslToRgb(h, accentS, accentL);

  // Rich, atmospheric wash for Spotify-style full-bleed hero gradients
  const washS = isNeutral ? 0.06 : clamp(Math.max(s * 1.05, 0.48), 0.44, 0.78);
  const washL = isNeutral ? 0.24 : 0.3;
  const [wr, wg, wb] = hslToRgb(h, washS, washL);

  const [mr, mg, mb] = hslToRgb(h, washS * 0.85, 0.16);

  return {
    accent: `rgb(${ar}, ${ag}, ${ab})`,
    soft: `rgba(${ar}, ${ag}, ${ab}, 0.2)`,
    wash: `rgba(${wr}, ${wg}, ${wb}, 0.62)`,
    washMid: `rgba(${mr}, ${mg}, ${mb}, 0.34)`,
    rgb: `${ar}, ${ag}, ${ab}`,
  };
}

export function paletteFromRgbInt(colorInt: number): AccentPalette {
  const r = (colorInt >> 16) & 255;
  const g = (colorInt >> 8) & 255;
  const b = colorInt & 255;
  const [h, s, l] = rgbToHsl(r, g, b);
  return buildPaletteFromHsl(h, s, l);
}

export function fallbackPaletteForSeed(seed?: string | null): AccentPalette {
  if (!seed) return DEFAULT_PALETTE;
  const hue = FALLBACK_HUES[hashString(seed) % FALLBACK_HUES.length];
  return buildPaletteFromHsl(hue, 0.68, 0.62);
}

async function extractPaletteFromImage(imageUrl: string, seed?: string | null): Promise<AccentPalette> {
  const cached = colorCache.get(imageUrl);
  if (cached) return cached;

  const existing = inflight.get(imageUrl);
  if (existing) return existing;

  const promise = new Promise<AccentPalette>((resolve) => {
    if (typeof window === 'undefined' || typeof document === 'undefined') {
      resolve(fallbackPaletteForSeed(seed ?? imageUrl));
      return;
    }

    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.decoding = 'async';

    const finishFallback = () => {
      const fb = fallbackPaletteForSeed(seed ?? imageUrl);
      colorCache.set(imageUrl, fb);
      inflight.delete(imageUrl);
      resolve(fb);
    };

    img.onerror = finishFallback;
    img.onload = () => {
      try {
        const size = 24;
        const canvas = document.createElement('canvas');
        canvas.width = size;
        canvas.height = size;
        const ctx = canvas.getContext('2d', { willReadFrequently: true });
        if (!ctx) {
          finishFallback();
          return;
        }
        ctx.drawImage(img, 0, 0, size, size);
        const { data } = ctx.getImageData(0, 0, size, size);

        // Bucket hues into 24 sectors weighted by vibrancy so the true signature cover color wins
        const buckets = Array.from({ length: 24 }, () => ({
          weight: 0,
          rSum: 0,
          gSum: 0,
          bSum: 0,
        }));

        let totalR = 0;
        let totalG = 0;
        let totalB = 0;
        let totalCount = 0;

        for (let i = 0; i < data.length; i += 4) {
          const a = data[i + 3];
          if (a < 180) continue;
          const r = data[i];
          const g = data[i + 1];
          const b = data[i + 2];

          totalR += r;
          totalG += g;
          totalB += b;
          totalCount += 1;

          const [h, s, l] = rgbToHsl(r, g, b);
          if (l < 0.08 || l > 0.93) continue;

          // Strongly reward saturated mid-luminance pixels (like album accent tones)
          const lumScore = 1 - Math.abs(l - 0.5) * 1.45;
          const weight = Math.pow(s, 1.45) * Math.max(0.12, lumScore);
          if (weight < 0.02) continue;

          const idx = Math.floor((h % 360) / 15) % 24;
          const bucket = buckets[idx];
          bucket.weight += weight;
          bucket.rSum += r * weight;
          bucket.gSum += g * weight;
          bucket.bSum += b * weight;
        }

        let best = buckets[0];
        for (const b of buckets) {
          if (b.weight > best.weight) best = b;
        }

        let palette: AccentPalette;
        if (best.weight > 0.25) {
          const r = best.rSum / best.weight;
          const g = best.gSum / best.weight;
          const b = best.bSum / best.weight;
          const [h, s, l] = rgbToHsl(r, g, b);
          palette = buildPaletteFromHsl(h, s, l);
        } else if (totalCount > 0) {
          const [h, s, l] = rgbToHsl(
            totalR / totalCount,
            totalG / totalCount,
            totalB / totalCount,
          );
          palette = buildPaletteFromHsl(h, s, l);
        } else {
          palette = fallbackPaletteForSeed(seed ?? imageUrl);
        }

        colorCache.set(imageUrl, palette);
        inflight.delete(imageUrl);
        resolve(palette);
      } catch {
        finishFallback();
      }
    };

    img.src = imageUrl;
  });

  inflight.set(imageUrl, promise);
  return promise;
}

/**
 * Extracts a vivid, Spotify-grade dynamic accent palette from a track/playlist/avatar image
 * (or directly from Spotify's RGB integer color when provided).
 */
export function useCoverAccent(
  imageUrl?: string | null,
  seed?: string | null,
  spotifyColorInt?: number | null,
): AccentPalette {
  const initial = (): AccentPalette => {
    if (imageUrl && colorCache.has(imageUrl)) {
      return colorCache.get(imageUrl)!;
    }
    if (spotifyColorInt != null && spotifyColorInt > 0) {
      return paletteFromRgbInt(spotifyColorInt);
    }
    return fallbackPaletteForSeed(seed ?? imageUrl);
  };

  const [palette, setPalette] = useState<AccentPalette>(initial);

  useEffect(() => {
    if (imageUrl) {
      const cached = colorCache.get(imageUrl);
      if (cached) {
        setPalette(cached);
        return;
      }
      if (spotifyColorInt != null && spotifyColorInt > 0) {
        setPalette(paletteFromRgbInt(spotifyColorInt));
      } else {
        setPalette(fallbackPaletteForSeed(seed ?? imageUrl));
      }

      let active = true;
      void extractPaletteFromImage(imageUrl, seed).then((extracted) => {
        if (active) setPalette(extracted);
      });
      return () => {
        active = false;
      };
    }

    if (spotifyColorInt != null && spotifyColorInt > 0) {
      setPalette(paletteFromRgbInt(spotifyColorInt));
      return;
    }

    setPalette(fallbackPaletteForSeed(seed));
  }, [imageUrl, seed, spotifyColorInt]);

  return palette;
}
