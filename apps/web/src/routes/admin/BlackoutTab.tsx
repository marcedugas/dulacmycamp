import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Lock, Plus, Trash2 } from 'lucide-react';
import { Button, Card, EmptyState, Field, Input, Modal, Spinner } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useBlackouts } from '../../lib/queries';
import { formatRange, toKey } from '../../lib/dates';
import type { BlackoutDate } from '../../lib/types';

export default function BlackoutTab() {
  const { data, isLoading } = useBlackouts();
  const queryClient = useQueryClient();

  const [open, setOpen] = useState(false);
  const today = toKey(new Date());
  const [form, setForm] = useState({ start_date: today, end_date: today, reason: '' });

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['blackout-dates'] });
  const onError = (err: unknown) =>
    toast.error(err instanceof ApiError ? err.message : 'That action failed.');

  const create = useMutation({
    mutationFn: () =>
      api<BlackoutDate>('/blackout-dates', {
        method: 'POST',
        body: { ...form, reason: form.reason.trim() || null },
      }),
    onSuccess: () => {
      toast.success('Blackout added.');
      setOpen(false);
      setForm({ start_date: today, end_date: today, reason: '' });
      invalidate();
    },
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api(`/blackout-dates/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Blackout removed.');
      invalidate();
    },
    onError,
  });

  if (isLoading) {
    return (
      <div className="flex justify-center py-20">
        <Spinner />
      </div>
    );
  }

  const blackouts = data ?? [];

  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button onClick={() => setOpen(true)}>
          <Plus size={16} /> Add Blackout
        </Button>
      </div>

      {blackouts.length === 0 ? (
        <EmptyState
          icon={<Lock size={24} />}
          title="No blackout dates"
          hint="Block off maintenance weeks or family holidays here."
        />
      ) : (
        <div className="space-y-2">
          {blackouts.map((b) => (
            <Card key={b.id} className="flex items-center justify-between gap-3 py-3">
              <div>
                <p className="font-semibold text-charcoal">
                  {formatRange(b.start_date, b.end_date)}
                </p>
                <p className="text-sm text-muted">{b.reason ?? 'No reason given'}</p>
              </div>
              <button
                onClick={() => remove.mutate(b.id)}
                title="Delete"
                className="rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
              >
                <Trash2 size={16} />
              </button>
            </Card>
          ))}
        </div>
      )}

      <Modal open={open} onClose={() => setOpen(false)} title="Add a blackout">
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate();
          }}
        >
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Start">
              <Input
                type="date"
                required
                value={form.start_date}
                onChange={(e) => setForm((f) => ({ ...f, start_date: e.target.value }))}
              />
            </Field>
            <Field label="End" hint="Inclusive — the camp is closed this day too.">
              <Input
                type="date"
                required
                min={form.start_date}
                value={form.end_date}
                onChange={(e) => setForm((f) => ({ ...f, end_date: e.target.value }))}
              />
            </Field>
          </div>
          <Field label="Reason">
            <Input
              value={form.reason}
              onChange={(e) => setForm((f) => ({ ...f, reason: e.target.value }))}
              placeholder="Roof repair, family reunion…"
            />
          </Field>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" type="button" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={create.isPending}>
              {create.isPending ? 'Saving…' : 'Add blackout'}
            </Button>
          </div>
        </form>
      </Modal>
    </>
  );
}
