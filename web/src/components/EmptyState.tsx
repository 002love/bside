import { Terminal } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';

interface EmptyStateProps {
  title: string;
  description: string;
  command?: string;
}

export function EmptyState({
  title,
  description,
  command = 'bside watch',
}: EmptyStateProps) {
  return (
    <Card className="border-dashed border-zinc-800/90 bg-zinc-900/35">
      <CardContent className="flex flex-col items-center justify-center py-14 text-center">
        <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl border border-zinc-800 bg-zinc-900 text-emerald-400">
          <Terminal className="h-5 w-5" />
        </div>
        <h3 className="text-sm font-medium text-zinc-100">{title}</h3>
        <p className="mt-1 max-w-md text-xs leading-relaxed text-zinc-400">{description}</p>
        {command && (
          <div className="mt-4 inline-flex items-center gap-2 rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-emerald-400">
            <span className="text-zinc-500">$</span>
            <span>{command}</span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
