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
    <Card className="border-dashed border-[#45453e] bg-[#141412]">
      <CardContent className="flex flex-col items-center justify-center py-14 text-center">
        <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-none border border-[#45453e] bg-[#181816] text-[#89b471] shadow-edge">
          <Terminal className="h-4 w-4" />
        </div>
        <h3 className="font-display text-xl font-bold uppercase tracking-tight text-[#efeee8]">
          {title}
        </h3>
        <p className="mt-1.5 max-w-md font-mono text-[11px] leading-relaxed text-[#8d8c84]">
          {description}
        </p>
        {command && (
          <div className="mt-4 inline-flex items-center gap-2 rounded-none border border-[#45453e] bg-[#111110] px-3 py-1.5 font-mono text-xs text-[#89b471] shadow-moss">
            <span className="text-[#8d8c84]">$</span>
            <span>{command}</span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
