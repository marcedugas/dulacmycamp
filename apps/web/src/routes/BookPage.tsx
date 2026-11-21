import { useMemo, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { addDays } from 'date-fns';
import { toast } from 'sonner';
import { CheckCircle2, Fish, Info, Lock, TriangleAlert, Users } from 'lucide-react';
import { buildIndex } from '../components/CampCalendar';
import { Button, Card, CountInput, Field, Input, PageHeader, Textarea, cx } from '../components/ui';
import { ReservationVisibility } from '../components/ReservationVisibility';
import { ContactFields, contactFromUser, type ContactValues } from '../components/ContactFields';
import { api, ApiError } from '../lib/api';
import { useAuth } from '../lib/auth';
import { WATERCRAFT_OPTIONS, phoneError } from '../lib/contact';
import { useCalendarData } from '../lib/queries';
import { daysInclusive, formatRange, nightCount, parseDay, pluralNights, toKey } from '../lib/dates';
import type { BookingWriteResponse, WatercraftKind } from '../lib/types';

export default function BookPage() {
  const [params] = useSearchParams();
  const queryClient = useQueryClient();
  const { bookings, blackouts, events, occupancy, capacityLimit } = useCalendarData();
  const { user, refresh } = useAuth();

  // Guests must tell us a little about themselves; family and admin accounts
  // are known to us, so for them everything below is optional. The server
  // enforces the same rule — this only makes the form say it.
  const mustAnswer = user?.role === 'guest';
  const [contact, setContact] = useState<ContactValues>(() => contactFromUser(user));
  const [fullName, setFullName] = useState(user?.full_name ?? '');
  // Everything person-level already on file: show it collapsed, not asked again.
  const onFile = Boolean(user?.phone && user.home_city && user.home_state && user.full_name?.trim());
  const [editingContact, setEditingContact] = useState(!onFile);
  const [referral, setReferral] = useState('');
  const [watercraft, setWatercraft] = useState<WatercraftKind | null>(null);
  // A returning guest's saved boat info prefills this (the profile field itself
  // is untouched). Only sent when a watercraft other than "none" is chosen.
  const [craftDetails, setCraftDetails] = useState(user?.boat_info ?? '');
  // Honeypot — see the hidden input below.
  const [fax, setFax] = useState('');
  const [showErrors, setShowErrors] = useState(false);

  const today = toKey(new Date());
  // Dates restored from the URL (Book Now → login → back here) are checked the
  // way the form checks them: real, not in the past, check-out after check-in.
  // Anything off falls back to the plain defaults rather than erroring.
  const restored = useMemo(() => {
    const from = params.get('from');
    const to = params.get('to');
    const ok = (v: string | null): v is string => !!v && /^\d{4}-\d{2}-\d{2}$/.test(v) && !Number.isNaN(Date.parse(v));
    if (ok(from) && ok(to) && from >= today && to > from) return { from, to };
    return null;
  }, [params, today]);
  const count = (key: string, min: number, fallback: number) => {
    const n = Number(params.get(key));
    return Number.isInteger(n) && n >= min && n <= 30 ? n : fallback;
  };
  const [checkIn, setCheckIn] = useState(restored?.from ?? today);
  const [checkOut, setCheckOut] = useState(restored?.to ?? toKey(addDays(new Date(), 2)));
  const [adults, setAdults] = useState(() => count('adults', 1, 2));
  const [kids, setKids] = useState(() => count('kids', 0, 0));
  const [pets, setPets] = useState(params.get('pets') === '1');
  const [requests, setRequests] = useState('');
  // Private until the booker says otherwise, matching the column default.
  const [isPrivate, setIsPrivate] = useState(true);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<BookingWriteResponse | null>(null);

  const index = useMemo(
    () => buildIndex(bookings, blackouts, events, occupancy),
    [bookings, blackouts, events, occupancy],
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

  const errors = {
    full_name: mustAnswer && !fullName.trim() ? 'Please enter your full name.' : null,
    phone:
      phoneError(contact.phone) ??
      (mustAnswer && !contact.phone.trim() ? 'Please enter a phone number we can reach you at.' : null),
    home_city: mustAnswer && !contact.home_city.trim() ? 'Please enter your city.' : null,
    home_state: mustAnswer && !contact.home_state.trim() ? 'Please enter your state.' : null,
    referral:
      mustAnswer && !referral.trim()
        ? 'Please tell us who referred you. If nobody did, type "none".'
        : null,
    watercraft:
      mustAnswer && !watercraft ? 'Please choose one. Pick "None" if you aren\'t bringing any.' : null,
  };
  const hasErrors = Object.values(errors).some(Boolean);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (hasErrors) {
      setShowErrors(true);
      setEditingContact(true);
      toast.error('Please fill in the highlighted questions.');
      return;
    }
    setBusy(true);
    try {
      const res = await api<BookingWriteResponse>('/bookings', {
        method: 'POST',
        body: {
          check_in: checkIn,
          check_out: checkOut,
          guest_count_adults: adults,
          guest_count_kids: kids,
          has_pets: pets,
          other_requests: requests.trim() || null,
          is_private: isPrivate,
          full_name: fullName.trim() || null,
          phone: contact.phone.trim() || null,
          home_city: contact.home_city.trim() || null,
          home_state: contact.home_state.trim() || null,
          mailing_address: contact.mailing_address.trim() || null,
          referral: referral.trim() || null,
          watercraft,
          watercraft_details: watercraft && watercraft !== 'none' ? craftDetails.trim() || null : null,
          fax,
        },
      });
      // The booking saved their contact info on their profile; pull it so the
      // next request is prefilled.
      void refresh();
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
            <div className="-mt-2 space-y-1.5">
              <p className="text-sm text-muted">
                {formatRange(checkIn, checkOut)} · {pluralNights(nightCount(checkIn, checkOut))}
              </p>
              {/* The forecast page reads the same `from`/`to` params this page does. */}
              <Link
                to={`/forecast?from=${checkIn}&to=${checkOut}`}
                className="inline-flex items-center gap-1.5 text-sm font-semibold text-forest-600 hover:text-forest-700"
              >
                <Fish size={15} />
                Check the fishing forecast for these dates
              </Link>
            </div>
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
              <CountInput min={1} max={30} required value={adults} onChange={setAdults} />
            </Field>
            <Field label="Kids">
              <CountInput min={0} max={30} value={kids} onChange={setKids} />
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

          {/* ── About you ── */}
          <fieldset className="space-y-4 rounded-lg border border-sand p-4">
            <legend className="px-1 text-sm font-bold text-charcoal">About you</legend>
            {onFile && !editingContact ? (
              <p className="text-sm text-muted">
                Contact info on file for {user?.full_name} ({user?.home_city}, {user?.home_state}).{' '}
                <button
                  type="button"
                  className="font-semibold text-forest-700 underline"
                  onClick={() => setEditingContact(true)}
                >
                  (edit)
                </button>
              </p>
            ) : (
              <>
              <Field label={`Full name${mustAnswer ? '' : ' (optional)'}`}>
                <Input
                  autoComplete="name"
                  maxLength={100}
                  value={fullName}
                  onChange={(e) => setFullName(e.target.value)}
                  placeholder="Jean Dugas"
                />
                {showErrors && errors.full_name && (
                  <p className="mt-1 text-sm text-clay">{errors.full_name}</p>
                )}
              </Field>
              <ContactFields
                values={contact}
                onChange={setContact}
                required={mustAnswer}
                errors={
                  showErrors
                    ? { phone: errors.phone, home_city: errors.home_city, home_state: errors.home_state }
                    : { phone: phoneError(contact.phone) }
                }
              />
              </>
            )}
          </fieldset>

          {/* ── About your stay ── */}
          <fieldset className="space-y-4 rounded-lg border border-sand p-4">
            <legend className="px-1 text-sm font-bold text-charcoal">About your stay</legend>
            <Field
              label={`Who referred you, or how did you hear about the camp?${mustAnswer ? '' : ' (optional)'}`}
            >
              <Input
                maxLength={300}
                value={referral}
                onChange={(e) => setReferral(e.target.value)}
                placeholder="A name, a friend of the family, Google…"
              />
              {showErrors && errors.referral && (
                <p className="mt-1 text-sm text-clay">{errors.referral}</p>
              )}
            </Field>

            <div role="radiogroup" aria-label="Are you bringing a boat or other watercraft?">
              <p className="mb-1.5 text-sm font-semibold text-charcoal">
                Are you bringing a boat or other watercraft?{mustAnswer ? '' : ' (optional)'}
              </p>
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                {WATERCRAFT_OPTIONS.map((o) => (
                  <label
                    key={o.value}
                    className={cx(
                      'flex min-h-12 cursor-pointer items-center justify-center rounded-lg border px-3 py-2 text-center text-sm font-semibold transition',
                      watercraft === o.value
                        ? 'border-forest-600 bg-forest-50 text-forest-800'
                        : 'border-sand bg-white text-charcoal hover:bg-cream-dark',
                    )}
                  >
                    <input
                      type="radio"
                      name="watercraft"
                      value={o.value}
                      checked={watercraft === o.value}
                      onChange={() => setWatercraft(o.value)}
                      className="sr-only"
                    />
                    {o.label}
                  </label>
                ))}
              </div>
              {showErrors && errors.watercraft && (
                <p className="mt-1 text-sm text-clay">{errors.watercraft}</p>
              )}
            </div>

            {watercraft && watercraft !== 'none' && (
              <Field label="Tell us about it" hint='For example "18 ft bay boat, trailer". Helpful, not required.'>
                <Input
                  maxLength={300}
                  value={craftDetails}
                  onChange={(e) => setCraftDetails(e.target.value)}
                />
              </Field>
            )}
          </fieldset>

          {/* Honeypot: invisible and unreachable for people, irresistible to
              form-filling bots. Anything typed here is silently refused. */}
          <div aria-hidden="true" className="absolute -left-[9999px] h-0 w-0 overflow-hidden">
            <label>
              Fax
              <input
                type="text"
                name="fax"
                tabIndex={-1}
                autoComplete="off"
                value={fax}
                onChange={(e) => setFax(e.target.value)}
              />
            </label>
          </div>

          {/* Last thing before the submit button on purpose: whatever else a
              booker skims past, they pass through this on the way out. */}
          <ReservationVisibility value={isPrivate} onChange={setIsPrivate} />

          <div className="flex items-center gap-2 text-xs text-muted">
            <Info size={14} />
            Requests are held until the owner approves or denies them.
          </div>

          <Button type="submit" size="lg" className="w-full" disabled={busy || !valid || blocked}>
            {busy ? 'Submitting…' : 'Submit Booking Request'}
          </Button>

          <p className="text-xs leading-relaxed text-muted">
            Why we ask: so the camp owners can contact you about your stay and know who is coming.
            Only the camp owners can see this. We never sell it or share it with anyone else. You
            can ask us to delete it any time.{' '}
            <Link to="/privacy" className="font-semibold text-forest-700 underline">
              Privacy
            </Link>
          </p>
        </form>
      </Card>
    </div>
  );
}
