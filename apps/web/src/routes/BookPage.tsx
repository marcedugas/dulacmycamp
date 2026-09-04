import { useMemo, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { addDays } from 'date-fns';
import { toast } from 'sonner';
import { CheckCircle2, Info, Lock, TriangleAlert, Users } from 'lucide-react';
import { buildIndex } from '../components/CampCalendar';
import { Button, Card, Field, Input, PageHeader, Textarea, cx } from '../components/ui';
import { api, ApiError } from '../lib/api';
import { useCalendarData } from '../lib/queries';
import { daysInclusive, formatRange, nightCount, parseDay, pluralNights, toKey } from '../lib/dates';
import type { CreateBookingResponse } from '../lib/types';

export default function BookPage() {
  const [params] = useSearchParams();
  const queryClient = useQueryClient();
  const { bookings, blackouts, events, capacityLimit } = useCalendarData();

  const today = toKey(new Date());
  const [checkIn, setCheckIn] = useState(params.get('from') ?? today);
  const [checkOut, setCheckOut] = useState(params.get('to') ?? toKey(addDays(new Date(), 2)));
  const [adults, setAdults] = useState(2);
  const [kids, setKids] = useState(0);
  const [pets, setPets] = useState(false);
  const [requests, setRequests] = useState('');
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<CreateBookingResponse | null>(null);

  const index = useMemo(
    () => buildIndex(bookings, blackouts, events),
    [bookings, blackouts, events],
  );

  const valid = Boolean(checkIn && checkOut && checkOut > checkIn);

  /** What the selected nights already hold: blackouts, stays, and head count. */
  const conflicts = useMemo(() => {
    if (!valid) return null;
    const nights = daysInclusive(parseDay(checkIn), addDays(parseDay(checkOut), -1));

    const blackoutReasons = new Set<string>();
    let bookedNights = 0;
    let pendingNights = 0;
    let peakAdults = 0;

    for (const n of nights) {
      const cell = index.get(toKey(n));
      if (!cell) continue;
      if (cell.blackout) blackoutReasons.add(cell.blackout.reason ?? 'Camp closed');
      if (cell.approved.length) bookedNights += 1;
      if (cell.pending.length) pendingNights += 1;
      peakAdults = Math.max(peakAdults, cell.adults);
    }

    return {
      blackoutReasons: [...blackoutReasons],
      bookedNights,
      pendingNights,
      peakAdults,
      totalAdults: peakAdults + adults,
    };
  }, [valid, checkIn, checkOut, index, adults]);

  const blocked = (conflicts?.blackoutReasons.length ?? 0) > 0;
  const overCapacity = (conflicts?.totalAdults ?? 0) > capacityLimit;
  const atCapacity = (conflicts?.totalAdults ?? 0) === capacityLimit;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const res = await api<CreateBookingResponse>('/bookings', {
        method: 'POST',
        body: {
          check_in: checkIn,
          check_out: checkOut,
          guest_count_adults: adults,
          guest_count_kids: kids,
          has_pets: pets,
          other_requests: requests.trim() || null,
        },
      });
      setDone(res);
      void queryClient.invalidateQueries({ queryKey: ['bookings'] });
      toast.success('Request submitted!');
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not submit that request.');
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <div className="mx-auto max-w-lg px-4 py-16 text-center">
        <CheckCircle2 className="mx-auto mb-4 text-forest-600" size={48} />
        <h1 className="font-display text-2xl font-bold text-charcoal">Request submitted!</h1>
        <p className="mx-auto mt-3 max-w-md text-muted">
          You&apos;ll receive an email once the owner reviews your request for{' '}
          <strong className="text-charcoal">
            {formatRange(done.booking.check_in, done.booking.check_out)}
          </strong>
          .
        </p>
        {done.warning && (
          <p className="mt-4 rounded-lg border border-amber-300 bg-amber-50 px-4 py-3 text-left text-sm text-amber-900">
            {done.warning}
          </p>
        )}
        <div className="mt-8 flex justify-center gap-3">
          <Link to="/my-bookings">
            <Button>View my bookings</Button>
          </Link>
          <Link to="/calendar">
            <Button variant="ghost">Back to calendar</Button>
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <PageHeader
        title="Request the camp"
        subtitle="The owner reviews every request — you'll hear back by email."
      />

      <Card>
        <form onSubmit={submit} className="space-y-5">
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Check in">
              <Input
                type="date"
                required
                min={today}
                value={checkIn}
                onChange={(e) => {
                  setCheckIn(e.target.value);
                  if (checkOut <= e.target.value) {
                    setCheckOut(toKey(addDays(parseDay(e.target.value), 1)));
                  }
                }}
              />
            </Field>
            <Field label="Check out">
              <Input
                type="date"
                required
                min={toKey(addDays(parseDay(checkIn || today), 1))}
                value={checkOut}
                onChange={(e) => setCheckOut(e.target.value)}
              />
            </Field>
          </div>

          {valid && (
            <p className="-mt-2 text-sm text-muted">
              {formatRange(checkIn, checkOut)} · {pluralNights(nightCount(checkIn, checkOut))}
            </p>
          )}

          {/* Unavailable dates are surfaced here rather than disabled outright —
              the owner decides on overlaps, so the guest can still ask. */}
          {blocked && (
            <div className="flex gap-2 rounded-lg border border-sand bg-cream-dark px-3 py-2.5 text-sm">
              <Lock size={16} className="mt-0.5 shrink-0 text-charcoal" />
              <div>
                <p className="font-semibold text-charcoal">These dates are blacked out.</p>
                <p className="text-muted">
                  {conflicts?.blackoutReasons.join(' · ')} — the camp is closed, so this request
                  can&apos;t be submitted.
                </p>
              </div>
            </div>
          )}

          {!blocked && conflicts && conflicts.bookedNights + conflicts.pendingNights > 0 && (
            <div className="flex gap-2 rounded-lg border border-amber-300 bg-amber-50 px-3 py-2.5 text-sm">
              <TriangleAlert size={16} className="mt-0.5 shrink-0 text-amber-700" />
              <div>
                <p className="font-semibold text-amber-900">These dates overlap an existing stay.</p>
                <p className="text-amber-800">
                  You can still ask — the owner will review both requests.
                </p>
              </div>
            </div>
          )}

          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Adults">
              <Input
                type="number"
                min={1}
                max={30}
                required
                value={adults}
                onChange={(e) => setAdults(Math.max(1, Number(e.target.value) || 1))}
              />
            </Field>
            <Field label="Kids">
              <Input
                type="number"
                min={0}
                max={30}
                value={kids}
                onChange={(e) => setKids(Math.max(0, Number(e.target.value) || 0))}
              />
            </Field>
          </div>

          {conflicts && (
            <div
              className={cx(
                'flex items-start gap-2 rounded-lg px-3 py-2.5 text-sm',
                overCapacity
                  ? 'bg-red-50 text-red-900'
                  : atCapacity
                    ? 'bg-amber-50 text-amber-900'
                    : 'bg-forest-50 text-forest-800',
              )}
            >
              <Users size={16} className="mt-0.5 shrink-0" />
              <div>
                <p className="font-semibold">
                  Total guests for these dates: {conflicts.totalAdults} adults (camp sleeps{' '}
                  {capacityLimit})
                </p>
                {overCapacity && <p>That&apos;s over capacity — the owner will review it.</p>}
                {conflicts.peakAdults > 0 && !overCapacity && (
                  <p>{conflicts.peakAdults} already approved for at least one of these nights.</p>
                )}
              </div>
            </div>
          )}

          <label className="flex items-center gap-2.5 text-sm font-semibold text-charcoal">
            <input
              type="checkbox"
              checked={pets}
              onChange={(e) => setPets(e.target.checked)}
              className="h-4 w-4 accent-forest-600"
            />
            Bringing pets
          </label>

          <Field label="Other requests" hint="Boat trailer, late arrival, anything else worth knowing.">
            <Textarea
              rows={3}
              value={requests}
              onChange={(e) => setRequests(e.target.value)}
              placeholder="Optional"
            />
          </Field>

          <div className="flex items-center gap-2 text-xs text-muted">
            <Info size={14} />
            Requests are held until the owner approves or denies them.
          </div>

          <Button type="submit" size="lg" className="w-full" disabled={busy || !valid || blocked}>
            {busy ? 'Submitting…' : 'Submit Booking Request'}
          </Button>
        </form>
      </Card>
    </div>
  );
}
