import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ArrowDown, ArrowUp, EyeOff, Plus, Trash2 } from 'lucide-react';
import { Button, Card, EmptyState, Input, Spinner, cx } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useFishSpeciesAdmin } from '../../lib/queries';
import type { FishSpecies } from '../../lib/types';

const onError = (err: unknown) =>
  toast.error(err instanceof ApiError ? err.message : 'That action failed.');

/**
 * The species list behind the journal catch log — the same shape of feature
 * as the checkout checklist, so deliberately the same UI as
 * {@link ChecklistTab}: reorder, rename in place, deactivate, delete.
 */
export default function FishSpeciesTab() {
  const { data, isLoading } = useFishSpeciesAdmin();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState('');

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['admin-fish-species'] });
    void queryClient.invalidateQueries({ queryKey: ['fish-species'] });
    // A rename reads through to every catch that points at it.
    void queryClient.invalidateQueries({ queryKey: ['journal-feed'], exact: false });
    void queryClient.invalidateQueries({ queryKey: ['journal-admin'] });
    void queryClient.invalidateQueries({ queryKey: ['journal-mine'] });
  };

  const create = useMutation({
    mutationFn: (name: string) =>
      api<FishSpecies>('/admin/fish-species', {
        method: 'POST',
        body: { name, sort_order: species.length },
      }),
    onSuccess: () => {
      setDraft('');
      invalidate();
    },
    onError,
  });

  const update = useMutation({
    mutationFn: (item: FishSpecies) =>
      api<FishSpecies>(`/admin/fish-species/${item.id}`, {
        method: 'PUT',
        body: { name: item.name, sort_order: item.sort_order, active: item.active },
      }),
    onSuccess: invalidate,
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) =>
      api<{ deleted: boolean; deactivated: boolean; message?: string }>(
        `/admin/fish-species/${id}`,
        { method: 'DELETE' },
      ),
    onSuccess: (res) => {
      toast.success(res.deleted ? 'Species deleted.' : (res.message ?? 'Species deactivated.'));
      invalidate();
    },
    onError,
  });

  const species = data ?? [];

  const move = (index: number, direction: -1 | 1) => {
    const other = species[index + direction];
    const mover = species[index];
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
        These are the options in the catch log's species dropdown when someone writes a journal
        entry. Deactivated species stay correct on catches already logged against them — they're
        never deleted out from under someone's memory of a trip.
      </p>

      {species.length === 0 ? (
        <EmptyState title="No species yet" hint="Add the first one below." />
      ) : (
        <ul className="mb-4 space-y-2">
          {species.map((item, i) => (
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
                  disabled={i === species.length - 1}
                  onClick={() => move(i, 1)}
                  className="text-muted hover:text-charcoal disabled:opacity-25"
                  aria-label="Move down"
                >
                  <ArrowDown size={14} />
                </button>
              </div>
              <Input
                defaultValue={item.name}
                className={cx('flex-1', !item.active && 'text-muted line-through')}
                onBlur={(e) => {
                  const name = e.target.value.trim();
                  if (name && name !== item.name) update.mutate({ ...item, name });
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
                aria-label="Delete"
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
          placeholder="Add a species…"
        />
        <Button type="submit" size="sm" disabled={create.isPending || !draft.trim()}>
          <Plus size={14} /> Add Species
        </Button>
      </form>
    </Card>
  );
}
