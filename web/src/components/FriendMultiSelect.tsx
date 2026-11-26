import { useCallback, useMemo, useRef, useState } from 'react';
import { Check, ChevronDown, Search, Users, X } from 'lucide-react';
import { UserAvatar } from '@/components/UserAvatar';
import { PopoverSurface } from '@/components/ui/popover';
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
  selectedUsers: string[];
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
  const closePopover = useCallback(() => setOpen(false), []);

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

  const selectAll = () => onChange([]);
  const selectOnlyMe = () => {
    if (selfOption) onChange([selfOption.user_uri]);
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
          'tactile-press inline-flex h-8 items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2.5 text-xs font-medium text-[var(--ink-primary)] transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--bg-elevated)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--border-focus)] cursor-pointer',
          open && 'border-[var(--border-strong)] bg-[var(--bg-elevated)]',
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
                    className="ring-1 ring-[var(--bg-surface)]"
                  />
                ))}
              </span>
            ) : (
              <Users className="h-3.5 w-3.5 text-[var(--ink-muted)]" />
            )}
            <span className="whitespace-nowrap">Everyone</span>
            {friends.length > 0 && (
              <span className="tabular-nums text-[11px] text-[var(--ink-muted)]">
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
                  className="ring-1 ring-[var(--bg-surface)]"
                />
              ))}
            </span>
            <span className="whitespace-nowrap text-[var(--accent-ember)]">
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

      <PopoverSurface
        open={open}
        onClose={closePopover}
        anchorRef={containerRef}
        preferredWidth={280}
        role="listbox"
        aria-multiselectable="true"
        className="flex flex-col overflow-hidden p-2"
      >
        <div className="flex shrink-0 items-center justify-between gap-1.5 pb-2">
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={selectAll}
              className={cn(
                'rounded-[6px] px-2.5 py-1 text-[11px] font-medium transition-colors cursor-pointer',
                selectedUsers.length === 0
                  ? 'bg-[var(--bg-active)] text-[var(--ink-primary)]'
                  : 'text-[var(--ink-muted)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
              )}
            >
              Everyone
            </button>
            {selfOption && (
              <button
                type="button"
                onClick={selectOnlyMe}
                className={cn(
                  'rounded-[6px] px-2.5 py-1 text-[11px] font-medium transition-colors cursor-pointer',
                  isOnlyMeActive
                    ? 'bg-[var(--bg-active)] text-[var(--ink-primary)]'
                    : 'text-[var(--ink-muted)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
                )}
              >
                Only me
              </button>
            )}
          </div>
          {selectedUsers.length > 0 && (
            <button
              type="button"
              onClick={selectAll}
              className="inline-flex items-center gap-1 text-[11px] text-[var(--ink-muted)] hover:text-[var(--ink-primary)] cursor-pointer"
              title="Clear selection"
            >
              <X className="h-3 w-3" />
              Clear
            </button>
          )}
        </div>

        {friends.length > 6 && (
          <div className="relative mb-2 shrink-0">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-muted)]" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search listeners..."
              className="h-7 w-full rounded-[6px] border border-[var(--border-subtle)] bg-[var(--bg-inset)] pr-2.5 pl-8 text-xs text-[var(--ink-primary)] placeholder:text-[var(--ink-muted)] focus:border-[var(--border-focus)] focus:outline-none"
            />
          </div>
        )}

        <div className="min-h-0 max-h-64 flex-1 space-y-0.5 overflow-y-auto pr-0.5">
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
                    'flex w-full items-center gap-2.5 rounded-[7px] px-2 py-1.5 text-left transition-colors cursor-pointer',
                    isSelected
                      ? 'bg-[var(--bg-active)] text-[var(--ink-primary)]'
                      : 'text-[var(--ink-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--ink-primary)]',
                  )}
                >
                  <span
                    className={cn(
                      'flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-[4px] border transition-colors',
                      isSelected
                        ? 'border-[var(--accent-cta)] bg-[var(--accent-cta)] text-[var(--ink-on-accent)]'
                        : 'border-[var(--border-strong)] bg-transparent',
                    )}
                  >
                    {isSelected && <Check className="h-2.5 w-2.5 stroke-[3]" />}
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
                        <span className="text-[10px] text-[var(--ink-muted)]">You</span>
                      )}
                    </div>
                  </div>

                  {typeof friend.play_count === 'number' && (
                    <span className="shrink-0 tabular-nums text-[11px] text-[var(--ink-muted)]">
                      {friend.play_count.toLocaleString()}
                    </span>
                  )}
                </button>
              );
            })
          )}
        </div>

        <div className="mt-2 shrink-0 border-t border-[var(--border-subtle)] pt-2 text-[11px] leading-snug text-[var(--ink-muted)]">
          Filters current views only. Background tracking stays active.
        </div>
      </PopoverSurface>
    </div>
  );
}
