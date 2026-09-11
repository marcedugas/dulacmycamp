import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { addDays } from 'date-fns';
import { toast } from 'sonner';
import { CalendarPlus, Download } from 'lucide-react';
import CampCalendar, { Legend, buildIndex, visibleDays } from '../components/CampCalendar';
import type { CalendarView } from '../components/CampCalendar';
import { Button, Card, PageHeader, Spinner } from '../components/ui';
import { useAuth } from '../lib/auth';
import { useCalendarData, useHolidays } from '../lib/queries';
import { exportCalendarPdf } from '../lib/pdf';
import { formatRange, parseDay, toKey } from '../lib/dates';

export default function CalendarPage() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const { bookings, blackouts, events, occupancy, capacityLimit, isLoading } = useCalendarData();

  // Mirrors the server's `User::sees_guest_details`: admins and anyone
  // flagged as an owner. They are the only viewers who are shown a booker on
  // a stay that isn't confirmed, so they are the only ones for whom
  // "confirmed or not?" is a question the calendar has to answer.
  const seesGuestDetails = Boolean(user && (user.role === 'admin' || user.is_owner));

  const [view, setView] = useState<CalendarView>('month');
  const [anchor, setAnchor] = useState(new Date());
  const [range, setRange] = useState<{ start?: string; end?: string }>({});

  // Year view renders all twelve months, whose week-aligned padding can spill
  // a few days into the adjacent years; every other view only needs the
  // year(s) its own visible days actually land in.
  const holidayYears = useMemo(() => {
    if (view === 'year') {
      const y = anchor.getFullYear();
      return [y - 1, y, y + 1];
    }
    return Array.from(new Set(visibleDays(view, anchor).map((d) => d.getFullYear())));
  }, [view, anchor]);
  const { data: holidays } = useHolidays(holidayYears);

  const index = useMemo(
    () => buildIndex(bookings, blackouts, events, occupancy),
    [bookings, blackouts, events, occupancy],
  );

  /**
   * First click sets check-in, second sets check-out. Clicking before the
   * start, or on a complete range, starts over from that day.
   */
  const handleDayClick = (key: string) => {
    setRange((prev) => {
      if (!prev.start || prev.end || key < prev.start) return { start: key };
      // A stay ends the morning after its last night.
      return { start: prev.start, end: toKey(addDays(parseDay(key), 1)) };
    });
  };

  const bookRange = () => {
    if (!range.start) return;
    const to = range.end ?? toKey(addDays(parseDay(range.start), 1));
    navigate(`/book?from=${range.start}&to=${to}`);
  };

  const handleExport = async () => {
    try {
      await exportCalendarPdf({ view, anchor, index, capacityLimit });
      toast.success('Calendar PDF downloaded.');
    } catch {
      toast.error("Couldn't generate the PDF.");
    }
  };

  return (
    <div className="mx-auto max-w-6xl px-4 py-10">
      {/* The old subtitle promised names were never shown, which is no longer
          unconditionally true. The part people actually rely on still is, so
          it leads: private is the default and what most stays will be. */}
      <PageHeader
        title="Camp calendar"
        subtitle="Approved stays show as booked. A name appears only when that booker chose to share it."
        actions={
          <Button variant="ghost" onClick={() => void handleExport()}>
            <Download size={16} /> Export PDF
          </Button>
        }
      />

      {isLoading ? (
        <div className="flex justify-center py-20">
          <Spinner />
        </div>
      ) : (
        <>
          <Card className="bg-cream">
            <CampCalendar
              bookings={bookings}
              blackouts={blackouts}
              events={events}
              occupancy={occupancy}
              showStatus={seesGuestDetails}
              holidays={holidays}
              capacityLimit={capacityLimit}
              view={view}
              onViewChange={setView}
              anchor={anchor}
              onAnchorChange={setAnchor}
              selection={range}
              onDayClick={view === 'year' ? undefined : handleDayClick}
            />
          </Card>

          <div className="mt-4 flex flex-wrap items-center justify-between gap-4">
            <Legend />

            <div className="flex items-center gap-3">
              {range.start && (
                <span className="text-sm text-muted">
                  {range.end
                    ? formatRange(range.start, range.end)
                    : `From ${range.start} — pick a last night`}
                </span>
              )}
              <Button onClick={bookRange} disabled={!range.start}>
                <CalendarPlus size={16} /> Book These Dates
              </Button>
            </div>
          </div>

          {view !== 'year' && !range.start && (
            <p className="mt-3 text-sm text-muted">
              Tip: click a day to start a range, then click your last night.
            </p>
          )}
        </>
      )}
    </div>
  );
}
