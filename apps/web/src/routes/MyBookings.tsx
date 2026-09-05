import { Link } from 'react-router-dom';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { differenceInCalendarDays } from 'date-fns';
import { toast } from 'sonner';
import { BookOpen, CalendarPlus, CheckCircle2, ClipboardCheck, Dog, Tent, Users } from 'lucide-react';
import {
  Button,
  Card,
  EmptyState,
  JournalStatusBadge,
  PageHeader,
  Spinner,
  StatusBadge,
} from '../components/ui';
import { api, ApiError } from '../lib/api';
import { useBookings } from '../lib/queries';
import { formatRange, nightCount, parseDay, pluralNights } from '../lib/dates';
import type { Booking } from '../lib/types';

/** "3 days until your stay!" — only for confirmed, future stays. */
function countdown(b: Booking): string | null {
  if (b.status !== 'approved') return null;
  const days = differenceInCalendarDays(parseDay(b.check_in), new Date());
  if (days < 0) return null;
  if (days === 0) return 'Your stay starts today!';
  if (days === 1) return '1 day until your stay!';
  return `${days} days until your stay!`;
}

// "Today" is computed in UTC (not local time) to match the server's own
// Utc::now().date_naive() — these are eligibility mirrors, and drifting a
// day off the backend's actual answer near midnight would be worse than a
// plain UTC comparison ever is.
const todayUtc = () => new Date().toISOString().slice(0, 10);

/** Mirrors the server's checkout-eligibility rule (see checkout::is_checkout_eligible). */
function isCheckoutEligible(b: Booking): boolean {
  return b.status === 'approved' && !b.checked_out && b.check_out <= todayUtc();
}

/**
 * Mirrors the server's journal-eligibility rule (see journal::is_journal_eligible):
 * approved, the stay has started, no entry yet. Checkout is no longer a
 * prerequisite — a guest can write about a stay any time after it begins.
 */
function isJournalEligible(b: Booking): boolean {
  return b.status === 'approved' && !b.journal_id && b.check_in <= todayUtc();
}

function BookingCard({ booking, onCancel, cancelling }: {
  booking: Booking;
  onCancel: (id: string) => void;
  cancelling: boolean;
}) {
  const soon = countdown(booking);
  const canCancel = booking.status === 'pending' || booking.status === 'approved';
  const checkoutEligible = isCheckoutEligible(booking);

  return (
    <Card>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="font-display text-lg font-bold text-charcoal">
            {formatRange(booking.check_in, booking.check_out)}
          </h3>
          <p className="text-sm text-muted">
            {pluralNights(nightCount(booking.check_in, booking.check_out))}
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-1.5">
          <StatusBadge status={booking.status} />
          {booking.checked_out && (
            <span className="inline-flex items-center gap-1 rounded-full border border-forest-300 bg-forest-100 px-2.5 py-0.5 text-xs font-semibold text-forest-800">
              <CheckCircle2 size={12} /> Checked out
            </span>
          )}
          {booking.journal_status && <JournalStatusBadge status={booking.journal_status} />}
        </div>
      </div>

      <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1.5 text-sm text-muted">
        <span className="flex items-center gap-1.5">
          <Users size={14} />
          {booking.guest_count_adults} adults
          {booking.guest_count_kids > 0 && `, ${booking.guest_count_kids} kids`}
        </span>
        {booking.has_pets && (
          <span className="flex items-center gap-1.5">
            <Dog size={14} /> Pets
          </span>
        )}
      </div>

      {booking.other_requests && (
        <p className="mt-3 rounded-lg bg-cream-dark/60 px-3 py-2 text-sm text-charcoal">
          {booking.other_requests}
        </p>
      )}

      {booking.denied_reason && (
        <p className="mt-3 rounded-lg border-l-2 border-clay bg-red-50 px-3 py-2 text-sm text-red-900">
          {booking.denied_reason}
        </p>
      )}

      {soon && (
        <p className="mt-3 rounded-lg bg-forest-100 px-3 py-2 text-sm font-semibold text-forest-800">
          🎣 {soon}
        </p>
      )}

      {(checkoutEligible || isJournalEligible(booking) || booking.journal_status === 'pending' || canCancel) && (
        <div className="mt-4 flex flex-wrap justify-end gap-2">
          {checkoutEligible && (
            <Link to="/checkout">
              <Button size="sm">
                <ClipboardCheck size={14} /> Complete Checkout
              </Button>
            </Link>
          )}
          {isJournalEligible(booking) && (
            <Link to={`/journal/new?booking_id=${booking.id}`}>
              <Button size="sm" variant="secondary">
                <BookOpen size={14} /> Share your story
              </Button>
            </Link>
          )}
          {booking.journal_status === 'pending' && booking.journal_id && (
            <Link to={`/journal/new?entry_id=${booking.journal_id}`}>
              <Button size="sm" variant="ghost">
                View / edit story
              </Button>
            </Link>
          )}
          {canCancel && (
            <Button
              variant="ghost"
              size="sm"
              disabled={cancelling}
              onClick={() => onCancel(booking.id)}
            >
              Cancel
            </Button>
          )}
        </div>
      )}
    </Card>
  );
}

export default function MyBookings() {
  const { data, isLoading } = useBookings({ mine: true });
  const queryClient = useQueryClient();

  const cancel = useMutation({
    mutationFn: (id: string) => api(`/bookings/${id}/cancel`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Booking cancelled.');
      void queryClient.invalidateQueries({ queryKey: ['bookings'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not cancel that booking.'),
  });

  // Upcoming first, newest past last.
  const sorted = [...(data ?? [])].sort((a, b) => b.check_in.localeCompare(a.check_in));

  return (
    <div className="mx-auto max-w-3xl px-4 py-10">
      <PageHeader
        title="My bookings"
        actions={
          <Link to="/book">
            <Button>
              <CalendarPlus size={16} /> New Booking
            </Button>
          </Link>
        }
      />

      {isLoading ? (
        <div className="flex justify-center py-20">
          <Spinner />
        </div>
      ) : sorted.length === 0 ? (
        <EmptyState
          icon={<Tent size={26} />}
          title="No bookings yet"
          hint="Pick some dates and send the owner a request."
        />
      ) : (
        <div className="space-y-4">
          {sorted.map((b) => (
            <BookingCard
              key={b.id}
              booking={b}
              cancelling={cancel.isPending}
              onCancel={cancel.mutate}
            />
          ))}
        </div>
      )}
    </div>
  );
}
