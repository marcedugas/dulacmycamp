import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { BookOpen, CheckCircle2, ClipboardCheck, PartyPopper, TriangleAlert } from 'lucide-react';
import { Button, Card, EmptyState, Field, PageHeader, Spinner, Textarea, cx } from '../components/ui';
import { api, ApiError } from '../lib/api';
import { useChecklist, useCheckoutEligible } from '../lib/queries';
import { formatRange, nightCount, pluralNights } from '../lib/dates';
import type { CheckoutEligibleBooking, CheckoutResponse } from '../lib/types';

function ChecklistForm({ booking, onDone }: { booking: CheckoutEligibleBooking; onDone: (bookingId: string) => void }) {
  const { data: items, isLoading } = useChecklist();
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [notes, setNotes] = useState('');
  const [nudge, setNudge] = useState(false);

  const submit = useMutation({
    mutationFn: () =>
      api<CheckoutResponse>('/checkout', {
        method: 'POST',
        body: {
          booking_id: booking.id,
          checked_item_ids: Array.from(checked),
          notes: notes.trim() || null,
        },
      }),
    onSuccess: () => onDone(booking.id),
    onError: (err) => toast.error(err instanceof ApiError ? err.message : 'Could not complete checkout.'),
  });

  const toggle = (id: string) => {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  if (isLoading) {
    return (
      <div className="flex justify-center py-16">
        <Spinner />
      </div>
    );
  }

  const list = items ?? [];
  const allChecked = list.length > 0 && checked.size === list.length;

  return (
    <Card>
      <div className="mb-5">
        <h3 className="font-display text-lg font-bold text-charcoal">
          {formatRange(booking.check_in, booking.check_out)}
        </h3>
        <p className="text-sm text-muted">{pluralNights(nightCount(booking.check_in, booking.check_out))}</p>
      </div>

      {list.length === 0 ? (
        <p className="mb-5 text-sm text-muted">Nothing on the checklist right now — just lock up when you go.</p>
      ) : (
        <ul className="mb-5 space-y-1">
          {list.map((item) => (
            <li key={item.id}>
              <label className="flex cursor-pointer items-center gap-3 rounded-lg px-2 py-2.5 text-base hover:bg-cream-dark/60">
                <input
                  type="checkbox"
                  checked={checked.has(item.id)}
                  onChange={() => toggle(item.id)}
                  className="h-5 w-5 shrink-0 rounded border-sand text-forest-600 focus:ring-forest-500"
                />
                <span className={cx('text-charcoal', checked.has(item.id) && 'text-muted line-through')}>
                  {item.label}
                </span>
              </label>
            </li>
          ))}
        </ul>
      )}

      <Field label="Anything to flag before you go? (optional)">
        <Textarea
          rows={3}
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          placeholder="A running toilet, a burnt-out bulb, anything the owner should know…"
        />
      </Field>

      {nudge && !allChecked && (
        <p className="mt-3 flex items-center gap-1.5 text-sm text-amber-800">
          <TriangleAlert size={14} /> A few boxes are still unchecked — that's okay, submit whenever you're ready.
        </p>
      )}

      <div className="mt-5 flex justify-end">
        <Button
          size="lg"
          disabled={submit.isPending}
          onClick={() => {
            if (!allChecked && !nudge) {
              setNudge(true);
              return;
            }
            submit.mutate();
          }}
        >
          <ClipboardCheck size={16} /> {submit.isPending ? 'Submitting…' : 'Complete Checkout'}
        </Button>
      </div>
    </Card>
  );
}

function JournalPrompt({ bookingId }: { bookingId: string }) {
  return (
    <Card className="text-center">
      <PartyPopper className="mx-auto mb-3 text-forest-600" size={30} />
      <h3 className="font-display text-xl font-bold text-charcoal">Thanks, safe travels!</h3>
      <p className="mt-2 text-muted">Want to leave a story from your stay?</p>
      <div className="mt-5 flex flex-wrap justify-center gap-3">
        <Link to={`/journal/new?booking_id=${bookingId}`}>
          <Button size="lg">
            <BookOpen size={16} /> Share Your Story
          </Button>
        </Link>
        <Link to="/my-bookings">
          <Button size="lg" variant="ghost">
            Skip
          </Button>
        </Link>
      </div>
    </Card>
  );
}

export default function Checkout() {
  const { data, isLoading } = useCheckoutEligible();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [doneBookingId, setDoneBookingId] = useState<string | null>(null);

  const eligible = data ?? [];
  const selected = selectedId
    ? (eligible.find((b) => b.id === selectedId) ?? null)
    : eligible.length === 1
      ? eligible[0]
      : null;

  const handleDone = (bookingId: string) => {
    void queryClient.invalidateQueries({ queryKey: ['checkout-eligible'] });
    void queryClient.invalidateQueries({ queryKey: ['bookings'] });
    setDoneBookingId(bookingId);
  };

  return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <PageHeader title="Checkout" subtitle="Wrapping up your stay at Dulac My Camp." />

      {isLoading ? (
        <div className="flex justify-center py-20">
          <Spinner />
        </div>
      ) : doneBookingId ? (
        <JournalPrompt bookingId={doneBookingId} />
      ) : eligible.length === 0 ? (
        <EmptyState
          icon={<CheckCircle2 size={26} />}
          title="Nothing to check out right now"
          hint="Hope you had a great stay!"
        />
      ) : selected ? (
        <ChecklistForm booking={selected} onDone={handleDone} />
      ) : (
        <div className="space-y-3">
          <p className="text-sm text-muted">You have more than one stay ready for checkout — pick one:</p>
          {eligible.map((b) => (
            <button
              key={b.id}
              onClick={() => setSelectedId(b.id)}
              className="block w-full rounded-xl border border-sand bg-white p-4 text-left transition hover:border-forest-400"
            >
              <p className="font-semibold text-charcoal">{formatRange(b.check_in, b.check_out)}</p>
              <p className="text-sm text-muted">{pluralNights(nightCount(b.check_in, b.check_out))}</p>
            </button>
          ))}
        </div>
      )}

      {!isLoading && eligible.length === 0 && !doneBookingId && (
        <div className="mt-5 text-center">
          <Button variant="ghost" onClick={() => navigate('/my-bookings')}>
            Back to My Bookings
          </Button>
        </div>
      )}
    </div>
  );
}
