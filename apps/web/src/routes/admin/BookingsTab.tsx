import { useMemo, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { Check, CheckCircle2, Dog, Flag, TriangleAlert, X } from 'lucide-react';
import {
  Button,
  Card,
  EmptyState,
  Modal,
  Spinner,
  StatusBadge,
  Textarea,
  cx,
} from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useAdminCheckouts, useBookings } from '../../lib/queries';
import { formatRange, nightCount, parseDay, pluralNights } from '../../lib/dates';
import type { AdminCheckout, Booking, BookingStatus } from '../../lib/types';

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
                      {b.guest_count_adults}a
                      {b.guest_count_kids > 0 && ` · ${b.guest_count_kids}k`}
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
              {details.guest_count_adults} adults, {details.guest_count_kids} kids ·{' '}
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
    </>
  );
}
