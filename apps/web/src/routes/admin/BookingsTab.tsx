import { useMemo, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { addDays, format } from 'date-fns';
import { toast } from 'sonner';
import {
  Check,
  CheckCircle2,
  Dog,
  Flag,
  Lock,
  Pencil,
  Plus,
  Trash2,
  TriangleAlert,
  X,
} from 'lucide-react';
import {
  Button,
  Card,
  CountInput,
  EmptyState,
  Field,
  Input,
  Modal,
  Spinner,
  StatusBadge,
  Textarea,
  cx,
} from '../../components/ui';
import { ReservationVisibility } from '../../components/ReservationVisibility';
import { api, ApiError } from '../../lib/api';
import { useAdminCheckouts, useBookings, useCalendarData, useUsers } from '../../lib/queries';
import { buildIndex } from '../../components/CampCalendar';
import {
  daysInclusive,
  formatRange,
  nightCount,
  parseDay,
  pluralNights,
  toKey,
} from '../../lib/dates';
import type {
  AdminCheckout,
  Booking,
  BookingStatus,
  BookingWriteResponse,
  DeleteSummary,
} from '../../lib/types';

const STATUSES: (BookingStatus | 'all')[] = ['all', 'pending', 'approved', 'denied', 'cancelled'];

/** Ids of bookings whose nights collide with another live request. */
function findOverlaps(bookings: Booking[]): Set<string> {
  const live = bookings.filter((b) => b.status === 'approved' || b.status === 'pending');
  const clashing = new Set<string>();
  for (let i = 0; i < live.length; i += 1) {
    for (let j = i + 1; j < live.length; j += 1) {
      const a = live[i];
      const b = live[j];
      if (a.check_in < b.check_out && b.check_in < a.check_out) {
        clashing.add(a.id);
        clashing.add(b.id);
      }
    }
  }
  return clashing;
}

/**
 * For a stay arranged outside the app (phone call, in person). Submits to
 * `POST /api/admin/bookings`, which reuses the exact same booking-creation
 * logic `/book` does — same validation, blackout/overlap/capacity handling,
 * and full email chain — so the overlap/capacity warning here reads the
 * identical `BookingWriteResponse` shape the guest-facing form does.
 */
function AddBookingModal({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: () => void;
}) {
  const { data: users } = useUsers();
  const today = toKey(new Date());

  const [email, setEmail] = useState('');
  const [fullName, setFullName] = useState('');
  const [checkIn, setCheckIn] = useState(today);
  const [checkOut, setCheckOut] = useState(toKey(addDays(new Date(), 2)));
  const [adults, setAdults] = useState(2);
  const [kids, setKids] = useState(0);
  const [pets, setPets] = useState(false);
  const [requests, setRequests] = useState('');
  // Same default as the guest's own form — an admin entering a booking is
  // deciding for someone else, so the quiet answer has to be the private one.
  const [isPrivate, setIsPrivate] = useState(true);
  const [result, setResult] = useState<BookingWriteResponse | null>(null);

  const trimmedEmail = email.trim().toLowerCase();
  const matchedUser = trimmedEmail
    ? (users ?? []).find((u) => u.email.toLowerCase() === trimmedEmail)
    : undefined;
  const isNewGuest = trimmedEmail.length > 3 && trimmedEmail.includes('@') && !matchedUser;

  const reset = () => {
    setEmail('');
    setFullName('');
    setCheckIn(today);
    setCheckOut(toKey(addDays(new Date(), 2)));
    setAdults(2);
    setKids(0);
    setPets(false);
    setRequests('');
    setIsPrivate(true);
    setResult(null);
  };

  const create = useMutation({
    mutationFn: () =>
      api<BookingWriteResponse>('/admin/bookings', {
        method: 'POST',
        body: {
          email: trimmedEmail,
          full_name: fullName.trim() || null,
          check_in: checkIn,
          check_out: checkOut,
          guest_count_adults: adults,
          guest_count_kids: kids,
          has_pets: pets,
          other_requests: requests.trim() || null,
          is_private: isPrivate,
        },
      }),
    onSuccess: (res) => {
      setResult(res);
      onCreated();
      toast.success('Booking entered — it now needs the owner’s approval, same as any other request.');
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : 'Could not create that booking.'),
  });

  const close = () => {
    onClose();
    reset();
  };

  const valid = Boolean(trimmedEmail.includes('@') && checkIn && checkOut && checkOut > checkIn);

  return (
    <Modal open={open} onClose={close} title="Add a booking">
      {result ? (
        <div className="space-y-4 text-sm">
          <p className="text-charcoal">
            Entered as <strong>pending</strong> for {formatRange(result.booking.check_in, result.booking.check_out)}
            . It'll show up in the Bookings table like any other request and still needs the owner's approval.
          </p>
          {result.warning && (
            <p className="rounded-lg border border-amber-300 bg-amber-50 px-3 py-2 text-amber-900">
              {result.warning}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={reset}>
              Add another
            </Button>
            <Button onClick={close}>Done</Button>
          </div>
        </div>
      ) : (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate();
          }}
          className="space-y-4"
        >
          <Field label="Guest email">
            <Input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="guest@example.com"
            />
          </Field>
          {trimmedEmail.includes('@') && (
            <p className="-mt-2 text-xs font-semibold text-muted">
              {matchedUser
                ? `Existing guest: ${matchedUser.full_name ?? matchedUser.email}`
                : 'New guest — an account will be created for them.'}
            </p>
          )}
          {isNewGuest && (
            <Field label="Guest name" hint="Optional — only used since this is a new guest.">
              <Input value={fullName} onChange={(e) => setFullName(e.target.value)} placeholder="Full name" />
            </Field>
          )}

          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Check in">
              <Input
                type="date"
                required
                value={checkIn}
                onChange={(e) => setCheckIn(e.target.value)}
              />
            </Field>
            <Field label="Check out">
              <Input
                type="date"
                required
                value={checkOut}
                onChange={(e) => setCheckOut(e.target.value)}
              />
            </Field>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Adults">
              <CountInput
                min={1}
                required
                max={30}
                value={adults}
                onChange={setAdults}
              />
            </Field>
            <Field label="Kids">
              <CountInput
                min={0}
                max={30}
                value={kids}
                onChange={setKids}
              />
            </Field>
          </div>

          <label className="flex items-center gap-2 text-sm text-charcoal">
            <input
              type="checkbox"
              checked={pets}
              onChange={(e) => setPets(e.target.checked)}
              className="h-4 w-4 rounded border-sand text-forest-600 focus:ring-forest-500"
            />
            Bringing pets
          </label>

          <Field label="Other requests" hint="Optional">
            <Textarea rows={2} value={requests} onChange={(e) => setRequests(e.target.value)} />
          </Field>

          {/* The same block the guest's own form shows, in the same place:
              directly before the submit. An admin booking on someone's behalf
              is making this call *for* them, so it is the one form where a
              quieter version would be most wrong. The guest can change it
              afterwards from /my-bookings. */}
          <ReservationVisibility
            value={isPrivate}
            onChange={setIsPrivate}
            forGuest
            name="admin-reservation-visibility"
          />

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button type="submit" disabled={!valid || create.isPending}>
              {create.isPending ? 'Submitting…' : 'Add booking'}
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}

/**
 * Corrects what a guest submitted — dates and party size — on a booking they
 * can no longer change themselves. Works on any status: an approved stay is
 * just as likely to have the wrong number on it as a pending one, and this is
 * a correction rather than a state change.
 *
 * Overlaps and blackouts are surfaced but never block the save, which is the
 * deliberate difference from the guest-facing form: there a blackout is a hard
 * 409, because a guest is being told the camp is closed. Here the admin *is*
 * the camp, and a tool for fixing reality cannot refuse to describe it.
 *
 * The guest is emailed once, covering whatever actually moved. Re-saving a
 * booking exactly as it was is silent.
 */
function EditBookingModal({
  booking,
  onClose,
  onSaved,
}: {
  booking: Booking;
  onClose: () => void;
  onSaved: () => void;
}) {
  // An admin always receives the real counts, so these fallbacks are a floor
  // for the type rather than a state the panel reaches.
  const [checkIn, setCheckIn] = useState(booking.check_in);
  const [checkOut, setCheckOut] = useState(booking.check_out);
  const [adults, setAdults] = useState(booking.guest_count_adults ?? 1);
  const [kids, setKids] = useState(booking.guest_count_kids ?? 0);

  const { bookings, blackouts, events, occupancy } = useCalendarData();

  const datesValid = Boolean(checkIn && checkOut && checkOut > checkIn);
  const unchanged =
    checkIn === booking.check_in &&
    checkOut === booking.check_out &&
    adults === (booking.guest_count_adults ?? 1) &&
    kids === (booking.guest_count_kids ?? 0);

  /**
   * What the proposed nights already hold — the same read of the same index
   * the guest-facing /book form does, so the two describe a collision
   * identically.
   *
   * The booking being edited is filtered out first: left in, every stay would
   * report overlapping itself the moment the modal opened. That mirrors the
   * `exclude` the server passes to its own `assess_stay`.
   */
  const conflicts = useMemo(() => {
    if (!datesValid) return null;
    const others = bookings.filter((b) => b.id !== booking.id);
    const index = buildIndex(others, blackouts, events, occupancy);
    const nights = daysInclusive(parseDay(checkIn), addDays(parseDay(checkOut), -1));

    const blackoutReasons = new Set<string>();
    let bookedNights = 0;
    let pendingNights = 0;
    for (const n of nights) {
      const cell = index.get(toKey(n));
      if (!cell) continue;
      if (cell.blackout) blackoutReasons.add(cell.blackout.reason ?? 'Camp closed');
      if (cell.approved.length) bookedNights += 1;
      if (cell.pending.length) pendingNights += 1;
    }
    return { blackoutReasons: [...blackoutReasons], bookedNights, pendingNights };
  }, [datesValid, checkIn, checkOut, bookings, blackouts, events, occupancy, booking.id]);

  const save = useMutation({
    mutationFn: () =>
      api<BookingWriteResponse>(`/bookings/${booking.id}/edit`, {
        method: 'PUT',
        body: {
          check_in: checkIn,
          check_out: checkOut,
          guest_count_adults: adults,
          guest_count_kids: kids,
        },
      }),
    onSuccess: (res) => {
      // The server re-runs the same assessment against real rows; if it has
      // something to say, say it rather than a bare success.
      toast.success(
        res.warning
          ? `Booking updated — the guest has been emailed. ${res.warning}`
          : 'Booking updated. The guest has been emailed what changed.',
      );
      onSaved();
      onClose();
    },
    onError: (err: unknown) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not update that booking.'),
  });

  return (
    <Modal open onClose={onClose} title="Edit booking">
      <div className="space-y-4">
        <p className="text-sm text-charcoal">
          <strong>{booking.guest_name ?? booking.guest_email}</strong> &middot;{' '}
          <StatusBadge status={booking.status} />
        </p>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Check in">
            <Input
              type="date"
              required
              value={checkIn}
              onChange={(e) => setCheckIn(e.target.value)}
            />
          </Field>
          <Field label="Check out">
            <Input
              type="date"
              required
              value={checkOut}
              onChange={(e) => setCheckOut(e.target.value)}
            />
          </Field>
        </div>

        {datesValid ? (
          <p className="-mt-2 text-xs text-muted">
            {formatRange(checkIn, checkOut)} &middot;{' '}
            {pluralNights(nightCount(checkIn, checkOut))}
          </p>
        ) : (
          <p className="-mt-2 text-xs font-semibold text-clay">
            Check-out has to be after check-in.
          </p>
        )}

        {/* Same amber advisory the /book form shows for the same collision —
            but here it never disables the save. */}
        {conflicts && conflicts.bookedNights + conflicts.pendingNights > 0 && (
          <div className="flex gap-2 rounded-lg border border-amber-300 bg-amber-50 px-3 py-2.5 text-sm">
            <TriangleAlert size={16} className="mt-0.5 shrink-0 text-amber-700" />
            <div>
              <p className="font-semibold text-amber-900">
                These dates overlap an existing stay.
              </p>
              <p className="text-amber-800">
                You can still save — the camp allows overlaps and the owner arbitrates.
              </p>
            </div>
          </div>
        )}

        {/* A blackout is a hard stop when a guest asks for the dates. For an
            admin moving a booking it is a heads-up, so it says so plainly
            rather than looking like the same refusal. */}
        {conflicts && conflicts.blackoutReasons.length > 0 && (
          <div className="flex gap-2 rounded-lg border border-sand bg-cream-dark px-3 py-2.5 text-sm">
            <Lock size={16} className="mt-0.5 shrink-0 text-charcoal" />
            <div>
              <p className="font-semibold text-charcoal">These dates are blacked out.</p>
              <p className="text-muted">
                {conflicts.blackoutReasons.join(' · ')} — saving anyway is allowed, since you are
                the one who sets the blackouts.
              </p>
            </div>
          </div>
        )}

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Adults">
            <CountInput min={1} max={30} value={adults} onChange={setAdults} />
          </Field>
          <Field label="Kids">
            <CountInput min={0} max={30} value={kids} onChange={setKids} />
          </Field>
        </div>

        <p className="text-xs text-muted">
          {unchanged
            ? 'No change yet — the guest is only emailed if something actually moves.'
            : 'The guest will be emailed a summary of what changed. Status stays as it is.'}
        </p>

        <div className="flex justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="button"
            disabled={save.isPending || unchanged || !datesValid}
            onClick={() => save.mutate()}
          >
            {save.isPending ? 'Saving…' : 'Save'}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

export default function BookingsTab() {
  const { data, isLoading } = useBookings();
  const queryClient = useQueryClient();

  const [status, setStatus] = useState<BookingStatus | 'all'>('all');
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [denying, setDenying] = useState<Booking | null>(null);
  const [reason, setReason] = useState('');
  const [details, setDetails] = useState<Booking | null>(null);
  const [viewingCheckout, setViewingCheckout] = useState<Booking | null>(null);
  const [deleting, setDeleting] = useState<Booking | null>(null);
  const [editing, setEditing] = useState<Booking | null>(null);
  const [adding, setAdding] = useState(false);
  const { data: checkouts } = useAdminCheckouts();
  const checkoutDetail: AdminCheckout | undefined = viewingCheckout
    ? checkouts?.find((c) => c.booking_id === viewingCheckout.id)
    : undefined;

  const bookings = data ?? [];
  const overlaps = useMemo(() => findOverlaps(bookings), [bookings]);

  const filtered = bookings
    .filter((b) => status === 'all' || b.status === status)
    .filter((b) => !from || b.check_out > from)
    .filter((b) => !to || b.check_in < to)
    .sort((a, b) => b.check_in.localeCompare(a.check_in));

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['bookings'] });
  const onError = (err: unknown) =>
    toast.error(err instanceof ApiError ? err.message : 'That action failed.');

  const approve = useMutation({
    mutationFn: (id: string) => api(`/bookings/${id}/approve`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Booking approved — the guest has been emailed.');
      invalidate();
    },
    onError,
  });

  const deny = useMutation({
    mutationFn: ({ id, reason }: { id: string; reason: string }) =>
      api(`/bookings/${id}/deny`, { method: 'PUT', body: { reason: reason || null } }),
    onSuccess: () => {
      toast.success('Booking denied — the guest has been emailed.');
      setDenying(null);
      setReason('');
      invalidate();
    },
    onError,
  });

  const cancel = useMutation({
    mutationFn: (id: string) => api(`/bookings/${id}/cancel`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Booking cancelled.');
      invalidate();
    },
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api<DeleteSummary>(`/bookings/${id}`, { method: 'DELETE' }),
    onSuccess: (res) => {
      // Say what actually went, since the journal entry and checkout are the
      // parts an admin would not expect to lose.
      const also = [
        res.journal_entry && 'its journal entry',
        res.checkout && 'its checkout note',
      ].filter(Boolean);
      toast.success(
        also.length ? `Booking deleted, along with ${also.join(' and ')}.` : 'Booking deleted.',
      );
      setDeleting(null);
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

  return (
    <>
      <div className="mb-4 flex flex-wrap items-end gap-3">
        <div className="flex rounded-lg border border-sand bg-white p-0.5">
          {STATUSES.map((s) => (
            <button
              key={s}
              onClick={() => setStatus(s)}
              className={cx(
                'rounded-md px-3 py-1.5 text-xs font-semibold capitalize transition',
                status === s ? 'bg-forest-600 text-cream' : 'text-muted hover:text-charcoal',
              )}
            >
              {s}
            </button>
          ))}
        </div>
        <label className="text-xs font-semibold text-muted">
          From
          <input
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
            className="ml-2 rounded-lg border border-sand bg-white px-2 py-1.5 text-sm"
          />
        </label>
        <label className="text-xs font-semibold text-muted">
          To
          <input
            type="date"
            value={to}
            onChange={(e) => setTo(e.target.value)}
            className="ml-2 rounded-lg border border-sand bg-white px-2 py-1.5 text-sm"
          />
        </label>
        <Button size="sm" className="ml-auto" onClick={() => setAdding(true)}>
          <Plus size={14} /> Add Booking
        </Button>
      </div>

      {filtered.length === 0 ? (
        <EmptyState title="No bookings match those filters" />
      ) : (
        <div className="overflow-x-auto rounded-xl border border-sand bg-white">
          <table className="w-full min-w-[820px] text-sm">
            <thead className="border-b border-sand bg-cream-dark/60 text-left text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2.5 font-bold">Guest</th>
                <th className="px-3 py-2.5 font-bold">Dates</th>
                <th className="px-3 py-2.5 font-bold">Guests</th>
                <th className="px-3 py-2.5 font-bold">Status</th>
                <th className="px-3 py-2.5 font-bold">Submitted</th>
                <th className="px-3 py-2.5 text-right font-bold">Actions</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((b) => {
                const clash = overlaps.has(b.id);
                return (
                  <tr
                    key={b.id}
                    className={cx(
                      'border-b border-sand/70 last:border-0',
                      clash && 'bg-amber-50',
                    )}
                  >
                    <td className="px-3 py-2.5">
                      <p className="font-semibold text-charcoal">{b.guest_name ?? '—'}</p>
                      <p className="text-xs text-muted">{b.guest_email}</p>
                    </td>
                    <td className="px-3 py-2.5">
                      <span className="flex items-center gap-1.5 font-medium text-charcoal">
                        {clash && (
                          <TriangleAlert size={14} className="text-amber-600" aria-label="Overlaps another booking" />
                        )}
                        {formatRange(b.check_in, b.check_out)}
                      </span>
                      <span className="text-xs text-muted">
                        {pluralNights(nightCount(b.check_in, b.check_out))}
                      </span>
                    </td>
                    <td className="px-3 py-2.5 text-charcoal">
                      {b.guest_count_adults ?? 0}a
                      {(b.guest_count_kids ?? 0) > 0 && ` · ${b.guest_count_kids}k`}
                      {b.has_pets && <Dog size={13} className="ml-1 inline text-wood-600" />}
                    </td>
                    <td className="px-3 py-2.5">
                      <div className="flex flex-wrap items-center gap-1.5">
                        <StatusBadge status={b.status} />
                        {b.checked_out && (
                          <span
                            title={b.checkout_notes ? 'Checked out — has a note' : 'Checked out'}
                            className="inline-flex items-center gap-1 rounded-full border border-forest-300 bg-forest-100 px-2 py-0.5 text-[11px] font-semibold text-forest-800"
                          >
                            <CheckCircle2 size={11} /> Checked out
                            {b.checkout_notes && (
                              <button
                                onClick={() => setViewingCheckout(b)}
                                aria-label="View checkout note"
                                className="text-clay hover:text-clay/70"
                              >
                                <Flag size={11} />
                              </button>
                            )}
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="px-3 py-2.5 text-xs text-muted">
                      {b.created_at ? format(new Date(b.created_at), 'MMM d, yyyy') : '—'}
                    </td>
                    <td className="px-3 py-2.5">
                      <div className="flex justify-end gap-1.5">
                        {b.status === 'pending' && (
                          <>
                            <Button
                              size="sm"
                              disabled={approve.isPending}
                              onClick={() => approve.mutate(b.id)}
                            >
                              <Check size={14} /> Approve
                            </Button>
                            <Button size="sm" variant="danger" onClick={() => setDenying(b)}>
                              <X size={14} /> Deny
                            </Button>
                          </>
                        )}
                        <Button size="sm" variant="ghost" onClick={() => setDetails(b)}>
                          Details
                        </Button>
                        {/* Any status: this corrects what was submitted, it is
                            not a state change. */}
                        <Button
                          size="sm"
                          variant="ghost"
                          title="Correct the dates or the adult/kid count"
                          onClick={() => setEditing(b)}
                        >
                          <Pencil size={14} /> Edit
                        </Button>
                        {(b.status === 'approved' || b.status === 'pending') && (
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={cancel.isPending}
                            onClick={() => cancel.mutate(b.id)}
                          >
                            Cancel
                          </Button>
                        )}
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-clay"
                          // Cancel keeps the row and is what a called-off stay
                          // wants. This is for rows that shouldn't exist.
                          // A confirmed stay is real history, so the API
                          // refuses it outright — cancel is the way through,
                          // and says so here rather than failing on click.
                          title={
                            b.status === 'approved'
                              ? 'Cancel this booking first — a confirmed stay can’t be deleted directly'
                              : 'Remove this booking from the record entirely'
                          }
                          disabled={remove.isPending || b.status === 'approved'}
                          onClick={() => setDeleting(b)}
                        >
                          <Trash2 size={14} />
                        </Button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <Modal open={Boolean(denying)} onClose={() => setDenying(null)} title="Deny this request?">
        <p className="mb-3 text-sm text-muted">
          {denying?.guest_name} · {denying && formatRange(denying.check_in, denying.check_out)}
        </p>
        <Textarea
          rows={3}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          placeholder="Reason (optional — the guest will see this)"
        />
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setDenying(null)}>
            Never mind
          </Button>
          <Button
            variant="danger"
            disabled={deny.isPending}
            onClick={() => denying && deny.mutate({ id: denying.id, reason })}
          >
            {deny.isPending ? 'Denying…' : 'Deny booking'}
          </Button>
        </div>
      </Modal>

      <Modal open={Boolean(details)} onClose={() => setDetails(null)} title="Booking details">
        {details && (
          <Card className="space-y-2 text-sm">
            <p>
              <strong>{details.guest_name}</strong> · {details.guest_email}
            </p>
            <p>
              {formatRange(details.check_in, details.check_out)} ·{' '}
              {pluralNights(nightCount(details.check_in, details.check_out))}
            </p>
            <p>
              {details.guest_count_adults ?? 0} adults, {details.guest_count_kids ?? 0} kids ·{' '}
              {details.has_pets ? 'pets' : 'no pets'}
            </p>
            {details.other_requests && (
              <p className="rounded bg-cream-dark/60 px-3 py-2">{details.other_requests}</p>
            )}
            {details.approved_by && (
              <p className="text-muted">
                {details.status} by {details.approved_by}
                {details.approved_at &&
                  ` on ${format(parseDay(details.approved_at.slice(0, 10)), 'MMM d, yyyy')}`}
              </p>
            )}
            {details.denied_reason && (
              <p className="rounded bg-red-50 px-3 py-2 text-red-900">{details.denied_reason}</p>
            )}
          </Card>
        )}
      </Modal>

      <Modal open={Boolean(viewingCheckout)} onClose={() => setViewingCheckout(null)} title="Checkout note">
        {viewingCheckout && (
          <div className="space-y-3 text-sm">
            <p className="text-muted">
              {viewingCheckout.guest_name} ·{' '}
              {formatRange(viewingCheckout.check_in, viewingCheckout.check_out)}
            </p>
            {checkoutDetail?.notes && (
              <p className="rounded-lg border-l-2 border-clay bg-red-50 px-3 py-2 text-red-900">
                {checkoutDetail.notes}
              </p>
            )}
            {checkoutDetail && (
              <div>
                <p className="mb-1.5 text-xs font-bold uppercase tracking-wide text-muted">Checklist</p>
                <ul className="space-y-1">
                  {checkoutDetail.items.map((item) => (
                    <li key={item.id} className="flex items-center gap-2 text-charcoal">
                      {item.checked ? (
                        <Check size={14} className="text-forest-600" />
                      ) : (
                        <X size={14} className="text-muted" />
                      )}
                      <span className={cx(!item.checked && 'text-muted')}>{item.label}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </Modal>

      <Modal
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        title="Delete this booking?"
      >
        <div className="space-y-4">
          <p className="text-sm text-charcoal">
            {deleting && (
              <>
                <strong>{deleting.guest_name ?? deleting.guest_email}</strong>,{' '}
                {formatRange(deleting.check_in, deleting.check_out)}. This removes it from the
                record entirely and cannot be undone.
              </>
            )}
          </p>

          {/* The parts an admin would not expect to lose with the booking. */}
          {(deleting?.journal_id || deleting?.checked_out) && (
            <p className="rounded-lg border border-clay/30 bg-clay/5 px-3 py-2 text-xs text-clay">
              This also deletes{' '}
              {[
                deleting?.journal_id && 'the guest’s journal entry',
                deleting?.checked_out && 'the checkout note',
              ]
                .filter(Boolean)
                .join(' and ')}
              . Messages about this stay are kept.
            </p>
          )}

          <div className="flex justify-end gap-2">
            <Button variant="ghost" type="button" onClick={() => setDeleting(null)}>
              Keep it
            </Button>
            <Button
              variant="danger"
              type="button"
              disabled={remove.isPending}
              onClick={() => deleting && remove.mutate(deleting.id)}
            >
              {remove.isPending ? 'Deleting…' : 'Delete booking'}
            </Button>
          </div>
        </div>
      </Modal>

      {editing && (
        <EditBookingModal
          // Fresh fields for each booking opened, never the last one's.
          key={editing.id}
          booking={editing}
          onClose={() => setEditing(null)}
          onSaved={invalidate}
        />
      )}

      <AddBookingModal open={adding} onClose={() => setAdding(false)} onCreated={invalidate} />
    </>
  );
}
