import type { ReactNode } from 'react';
import { Info } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

export interface InfoTooltipProps {
  content: ReactNode;
  children?: ReactNode;
  side?: 'top' | 'bottom' | 'left' | 'right';
  iconOnly?: boolean;
  className?: string;
}

export function InfoTooltip({
  content,
  children,
  side = 'top',
  iconOnly = false,
  className,
}: InfoTooltipProps) {
  const showIcon = iconOnly || !children;

  return (
    <Tooltip delayDuration={180}>
      <TooltipTrigger asChild>
        {showIcon ? (
          <button
            type="button"
            className={cn(
              'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[var(--ink-muted)] transition-colors hover:text-[var(--ink-primary)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent-primary)] cursor-help',
              className,
            )}
            aria-label="More information"
          >
            <Info className="h-3.5 w-3.5" />
          </button>
        ) : (
          <span
            className={cn(
              'inline-flex items-center cursor-help decoration-dotted underline-offset-2',
              className,
            )}
          >
            {children}
          </span>
        )}
      </TooltipTrigger>
      <TooltipContent side={side}>{content}</TooltipContent>
    </Tooltip>
  );
}
