import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';
import { Button, Card, EmptyState, Field, Input, Spinner, Textarea } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useCheckinInfoAdmin } from '../../lib/queries';
import type { CheckinInfoItem } from '../../lib/types';

const onError = (err: unknown) =>
  toast.error(err instanceof ApiError ? err.message : 'That action failed.');

export default function CheckinInfoTab() {
  const { data, isLoading } = useCheckinInfoAdmin();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState({ title: '', body: '' });

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['admin-checkin-info'] });
    void queryClient.invalidateQueries({ queryKey: ['checkin-info'] });
  };

  const create = useMutation({
    mutationFn: (body: { title: string; body: string }) =>
      api<CheckinInfoItem>('/admin/checkin-info', {
        method: 'POST',
        body: { ...body, sort_order: items.length },
      }),
    onSuccess: () => {
      setDraft({ title: '', body: '' });
      invalidate();
    },
    onError,
  });

  const update = useMutation({
    mutationFn: (item: CheckinInfoItem) =>
      api<CheckinInfoItem>(`/admin/checkin-info/${item.id}`, {
        method: 'PUT',
        body: { title: item.title, body: item.body, sort_order: item.sort_order },
      }),
    onSuccess: invalidate,
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api(`/admin/checkin-info/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Item removed.');
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
    <div className="space-y-4">
      <p className="text-sm text-muted">
        Shown to guests on <code className="rounded bg-cream-dark px-1.5 py-0.5">/my-stay</code> for the
        length of their stay, and baked into the booking-confirmation email at send time.
      </p>

      {items.length === 0 ? (
        <Card>
          <EmptyState title="No check-in info yet" hint="Add the first one below." />
        </Card>
      ) : (
        <div className="space-y-3">
          {items.map((item, i) => (
            <Card key={item.id}>
              <div className="flex items-start gap-3">
                <div className="flex flex-col pt-2">
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
                <div className="flex-1 space-y-2">
                  <Input
                    defaultValue={item.title}
                    className="font-semibold"
                    onBlur={(e) => {
                      const title = e.target.value.trim();
                      if (title && title !== item.title) update.mutate({ ...item, title });
                    }}
                  />
                  <Textarea
                    defaultValue={item.body}
                    rows={3}
                    onBlur={(e) => {
                      const body = e.target.value.trim();
                      if (body && body !== item.body) update.mutate({ ...item, body });
                    }}
                  />
                </div>
                <button
                  onClick={() => remove.mutate(item.id)}
                  title="Delete"
                  className="shrink-0 rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </Card>
          ))}
        </div>
      )}

      <Card>
        <form
          className="space-y-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (draft.title.trim() && draft.body.trim()) create.mutate(draft);
          }}
        >
          <Field label="Title">
            <Input
              value={draft.title}
              onChange={(e) => setDraft((d) => ({ ...d, title: e.target.value }))}
              placeholder="e.g. Key Location"
            />
          </Field>
          <Field label="Body">
            <Textarea
              rows={3}
              value={draft.body}
              onChange={(e) => setDraft((d) => ({ ...d, body: e.target.value }))}
              placeholder="Instructions guests will actually read standing in the driveway…"
            />
          </Field>
          <div className="flex justify-end">
            <Button
              type="submit"
              size="sm"
              disabled={create.isPending || !draft.title.trim() || !draft.body.trim()}
            >
              <Plus size={14} /> Add Item
            </Button>
          </div>
        </form>
      </Card>
    </div>
  );
}
