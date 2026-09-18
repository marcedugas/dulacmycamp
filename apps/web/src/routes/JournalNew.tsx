import { useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Tent } from 'lucide-react';
import { Card, EmptyState, Field, PageHeader, Spinner } from '../components/ui';
import { JournalComposer } from '../components/JournalComposer';
import { api } from '../lib/api';
import { useJournalEligibleBookings, useJournalMine } from '../lib/queries';
import { formatRange } from '../lib/dates';
import type { JournalEntry } from '../lib/types';

/**
 * Doubles as the "new story" form (?booking_id=) and the "edit my story"
 * form (?entry_id=) — the same composer either way, POST vs PUT underneath.
 *
 * Everything — story, catches, photos — is composed locally and sent by the
 * one submit button, then it's straight back to the journal. There is no
 * intermediate "now add photos" step any more; photos go in with the story.
 *
 * Editing is not time-limited. With no review queue there is no "already
 * reviewed" state for a lock to protect.
 */
export default function JournalNew() {
  const [params] = useSearchParams();
  const entryId = params.get('entry_id');
  const fromQuery = params.get('booking_id');
  const navigate = useNavigate();

  const eligibleQuery = useJournalEligibleBookings();
  const mineQuery = useJournalMine();
  const queryClient = useQueryClient();

  const eligible = eligibleQuery.data ?? [];
  const editing = entryId ? (mineQuery.data ?? []).find((e) => e.id === entryId) : undefined;

  const [pickedId, setPickedId] = useState<string | null>(fromQuery);

  // Derived, not stored: when there's exactly one eligible stay, it's the
  // effective choice immediately — no separate "select it" step, and no
  // risk of submitting before a setState from a click has landed.
  const bookingId = pickedId ?? (eligible.length === 1 ? eligible[0].booking_id : null);
  const chosen = bookingId ? eligible.find((b) => b.booking_id === bookingId) : undefined;

  // Only once the whole submit is over. Invalidating mid-way would drop the
  // just-journaled stay from `eligible` and unmount the form out from under
  // a "some photos failed" retry.
  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['journal-mine'] });
    void queryClient.invalidateQueries({ queryKey: ['journal-feed'], exact: false });
    void queryClient.invalidateQueries({ queryKey: ['journal-eligible-bookings'] });
    void queryClient.invalidateQueries({ queryKey: ['my-stay'] });
    void queryClient.invalidateQueries({ queryKey: ['bookings'] });
  };

  const done = (message: string) => (outcome: { skippedPhotos: number }) => {
    invalidate();
    toast.success(
      outcome.skippedPhotos > 0
        ? `${message} Some photos weren't attached — you can add them by editing it.`
        : message,
    );
    navigate('/journal');
  };

  // Back to wherever they came from (My Stay, My Bookings, the feed) — or
  // the journal, if this page was opened directly and there's no "back".
  const leave = () => {
    const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0;
    if (idx > 0) navigate(-1);
    else navigate('/journal');
  };

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
        <PageHeader
          title="Edit your story"
          subtitle="Nothing changes until you press Save Changes."
        />
        <Card>
          <JournalComposer
            key={editing.id}
            source={editing}
            save={(payload) =>
              api<JournalEntry>(`/journal/${editing.id}`, { method: 'PUT', body: payload })
            }
            submitLabel="Save Changes"
            submittingLabel="Saving…"
            onDone={done('Your story is updated.')}
            onCancel={leave}
          />
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
          <JournalComposer
            key={chosen.booking_id}
            save={(payload) =>
              api<JournalEntry>('/journal', {
                method: 'POST',
                body: { booking_id: chosen.booking_id, ...payload },
              })
            }
            submitLabel="Post Story"
            submittingLabel="Posting…"
            onDone={done('Your story is posted!')}
            onCancel={leave}
          />
        </Card>
      )}
    </div>
  );
}
