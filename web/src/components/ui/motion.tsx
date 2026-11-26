import type { ReactNode } from 'react';
import { motion } from 'motion/react';
import { cn } from '@/lib/utils';
import type { AccentPalette } from '@/lib/extractColor';

export const SPRING_SNAPPY = {
  type: 'spring' as const,
  stiffness: 520,
  damping: 36,
  mass: 0.7,
};

export const SPRING_SMOOTH = {
  type: 'spring' as const,
  stiffness: 320,
  damping: 30,
  mass: 0.85,
};

export const EASE_OUT_EXPO = [0.16, 1, 0.3, 1] as const;

export interface PageTransitionProps {
  children: ReactNode;
  className?: string;
}

export function PageTransition({ children, className }: PageTransitionProps) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -4 }}
      transition={{ duration: 0.18, ease: EASE_OUT_EXPO }}
      className={className}
    >
      {children}
    </motion.div>
  );
}

export interface StaggerItemProps {
  children: ReactNode;
  index?: number;
  className?: string;
}

export function StaggerItem({ children, index = 0, className }: StaggerItemProps) {
  const delay = Math.min(index * 0.022, 0.16);
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2, delay, ease: EASE_OUT_EXPO }}
      className={className}
    >
      {children}
    </motion.div>
  );
}

export interface AmbientArtGlowProps {
  imageUrl?: string | null;
  /** Extracted dynamic accent palette for crisp, visible Spotify-grade gradient wash */
  palette?: AccentPalette;
  className?: string;
  intensity?: 'subtle' | 'medium' | 'vivid';
}

/**
 * Full-bleed Spotify/Apple-grade dynamic color wash that smoothly melts into #09090b.
 * Uses extracted palette colors directly so the accent hue is rich and unmistakable
 * instead of muddy gray blur.
 */
export function AmbientArtGlow({
  imageUrl,
  palette,
  className,
  intensity = 'medium',
}: AmbientArtGlowProps) {
  if (!imageUrl && !palette) return null;

  const topAlpha = intensity === 'vivid' ? 0.56 : intensity === 'medium' ? 0.42 : 0.26;
  const midAlpha = intensity === 'vivid' ? 0.22 : intensity === 'medium' ? 0.15 : 0.08;

  const gradientBg = palette
    ? `radial-gradient(ellipse 90% 100% at 15% 0%, rgba(${palette.rgb}, ${topAlpha}) 0%, rgba(${palette.rgb}, ${midAlpha}) 48%, rgba(9, 9, 11, 0) 85%), linear-gradient(180deg, ${palette.wash} 0%, ${palette.washMid} 42%, #09090b 100%)`
    : undefined;

  return (
    <div
      aria-hidden="true"
      className={cn(
        'pointer-events-none absolute inset-0 overflow-hidden transition-all duration-500',
        className,
      )}
    >
      {gradientBg ? (
        <div
          className="absolute inset-0 transition-[background] duration-500"
          style={{ background: gradientBg }}
        />
      ) : (
        imageUrl && (
          <>
            <img
              src={imageUrl}
              alt=""
              loading="lazy"
              className={cn(
                'h-full w-full scale-125 object-cover blur-3xl saturate-150 transition-opacity duration-500',
                intensity === 'vivid'
                  ? 'opacity-45'
                  : intensity === 'medium'
                    ? 'opacity-35'
                    : 'opacity-22',
              )}
            />
            <div className="absolute inset-0 bg-gradient-to-b from-black/20 via-[#09090b]/60 to-[#09090b]" />
          </>
        )
      )}
    </div>
  );
}
