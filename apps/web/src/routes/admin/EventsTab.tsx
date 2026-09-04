import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { PartyPopper, Pencil, Plus, Trash2 } from 'lucide-react';
import {
  Button,
  Card,
  EmptyState,
  Field,
  Input,
  Modal,
  Spinner,
  Textarea,
  cx,
} from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useEvents } from '../../lib/queries';
import { formatRange, parseDay, toKey } from '../../lib/dates';
import type { SpecialEvent } from '../../lib/types';

/** A small palette beats a full emoji keyboard for the handful in play here. */
const EMOJI = ['🎉', '🎣', '🦐', '🦀', '🐟', '🏆', '🎆', '🦆', '🎄', '🥳', '⛵', '🌊'];

interface FormState {
  name: string;
  event_date: string;
  end_date: string;
  description: string;
  emoji: string;
}

const blank = (): FormState => ({
  name: '',
  event_date: toKey(new Date()),
  end_date: '',
  description: '',
  emoji: '🎉',
});

export default function EventsTab() {
  const { data, isLoading } = useEvents();
  const queryClient = useQueryClient();

  const [editing, setEditing] = useState<SpecialEvent | null>(null);
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState<FormState>(blank());

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['events'] });
  const onError = (err: unknown) =>
    toast.error(err instanceof ApiError ? err.message : 'That action failed.');

  const openNew = () => {
    setEditing(null);
    setForm(blank());
    setOpen(true);
  };

  const openEdit = (ev: SpecialEvent) => {
    setEditing(ev);
    setForm({
      name: ev.name,
      event_date: ev.event_date,
      end_date: ev.end_date ?? '',
      description: ev.description ?? '',
      emoji: ev.emoji ?? '🎉',
    });
    setOpen(true);
  };

  const save = useMutation({
    mutationFn: () => {
      const body = {
        name: form.name,
        event_date: form.event_date,
        end_date: form.end_date || null,
        description: form.description.trim() || null,
        emoji: form.emoji,
      };
      return editing
        ? api<SpecialEvent>(`/events/${editing.id}`, { method: 'PUT', body })
        : api<SpecialEvent>('/events', { method: 'POST', body });
    },
    onSuccess: () => {
      toast.success(editing ? 'Event updated.' : 'Event added.');
      setOpen(false);
      invalidate();
    },
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api(`/events/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Event removed.');
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

  const events = data ?? [];

  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button onClick={openNew}>
          <Plus size={16} /> Add Event
        </Button>
      </div>

      {events.length === 0 ? (
        <EmptyState
          icon={<PartyPopper size={24} />}
          title="No special events"
          hint="Rodeos, tournaments, holidays — anything worth planning around."
        />
      ) : (
        <div className="space-y-2">
          {events.map((ev) => (
            <Card key={ev.id} className="flex items-start justify-between gap-3 py-3">
              <div className="flex items-start gap-3">
                <span className="text-2xl leading-none" aria-hidden>
                  {ev.emoji ?? '🎉'}
                </span>
                <div>
                  <p className="font-semibold text-charcoal">{ev.name}</p>
                  <p className="text-sm text-muted">
                    {ev.end_date && ev.end_date !== ev.event_date
                      ? formatRange(ev.event_date, ev.end_date)
                      : format(parseDay(ev.event_date), 'MMM d, yyyy')}
                  </p>
                  {ev.description && <p className="mt-1 text-sm text-muted">{ev.description}</p>}
                </div>
              </div>
              <div className="flex shrink-0 gap-1">
                <button
                  onClick={() => openEdit(ev)}
                  title="Edit"
                  className="rounded p-2 text-muted hover:bg-cream-dark hover:text-charcoal"
                >
                  <Pencil size={15} />
                </button>
                <button
                  onClick={() => remove.mutate(ev.id)}
                  title="Delete"
                  className="rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
                >
                  <Trash2 size={15} />
                </button>
              </div>
            </Card>
          ))}
        </div>
      )}

      <Modal open={open} onClose={() => setOpen(false)} title={editing ? 'Edit event' : 'Add an event'}>
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate();
          }}
        >
          <Field label="Name">
            <Input
              required
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="Shrimp season opener"
            />
          </Field>

          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Date">
              <Input
                type="date"
                required
                value={form.event_date}
                onChange={(e) => setForm((f) => ({ ...f, event_date: e.target.value }))}
              />
            </Field>
            <Field label="End date" hint="Leave blank for a single day.">
              <Input
                type="date"
                min={form.event_date}
                value={form.end_date}
                onChange={(e) => setForm((f) => ({ ...f, end_date: e.target.value }))}
              />
            </Field>
          </div>

          <Field label="Emoji">
            <div className="flex flex-wrap gap-1.5">
              {EMOJI.map((e) => (
                <button
                  key={e}
                  type="button"
                  onClick={() => setForm((f) => ({ ...f, emoji: e }))}
                  className={cx(
                    'grid h-9 w-9 place-items-center rounded-lg border text-lg transition',
                    form.emoji === e
                      ? 'border-forest-600 bg-forest-50'
                      : 'border-sand bg-white hover:bg-cream-dark',
                  )}
                >
                  {e}
                </button>
              ))}
            </div>
          </Field>

          <Field label="Description">
            <Textarea
              rows={3}
              value={form.description}
              onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
              placeholder="Optional"
            />
          </Field>

          <div className="flex justify-end gap-2">
            <Button variant="ghost" type="button" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={save.isPending}>
              {save.isPending ? 'Saving…' : editing ? 'Save changes' : 'Add event'}
            </Button>
          </div>
        </form>
      </Modal>
    </>
  );
}
