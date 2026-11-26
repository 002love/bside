import type { ReactNode } from 'react';
import { motion } from 'motion/react';
import { cn } from '@/lib/utils';

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
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -4 }}
      transition={{ duration: 0.2, ease: EASE_OUT_EXPO }}
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
  const delay = Math.min(index * 0.025, 0.18);
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.22, delay, ease: EASE_OUT_EXPO }}
      className={className}
    >
      {children}
    </motion.div>
  );
}

export interface AmbientArtGlowProps {
  imageUrl?: string | null;
  className?: string;
  intensity?: 'subtle' | 'medium';
}

/**
 * Renders a deeply blurred, low-saturation ambient tint sourced from real album or
 * playlist artwork, masked smoothly into the surrounding canvas.
 */
export function AmbientArtGlow({
  imageUrl,
  className,
  intensity = 'subtle',
}: AmbientArtGlowProps) {
  if (!imageUrl) return null;

  return (
    <div
      aria-hidden="true"
      className={cn(
        'pointer-events-none absolute inset-0 overflow-hidden rounded-[inherit]',
        className,
      )}
    >
      <img
        src={imageUrl}
        alt=""
        loading="lazy"
        className={cn(
          'h-full w-full scale-125 object-cover blur-3xl saturate-[1.15] transition-opacity duration-700',
          intensity === 'medium' ? 'opacity-[0.22]' : 'opacity-[0.14]',
        )}
      />
      <div className="absolute inset-0 bg-gradient-to-b from-[#0f0f12]/35 via-[#0f0f12]/75 to-[#0f0f12]" />
    </div>
  );
}
