import { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { BookOpen, Send, Tent } from 'lucide-react';
import { Button, Card, EmptyState, Field, Input, PageHeader, Spinner, Textarea } from '../components/ui';
import { api, ApiError } from '../lib/api';
import { useJournalEligibleBookings, useJournalMine } from '../lib/queries';
import { formatRange } from '../lib/dates';
import type { JournalEntry } from '../lib/types';

/**
 * Doubles as the "new story" form (?booking_id=) and the "edit my pending
 * story" form (?entry_id=, reached from My Bookings) — same fields, same
 * card, just POST vs PUT under the hood.
 */
export default function JournalNew() {
  const [params] = useSearchParams();
  const entryId = params.get('entry_id');
  const fromQuery = params.get('booking_id');

  const eligibleQuery = useJournalEligibleBookings();
  const mineQuery = useJournalMine();
  const queryClient = useQueryClient();

  const eligible = eligibleQuery.data ?? [];
  const editing = entryId ? (mineQuery.data ?? []).find((e) => e.id === entryId) : undefined;

  const [pickedId, setPickedId] = useState<string | null>(fromQuery);
  const [title, setTitle] = useState('');
  const [body, setBody] = useState('');
  const [seeded, setSeeded] = useState(false);
  const [submitted, setSubmitted] = useState(false);

  // Seed the form from the entry being edited, once — without this guard a
  // background refetch would clobber whatever the guest is mid-typing.
  useEffect(() => {
    if (editing && !seeded) {
      setTitle(editing.title);
      setBody(editing.body);
      setSeeded(true);
    }
  }, [editing, seeded]);

  // Derived, not stored: when there's exactly one eligible stay, it's the
  // effective choice immediately — no separate "select it" step, and no
  // risk of submitting before a setState from a click has landed.
  const bookingId = pickedId ?? (eligible.length === 1 ? eligible[0].booking_id : null);
  const chosen = bookingId ? eligible.find((b) => b.booking_id === bookingId) : undefined;

  const create = useMutation({
    mutationFn: () =>
      api<JournalEntry>('/journal', {
        method: 'POST',
        body: { booking_id: bookingId, title: title.trim(), body: body.trim() },
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['journal-mine'] });
      void queryClient.invalidateQueries({ queryKey: ['journal-eligible-bookings'] });
      setSubmitted(true);
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : 'Could not submit your story.'),
  });

  const save = useMutation({
    mutationFn: () =>
      api<JournalEntry>(`/journal/${entryId}`, {
        method: 'PUT',
        body: { title: title.trim(), body: body.trim() },
      }),
    onSuccess: () => {
      toast.success('Story updated.');
      void queryClient.invalidateQueries({ queryKey: ['journal-mine'] });
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : 'Could not save your changes.'),
  });

  // ── edit mode ──
  if (entryId) {
    if (mineQuery.isLoading) {
      return (
        <div className="mx-auto max-w-2xl px-4 py-10">
          <div className="flex justify-center py-20">
            <Spinner />
          </div>
        </div>
      );
    }
    if (!editing) {
      return (
        <div className="mx-auto max-w-2xl px-4 py-10">
          <EmptyState title="Story not found" hint="It may have been removed." />
        </div>
      );
    }
    if (editing.status !== 'pending') {
      return (
        <div className="mx-auto max-w-2xl px-4 py-10">
          <PageHeader title="Edit story" />
          <EmptyState
            title="This story has already been reviewed"
            hint="Once a story is approved or not posted, it can no longer be edited."
          />
        </div>
      );
    }
    return (
      <div className="mx-auto max-w-2xl px-4 py-10">
        <PageHeader title="Edit your story" subtitle="Only editable while your story is still pending review." />
        <Card>
          <form
            className="space-y-4"
            onSubmit={(e) => {
              e.preventDefault();
              save.mutate();
            }}
          >
            <Field label="Title">
              <Input required value={title} onChange={(e) => setTitle(e.target.value)} />
            </Field>
            <Field label="Story">
              <Textarea required rows={10} value={body} onChange={(e) => setBody(e.target.value)} />
            </Field>
            <div className="flex justify-end gap-2">
              <Link to="/my-bookings">
                <Button type="button" variant="ghost">
                  Cancel
                </Button>
              </Link>
              <Button type="submit" size="lg" disabled={save.isPending}>
                {save.isPending ? 'Saving…' : 'Save changes'}
              </Button>
            </div>
          </form>
        </Card>
      </div>
    );
  }

  // ── new story mode ──
  return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <PageHeader title="Share your story" subtitle="Fishing reports, camp memories — no reviews, just stories." />

      {eligibleQuery.isLoading ? (
        <div className="flex justify-center py-20">
          <Spinner />
        </div>
      ) : submitted ? (
        <Card className="text-center">
          <BookOpen className="mx-auto mb-3 text-forest-600" size={30} />
          <h3 className="font-display text-xl font-bold text-charcoal">Thanks for sharing!</h3>
          <p className="mt-2 text-muted">Your story will be posted once it's reviewed.</p>
          <div className="mt-5 flex justify-center gap-3">
            <Link to="/my-bookings">
              <Button variant="ghost">My Bookings</Button>
            </Link>
            <Link to="/journal">
              <Button>Read the journal</Button>
            </Link>
          </div>
        </Card>
      ) : eligible.length === 0 ? (
        <EmptyState
          icon={<Tent size={26} />}
          title="You'll be able to share a story after your stay!"
          hint="Complete checkout for a stay and you can journal about it here."
        />
      ) : !chosen ? (
        <Card>
          <Field label="Which stay is this story about?">
            <select
              className="w-full rounded-lg border border-sand bg-white px-3 py-2.5 text-sm text-charcoal focus:border-forest-500 focus:outline-none"
              value=""
              onChange={(e) => setPickedId(e.target.value)}
            >
              <option value="" disabled>
                Pick a stay…
              </option>
              {eligible.map((b) => (
                <option key={b.booking_id} value={b.booking_id}>
                  {formatRange(b.check_in, b.check_out)}
                </option>
              ))}
            </select>
          </Field>
        </Card>
      ) : (
        <Card>
          <p className="mb-4 text-sm font-semibold text-forest-700">
            {formatRange(chosen.check_in, chosen.check_out)}
          </p>
          <form
            className="space-y-4"
            onSubmit={(e) => {
              e.preventDefault();
              create.mutate();
            }}
          >
            <Field label="Title">
              <Input
                required
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder="Redfish on the falling tide"
              />
            </Field>
            <Field label="Story">
              <Textarea
                required
                rows={10}
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder="Tell us about your stay…"
              />
            </Field>
            <div className="flex justify-end">
              <Button type="submit" size="lg" disabled={create.isPending}>
                <Send size={16} /> {create.isPending ? 'Submitting…' : 'Submit Story'}
              </Button>
            </div>
          </form>
        </Card>
      )}
    </div>
  );
}
