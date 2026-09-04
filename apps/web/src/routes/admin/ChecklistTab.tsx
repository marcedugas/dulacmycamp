import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ArrowDown, ArrowUp, EyeOff, Plus, Trash2 } from 'lucide-react';
import { Button, Card, EmptyState, Input, Spinner, cx } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useChecklistAdmin } from '../../lib/queries';
import type { ChecklistItem } from '../../lib/types';

const onError = (err: unknown) =>
  toast.error(err instanceof ApiError ? err.message : 'That action failed.');

export default function ChecklistTab() {
  const { data, isLoading } = useChecklistAdmin();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState('');

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['admin-checklist'] });
    void queryClient.invalidateQueries({ queryKey: ['checklist'] });
  };

  const create = useMutation({
    mutationFn: (label: string) =>
      api<ChecklistItem>('/admin/checklist', {
        method: 'POST',
        body: { label, sort_order: items.length },
      }),
    onSuccess: () => {
      setDraft('');
      invalidate();
    },
    onError,
  });

  const update = useMutation({
    mutationFn: (item: ChecklistItem) =>
      api<ChecklistItem>(`/admin/checklist/${item.id}`, {
        method: 'PUT',
        body: { label: item.label, sort_order: item.sort_order, active: item.active },
      }),
    onSuccess: invalidate,
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api<{ deleted: boolean; deactivated: boolean; message?: string }>(
      `/admin/checklist/${id}`,
      { method: 'DELETE' },
    ),
    onSuccess: (res) => {
      toast.success(res.deleted ? 'Item deleted.' : (res.message ?? 'Item deactivated.'));
      invalidate();
    },
    onError,
  });

  const items = data ?? [];

  const move = (index: number, direction: -1 | 1) => {
    const other = items[index + direction];
    const mover = items[index];
    if (!other) return;
    update.mutate({ ...mover, sort_order: other.sort_order });
    update.mutate({ ...other, sort_order: mover.sort_order });
  };

  if (isLoading) {
    return (
      <div className="flex justify-center py-20">
        <Spinner />
      </div>
    );
  }

  return (
    <Card>
      <p className="mb-4 text-sm text-muted">
        This is the checklist guests see at{' '}
        <code className="rounded bg-cream-dark px-1.5 py-0.5">/checkout</code>. Deactivated items stay
        correct on past checkout records — they're never deleted out from under history.
      </p>

      {items.length === 0 ? (
        <EmptyState title="No checklist items yet" hint="Add the first one below." />
      ) : (
        <ul className="mb-4 space-y-2">
          {items.map((item, i) => (
            <li
              key={item.id}
              className={cx(
                'flex items-center gap-2 rounded-lg border px-2 py-1.5',
                item.active ? 'border-sand' : 'border-sand/60 bg-cream-dark/40',
              )}
            >
              <div className="flex flex-col">
                <button
                  disabled={i === 0}
                  onClick={() => move(i, -1)}
                  className="text-muted hover:text-charcoal disabled:opacity-25"
                  aria-label="Move up"
                >
                  <ArrowUp size={14} />
                </button>
                <button
                  disabled={i === items.length - 1}
                  onClick={() => move(i, 1)}
                  className="text-muted hover:text-charcoal disabled:opacity-25"
                  aria-label="Move down"
                >
                  <ArrowDown size={14} />
                </button>
              </div>
              <Input
                defaultValue={item.label}
                className={cx('flex-1', !item.active && 'text-muted line-through')}
                onBlur={(e) => {
                  const label = e.target.value.trim();
                  if (label && label !== item.label) update.mutate({ ...item, label });
                }}
              />
              {!item.active && (
                <span className="shrink-0 rounded-full border border-sand bg-cream-dark px-2 py-0.5 text-xs font-semibold text-muted">
                  Inactive
                </span>
              )}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => update.mutate({ ...item, active: !item.active })}
                title={item.active ? 'Deactivate' : 'Reactivate'}
              >
                <EyeOff size={14} /> {item.active ? 'Deactivate' : 'Reactivate'}
              </Button>
              <button
                onClick={() => remove.mutate(item.id)}
                title="Delete"
                className="shrink-0 rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
              >
                <Trash2 size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}

      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (draft.trim()) create.mutate(draft.trim());
        }}
      >
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Add a checklist item…"
        />
        <Button type="submit" size="sm" disabled={create.isPending || !draft.trim()}>
          <Plus size={14} /> Add Item
        </Button>
      </form>
    </Card>
  );
}
