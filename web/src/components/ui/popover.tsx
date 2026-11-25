import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type RefObject,
} from 'react';
import { createPortal } from 'react-dom';
import { cn } from '@/lib/utils';

export interface PopoverSurfaceProps {
  open: boolean;
  onClose: () => void;
  anchorRef: RefObject<HTMLElement | null>;
  preferredWidth?: number;
  sideOffset?: number;
  collisionPadding?: number;
  align?: 'start' | 'end';
  role?: string;
  'aria-label'?: string;
  'aria-multiselectable'?: boolean | 'true' | 'false';
  className?: string;
  children: ReactNode;
}

interface PopoverCoords {
  left: number;
  top?: number;
  bottom?: number;
  width: number;
  maxHeight: number;
}

export function PopoverSurface({
  open,
  onClose,
  anchorRef,
  preferredWidth = 288,
  sideOffset = 8,
  collisionPadding = 12,
  align = 'end',
  role,
  'aria-label': ariaLabel,
  'aria-multiselectable': ariaMultiselectable,
  className,
  children,
}: PopoverSurfaceProps) {
  const popoverRef = useRef<HTMLDivElement>(null);
  const [coords, setCoords] = useState<PopoverCoords | null>(null);

  const updatePosition = useCallback(() => {
    const anchor = anchorRef.current;
    if (!anchor) return;

    const rect = anchor.getBoundingClientRect();
    const vv = window.visualViewport;
    const viewportWidth = vv?.width ?? (document.documentElement.clientWidth || window.innerWidth);
    const viewportHeight = vv?.height ?? window.innerHeight;
    const viewportLeft = vv?.offsetLeft ?? 0;
    const viewportTop = vv?.offsetTop ?? 0;

    const maxAvailableWidth = Math.max(0, viewportWidth - collisionPadding * 2);
    const width = Math.min(preferredWidth, maxAvailableWidth);

    const preferredLeft = align === 'end' ? rect.right - width : rect.left;
    const minLeft = viewportLeft + collisionPadding;
    const maxLeft = Math.max(minLeft, viewportLeft + viewportWidth - width - collisionPadding);
    const left = Math.max(minLeft, Math.min(preferredLeft, maxLeft));

    const belowTop = rect.bottom + sideOffset;
    const spaceBelow = viewportTop + viewportHeight - belowTop - collisionPadding;
    const spaceAbove = rect.top - sideOffset - (viewportTop + collisionPadding);

    if (spaceBelow < 180 && spaceAbove > spaceBelow) {
      setCoords({
        left,
        bottom: window.innerHeight - rect.top + sideOffset,
        width,
        maxHeight: Math.max(120, Math.floor(spaceAbove)),
      });
    } else {
      setCoords({
        left,
        top: belowTop,
        width,
        maxHeight: Math.max(120, Math.floor(spaceBelow)),
      });
    }
  }, [anchorRef, preferredWidth, sideOffset, collisionPadding, align]);

  useLayoutEffect(() => {
    if (!open) {
      setCoords(null);
      return;
    }

    updatePosition();

    const handleReposition = () => updatePosition();
    window.addEventListener('resize', handleReposition);
    window.addEventListener('scroll', handleReposition, { capture: true, passive: true });
    window.visualViewport?.addEventListener('resize', handleReposition);
    window.visualViewport?.addEventListener('scroll', handleReposition);

    return () => {
      window.removeEventListener('resize', handleReposition);
      window.removeEventListener('scroll', handleReposition, true);
      window.visualViewport?.removeEventListener('resize', handleReposition);
      window.visualViewport?.removeEventListener('scroll', handleReposition);
    };
  }, [open, updatePosition]);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (anchorRef.current?.contains(target)) return;
      if (popoverRef.current?.contains(target)) return;
      onClose();
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open, onClose, anchorRef]);

  if (!open || typeof document === 'undefined') return null;

  const style: CSSProperties = coords
    ? {
        position: 'fixed',
        left: `${coords.left}px`,
        top: coords.top !== undefined ? `${coords.top}px` : undefined,
        bottom: coords.bottom !== undefined ? `${coords.bottom}px` : undefined,
        width: `${coords.width}px`,
        maxHeight: `${coords.maxHeight}px`,
      }
    : {
        position: 'fixed',
        visibility: 'hidden',
        left: 0,
        top: 0,
        width: `${preferredWidth}px`,
      };

  return createPortal(
    <div
      ref={popoverRef}
      role={role}
      aria-label={ariaLabel}
      aria-multiselectable={ariaMultiselectable}
      style={style}
      className={cn('bside-popover z-50', className)}
    >
      {children}
    </div>,
    document.body,
  );
}
