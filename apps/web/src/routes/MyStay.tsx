import { Link } from 'react-router-dom';
import { BookOpen, ClipboardCheck, KeyRound, Tent, Users } from 'lucide-react';
import { Button, Card, EmptyState, PageHeader, Spinner } from '../components/ui';
import { useCheckinInfo, useMyStay } from '../lib/queries';
import { formatRange, nightCount, pluralNights } from '../lib/dates';

export default function MyStay() {
  const { data: stay, isLoading: stayLoading } = useMyStay();
  const { data: info, isLoading: infoLoading, isError: noCheckinAccess } = useCheckinInfo();

  if (stayLoading) {
    return (
      <div className="mx-auto max-w-2xl px-4 py-10">
        <div className="flex justify-center py-20">
          <Spinner />
        </div>
      </div>
    );
  }

  if (!stay?.has_stay) {
    return (
      <div className="mx-auto max-w-2xl px-4 py-10">
        <PageHeader title="My Stay" />
        <EmptyState
          icon={<Tent size={26} />}
          title="Nothing to show yet"
          hint="Once a booking is approved, your stay details and check-in info will show up here."
        />
        <div className="mt-5 text-center">
          <Link to="/book">
            <Button>Book a Stay</Button>
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <PageHeader title="My Stay" subtitle="Your stay details and everything you need for arrival." />

      <Card>
        <h3 className="font-display text-lg font-bold text-charcoal">
          {formatRange(stay.check_in!, stay.check_out!)}
        </h3>
        <p className="text-sm text-muted">{pluralNights(nightCount(stay.check_in!, stay.check_out!))}</p>
        <p className="mt-2 flex items-center gap-1.5 text-sm text-muted">
          <Users size={14} />
          {stay.guest_count_adults} adults
          {Boolean(stay.guest_count_kids) && `, ${stay.guest_count_kids} kids`}
        </p>

        {(stay.checkout_eligible || (stay.checked_out && !stay.has_journal_entry)) && (
          <div className="mt-4 flex flex-wrap gap-2">
            {stay.checkout_eligible && (
              <Link to="/checkout">
                <Button>
                  <ClipboardCheck size={16} /> Complete Checkout
                </Button>
              </Link>
            )}
            {stay.checked_out && !stay.has_journal_entry && (
              <Link to={`/journal/new?booking_id=${stay.booking_id}`}>
                <Button variant="secondary">
                  <BookOpen size={16} /> Share your story
                </Button>
              </Link>
            )}
          </div>
        )}
      </Card>

      {!noCheckinAccess && (
        <div className="mt-8">
          <h2 className="mb-1 flex items-center gap-2 text-xl font-bold text-charcoal">
            <KeyRound size={20} className="text-forest-600" /> Check-in info
          </h2>
          <p className="mb-4 text-sm text-muted">Everything you need for arrival — save this page.</p>

          {infoLoading ? (
            <div className="flex justify-center py-10">
              <Spinner />
            </div>
          ) : !info || info.length === 0 ? (
            <EmptyState title="No check-in info posted yet" />
          ) : (
            <div className="space-y-3">
              {info.map((item) => (
                <Card key={item.id}>
                  <h3 className="font-semibold text-charcoal">{item.title}</h3>
                  <p className="mt-1 whitespace-pre-wrap text-sm text-muted">{item.body}</p>
                </Card>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
