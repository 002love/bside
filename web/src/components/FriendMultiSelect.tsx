import { useEffect, useMemo, useRef, useState } from 'react';
import { Check, ChevronDown, Search, Users, X } from 'lucide-react';
import { UserAvatar } from '@/components/UserAvatar';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

export interface FriendOption {
  user_uri: string;
  user_name: string;
  image_url: string | null;
  play_count?: number;
  is_self?: boolean;
  is_live?: boolean;
}

export interface FriendMultiSelectProps {
  friends: FriendOption[];
  selectedUsers: string[]; // empty = all friends
  onChange: (next: string[]) => void;
  className?: string;
}

export function FriendMultiSelect({
  friends,
  selectedUsers,
  onChange,
  className,
}: FriendMultiSelectProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false);
      }
    };

    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  const selectedSet = useMemo(() => new Set(selectedUsers), [selectedUsers]);

  const selfOption = useMemo(() => friends.find((f) => f.is_self) ?? null, [friends]);

  const selectedFriends = useMemo(
    () => friends.filter((f) => selectedSet.has(f.user_uri)),
    [friends, selectedSet],
  );

  const filteredFriends = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return friends;
    return friends.filter((f) => f.user_name.toLowerCase().includes(q));
  }, [friends, query]);

  const toggleUser = (uri: string) => {
    if (selectedSet.has(uri)) {
      onChange(selectedUsers.filter((u) => u !== uri));
    } else {
      onChange([...selectedUsers, uri]);
    }
  };

  const selectAll = () => {
    onChange([]);
  };

  const selectOnlyMe = () => {
    if (selfOption) {
      onChange([selfOption.user_uri]);
    }
  };

  const isOnlyMeActive =
    selfOption != null &&
    selectedUsers.length === 1 &&
    selectedUsers[0] === selfOption.user_uri;

  return (
    <div ref={containerRef} className={cn('relative inline-block', className)}>
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        aria-haspopup="listbox"
        className={cn(
          'inline-flex h-8 items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] px-2.5 text-xs font-medium text-[var(--ink-primary)] shadow-edge transition-all hover:border-[var(--border-strong)] hover:bg-[var(--bg-hover)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]/70 cursor-pointer',
          open && 'border-[var(--border-accent)] bg-[var(--bg-hover)]',
        )}
      >
        {selectedFriends.length === 0 ? (
          <>
            {friends.length > 0 ? (
              <span className="flex items-center -space-x-1.5">
                {friends.slice(0, 3).map((friend) => (
                  <UserAvatar
                    key={friend.user_uri}
                    name={friend.user_name}
                    imageUrl={friend.image_url}
                    size="xs"
                    className="ring-1 ring-[var(--bg-panel)]"
                  />
                ))}
              </span>
            ) : (
              <Users className="h-3.5 w-3.5 text-[var(--ink-muted)]" />
            )}
            <span className="whitespace-nowrap">All Listeners</span>
            {friends.length > 0 && (
              <span className="rounded-[var(--radius-pill)] bg-[var(--bg-inset)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--ink-muted)]">
                {friends.length}
              </span>
            )}
          </>
        ) : selectedFriends.length === 1 ? (
          <>
            <UserAvatar
              name={selectedFriends[0].user_name}
              imageUrl={selectedFriends[0].image_url}
              size="xs"
              isLive={selectedFriends[0].is_live}
              isSelf={selectedFriends[0].is_self}
            />
            <span className="max-w-[120px] truncate">{selectedFriends[0].user_name}</span>
          </>
        ) : (
          <>
            <span className="flex items-center -space-x-1.5">
              {selectedFriends.slice(0, 3).map((friend) => (
                <UserAvatar
                  key={friend.user_uri}
                  name={friend.user_name}
                  imageUrl={friend.image_url}
                  size="xs"
                  className="ring-1 ring-[var(--bg-panel)]"
                />
              ))}
            </span>
            <span className="whitespace-nowrap text-[var(--accent-primary)]">
              {selectedFriends.length} selected
            </span>
          </>
        )}
        <ChevronDown
          className={cn(
            'h-3.5 w-3.5 text-[var(--ink-muted)] transition-transform duration-150',
            open && 'rotate-180',
          )}
        />
      </button>

      {open && (
        <div
          role="listbox"
          aria-multiselectable="true"
          className="bside-popover absolute right-0 z-50 mt-2 w-72 overflow-hidden p-2"
        >
          {/* Quick actions header */}
          <div className="flex items-center justify-between gap-1.5 pb-2">
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={selectAll}
                className={cn(
                  'rounded-[var(--radius-pill)] border px-2.5 py-1 font-mono text-[10px] font-semibold uppercase tracking-wider transition-colors cursor-pointer',
                  selectedUsers.length === 0
                    ? 'border-[var(--accent-primary)] bg-[var(--accent-soft)] text-[var(--accent-primary)]'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-inset)] text-[var(--ink-muted)] hover:text-[var(--ink-primary)]',
                )}
              >
                All
              </button>
              {selfOption && (
                <button
                  type="button"
                  onClick={selectOnlyMe}
                  className={cn(
                    'rounded-[var(--radius-pill)] border px-2.5 py-1 font-mono text-[10px] font-semibold uppercase tracking-wider transition-colors cursor-pointer',
                    isOnlyMeActive
                      ? 'border-[var(--accent-primary)] bg-[var(--accent-soft)] text-[var(--accent-primary)]'
                      : 'border-[var(--border-subtle)] bg-[var(--bg-inset)] text-[var(--ink-muted)] hover:text-[var(--ink-primary)]',
                  )}
                >
                  Only Me
                </button>
              )}
            </div>
            {selectedUsers.length > 0 && (
              <button
                type="button"
                onClick={selectAll}
                className="inline-flex items-center gap-1 text-[10px] text-[var(--ink-muted)] hover:text-[var(--ink-primary)] cursor-pointer"
                title="Clear selection"
              >
                <X className="h-3 w-3" />
                Reset
              </button>
            )}
          </div>

          {/* Search input when > 6 friends */}
          {friends.length > 6 && (
            <div className="relative mb-2">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter listeners..."
                className="h-7 w-full rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-inset)] pr-2.5 pl-8 text-xs text-[var(--ink-primary)] placeholder:text-[var(--ink-muted)] focus:border-[var(--accent-primary)] focus:outline-none"
              />
            </div>
          )}

          {/* Checkable list */}
          <div className="max-h-64 space-y-0.5 overflow-y-auto pr-0.5">
            {filteredFriends.length === 0 ? (
              <div className="py-5 text-center text-xs text-[var(--ink-muted)]">
                No listeners found
              </div>
            ) : (
              filteredFriends.map((friend) => {
                const isSelected = selectedSet.has(friend.user_uri);
                return (
                  <button
                    key={friend.user_uri}
                    type="button"
                    role="option"
                    aria-selected={isSelected}
                    onClick={() => toggleUser(friend.user_uri)}
                    className={cn(
                      'flex w-full items-center gap-2.5 rounded-[calc(var(--radius-control)*0.75)] px-2 py-1.5 text-left transition-colors cursor-pointer',
                      isSelected
                        ? 'bg-[var(--accent-soft)] text-[var(--ink-primary)]'
                        : 'text-[var(--ink-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
                    )}
                  >
                    <span
                      className={cn(
                        'flex h-4 w-4 shrink-0 items-center justify-center rounded-[calc(var(--radius-control)*0.5)] border transition-colors',
                        isSelected
                          ? 'border-[var(--accent-primary)] bg-[var(--accent-primary)] text-[var(--ink-on-accent)]'
                          : 'border-[var(--border-strong)] bg-[var(--bg-inset)]',
                      )}
                    >
                      {isSelected && <Check className="h-3 w-3 stroke-[2.75]" />}
                    </span>

                    <UserAvatar
                      name={friend.user_name}
                      imageUrl={friend.image_url}
                      size="sm"
                      isLive={friend.is_live}
                      isSelf={friend.is_self}
                    />

                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate text-xs font-medium text-[var(--ink-primary)]">
                          {friend.user_name}
                        </span>
                        {friend.is_self && (
                          <Badge variant="default" className="px-1.5 py-0 text-[9px]">
                            YOU
                          </Badge>
                        )}
                      </div>
                    </div>

                    {typeof friend.play_count === 'number' && (
                      <span className="shrink-0 font-mono text-[10px] text-[var(--ink-muted)]">
                        {friend.play_count.toLocaleString()}
                      </span>
                    )}
                  </button>
                );
              })
            )}
          </div>

          {/* Footer hint */}
          <div className="mt-2 border-t border-[var(--border-subtle)] pt-2 text-[10px] leading-snug text-[var(--ink-muted)]">
            View filter only — background collection still tracks everyone.
          </div>
        </div>
      )}
    </div>
  );
}
