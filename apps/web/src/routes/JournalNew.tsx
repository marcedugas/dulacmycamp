import { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { BookOpen, Send, Tent } from 'lucide-react';
import { Button, Card, EmptyState, Field, Input, PageHeader, Spinner, Textarea } from '../components/ui';
import { CatchLog, toCatchInputs } from '../components/CatchLog';
import { JournalPhotos } from '../components/JournalPhotos';
import { JournalVisibilityChoice } from '../components/JournalVisibility';
import { api, ApiError } from '../lib/api';
import { useJournalEligibleBookings, useJournalMine } from '../lib/queries';
import { formatRange } from '../lib/dates';
import type { JournalCatchInput, JournalEntry, JournalVisibility } from '../lib/types';

/**
 * Doubles as the "new story" form (?booking_id=) and the "edit my story"
 * form (?entry_id=) — same fields, same card, just POST vs PUT underneath.
 *
 * Editing is no longer time-limited. With the review queue gone there is no
 * "already reviewed" state for a lock to protect, so an author can keep
 * adding to a stay's memory log as long as they like.
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
  // Family by default — the private option, matching the column default and
  // the reservation privacy toggle's philosophy.
  const [visibility, setVisibility] = useState<JournalVisibility>('family');
  const [catches, setCatches] = useState<JournalCatchInput[]>([]);
  const [seeded, setSeeded] = useState(false);
  const [created, setCreated] = useState<JournalEntry | null>(null);

  // Seed the form from the entry being edited, once — without this guard a
  // background refetch would clobber whatever the author is mid-typing.
  useEffect(() => {
    if (editing && !seeded) {
      setTitle(editing.title);
      setBody(editing.body);
      setVisibility(editing.visibility);
      setCatches(toCatchInputs(editing.catches));
      setSeeded(true);
    }
  }, [editing, seeded]);

  // Derived, not stored: when there's exactly one eligible stay, it's the
  // effective choice immediately — no separate "select it" step, and no
  // risk of submitting before a setState from a click has landed.
  const bookingId = pickedId ?? (eligible.length === 1 ? eligible[0].booking_id : null);
  const chosen = bookingId ? eligible.find((b) => b.booking_id === bookingId) : undefined;

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['journal-mine'] });
    void queryClient.invalidateQueries({ queryKey: ['journal-feed'], exact: false });
    void queryClient.invalidateQueries({ queryKey: ['journal-eligible-bookings'] });
    void queryClient.invalidateQueries({ queryKey: ['my-stay'] });
    void queryClient.invalidateQueries({ queryKey: ['bookings'] });
  };

  const payload = () => ({
    title: title.trim(),
    body: body.trim(),
    visibility,
    catches: catches.map((c) => ({ ...c, notes: c.notes?.trim() || null })),
  });

  const create = useMutation({
    mutationFn: () =>
      api<JournalEntry>('/journal', {
        method: 'POST',
        body: { booking_id: bookingId, ...payload() },
      }),
    onSuccess: (entry) => {
      invalidate();
      // Straight to the photo step: the entry is already live, so there is
      // nothing to "finish submitting" — only more to add if they want.
      setCreated(entry);
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : 'Could not post your story.'),
  });

  const save = useMutation({
    mutationFn: () => api<JournalEntry>(`/journal/${entryId}`, { method: 'PUT', body: payload() }),
    onSuccess: () => {
      toast.success('Story updated.');
      invalidate();
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : 'Could not save your changes.'),
  });

  const storyFields = (
    <>
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
          rows={9}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Tell us about your stay…"
        />
      </Field>
      <CatchLog catches={catches} onChange={setCatches} />
      <JournalVisibilityChoice value={visibility} onChange={setVisibility} />
    </>
  );

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
    return (
      <div className="mx-auto max-w-2xl px-4 py-10">
        <PageHeader title="Edit your story" subtitle="Changes go live straight away." />
        <Card>
          <form
            className="space-y-4"
            onSubmit={(e) => {
              e.preventDefault();
              save.mutate();
            }}
          >
            {storyFields}
            <div className="flex justify-end gap-2">
              <Link to="/journal">
                <Button type="button" variant="ghost">
                  Done
                </Button>
              </Link>
              <Button type="submit" size="lg" disabled={save.isPending}>
                {save.isPending ? 'Saving…' : 'Save changes'}
              </Button>
            </div>
          </form>
        </Card>

        <div className="mt-4">
          <JournalPhotos
            entryId={editing.id}
            photos={editing.photos}
            editable
            onChanged={invalidate}
          />
        </div>
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
      ) : created ? (
        <>
          <Card className="text-center">
            <BookOpen className="mx-auto mb-3 text-forest-600" size={30} />
            <h3 className="font-display text-xl font-bold text-charcoal">Your story is posted!</h3>
            <p className="mt-2 text-muted">
              It's live in the camp journal now — add some photos below if you'd like, or come back
              and edit it any time.
            </p>
          </Card>

          <div className="mt-4">
            <JournalPhotos
              entryId={created.id}
              photos={(mineQuery.data ?? []).find((e) => e.id === created.id)?.photos ?? []}
              editable
              onChanged={invalidate}
            />
          </div>

          <div className="mt-5 flex justify-center gap-3">
            <Link to="/my-bookings">
              <Button variant="ghost">My Bookings</Button>
            </Link>
            <Link to="/journal">
              <Button>Read the journal</Button>
            </Link>
          </div>
        </>
      ) : eligible.length === 0 ? (
        <EmptyState
          icon={<Tent size={26} />}
          title="You'll be able to share a story after your stay starts!"
          hint="Once you've checked in, you can journal about that stay here."
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
            {storyFields}
            <div className="flex justify-end">
              <Button type="submit" size="lg" disabled={create.isPending}>
                <Send size={16} /> {create.isPending ? 'Posting…' : 'Post Story'}
              </Button>
            </div>
          </form>
        </Card>
      )}
    </div>
  );
}
