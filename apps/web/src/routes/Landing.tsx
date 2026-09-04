import { useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import { addDays, format } from 'date-fns';
import {
  Anchor,
  CalendarDays,
  CheckCircle2,
  Flame,
  Image as ImageIcon,
  PartyPopper,
  Ship,
  Snowflake,
  Utensils,
  Wifi,
  XCircle,
} from 'lucide-react';
import { buildIndex } from '../components/CampCalendar';
import { LunarWidget, TideWidget, WeatherWidget } from '../components/EnvironmentWidgets';
import { Button, Card, EmptyState, Section, cx } from '../components/ui';
import { useCalendarData, useConfig } from '../lib/queries';
import { daysInclusive, formatRange, parseDay, toKey } from '../lib/dates';

// TODO(content): replace with the real amenity list from Marc/Jean.
const AMENITIES = [
  { icon: Ship, label: 'Boat slip & launch', detail: 'Covered slip; ramp two minutes away' },
  { icon: Snowflake, label: 'Central A/C & heat', detail: 'Plus a chest freezer for the catch' },
  { icon: Utensils, label: 'Full kitchen', detail: 'Stove, oven, fridge, coffee maker' },
  { icon: Flame, label: 'Outdoor cooker', detail: 'Propane burner and fish-cleaning table' },
  { icon: Wifi, label: 'Wi-Fi & TV', detail: 'Satellite internet, streaming on the big screen' },
  { icon: Anchor, label: 'Sleeps 6 adults', detail: 'More with kids on the bunks' },
];

// TODO(content): confirm house rules with the owners before launch.
const RULES = [
  'Clean up before you leave — sweep, run the dishwasher, take the trash to the road.',
  'Strip the beds and start a load of towels. Fresh linens are in the hall closet.',
  'Cut the A/C to 78° and kill the water heater breaker when you lock up.',
  'Clean fish at the outside table only, and bag the scraps — never in the bayou.',
  'No smoking inside. The porch is yours.',
  'Pets are welcome but must be asked for on the booking form.',
  'Last one out: doors locked, windows latched, boat slip cover down.',
];

/** Quick "are these dates free?" check, answered from data already on the page. */
function AvailabilityWidget() {
  const { bookings, blackouts, events } = useCalendarData();
  const [from, setFrom] = useState(toKey(new Date()));
  const [to, setTo] = useState(toKey(addDays(new Date(), 3)));

  const index = useMemo(() => buildIndex(bookings, blackouts, events), [bookings, blackouts, events]);

  const verdict = useMemo(() => {
    if (!from || !to || to <= from) return null;
    // Check the nights of the stay: check-in through the night before checkout.
    const nights = daysInclusive(parseDay(from), addDays(parseDay(to), -1));
    let blackout = false;
    let booked = false;
    for (const n of nights) {
      const cell = index.get(toKey(n));
      if (!cell) continue;
      if (cell.blackout) blackout = true;
      if (cell.approved.length > 0) booked = true;
    }
    return { blackout, booked, open: !blackout && !booked };
  }, [from, to, index]);

  return (
    <Card className="mx-auto -mt-12 max-w-3xl bg-cream shadow-lg">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end">
        <label className="flex-1">
          <span className="mb-1 block text-xs font-bold uppercase tracking-wide text-muted">
            Check in
          </span>
          <input
            type="date"
            value={from}
            min={toKey(new Date())}
            onChange={(e) => setFrom(e.target.value)}
            className="w-full rounded-lg border border-sand bg-white px-3 py-2.5 text-sm"
          />
        </label>
        <label className="flex-1">
          <span className="mb-1 block text-xs font-bold uppercase tracking-wide text-muted">
            Check out
          </span>
          <input
            type="date"
            value={to}
            min={from}
            onChange={(e) => setTo(e.target.value)}
            className="w-full rounded-lg border border-sand bg-white px-3 py-2.5 text-sm"
          />
        </label>
        <Link to={`/book?from=${from}&to=${to}`} className="sm:pb-0.5">
          <Button size="lg" className="w-full">
            Book Now
          </Button>
        </Link>
      </div>

      {verdict && (
        <div
          className={cx(
            'mt-4 flex items-start gap-2 rounded-lg px-3 py-2.5 text-sm font-semibold',
            verdict.open
              ? 'bg-forest-100 text-forest-800'
              : verdict.blackout
                ? 'bg-cream-dark text-charcoal'
                : 'bg-amber-100 text-amber-900',
          )}
        >
          {verdict.open ? (
            <CheckCircle2 size={18} className="mt-px shrink-0" />
          ) : (
            <XCircle size={18} className="mt-px shrink-0" />
          )}
          <span>
            {verdict.open
              ? `${formatRange(from, to)} is available — book it.`
              : verdict.blackout
                ? `The camp is closed for part of ${formatRange(from, to)}.`
                : `${formatRange(from, to)} is already taken. You can still ask — the owner reviews overlaps.`}
          </span>
        </div>
      )}
      {!verdict && from && to && (
        <p className="mt-4 text-sm text-muted">Pick a check-out date after your check-in date.</p>
      )}
    </Card>
  );
}

function UpcomingEvents() {
  const { events } = useCalendarData();
  const today = toKey(new Date());
  const upcoming = events
    .filter((e) => (e.end_date ?? e.event_date) >= today)
    .slice(0, 6);

  if (upcoming.length === 0) {
    return (
      <EmptyState
        icon={<PartyPopper size={26} />}
        title="No upcoming events"
        hint="Rodeos and tournaments show up here once they're on the calendar."
      />
    );
  }

  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {upcoming.map((e) => (
        <Card key={e.id} className="transition hover:border-forest-400">
          <div className="flex items-start gap-3">
            <span className="text-3xl leading-none" aria-hidden>
              {e.emoji ?? '🎉'}
            </span>
            <div className="min-w-0">
              <h3 className="font-display text-base font-bold text-charcoal">{e.name}</h3>
              <p className="text-xs font-semibold uppercase tracking-wide text-bayou-600">
                {e.end_date && e.end_date !== e.event_date
                  ? formatRange(e.event_date, e.end_date)
                  : format(parseDay(e.event_date), 'MMM d, yyyy')}
              </p>
              {e.description && <p className="mt-2 text-sm text-muted">{e.description}</p>}
            </div>
          </div>
        </Card>
      ))}
    </div>
  );
}

export default function Landing() {
  const { data: config } = useConfig();

  return (
    <>
      {/* Hero. The gradient stands in for the real camp photo — see README. */}
      <section className="relative isolate overflow-hidden bg-charcoal">
        <div
          className="absolute inset-0 bg-gradient-to-br from-forest-900 via-forest-700 to-bayou-800"
          aria-hidden
        />
        <div className="wood-grain absolute inset-0 opacity-40" aria-hidden />
        <div className="relative mx-auto flex max-w-6xl flex-col items-center px-4 py-28 text-center sm:py-36">
          <p className="mb-3 text-xs font-bold uppercase tracking-[0.25em] text-forest-200">
            {config?.location ?? 'Dulac, Louisiana'}
          </p>
          <h1 className="font-display text-5xl font-extrabold text-cream drop-shadow sm:text-7xl">
            Dulac My Camp
          </h1>
          <p className="mt-4 max-w-xl text-lg text-cream/85">
            A fishing camp in the heart of Dulac, Louisiana
          </p>
          <div className="mt-8 flex flex-wrap justify-center gap-3">
            <Link to="/calendar">
              <Button variant="secondary" size="lg">
                <CalendarDays size={18} /> Check Availability
              </Button>
            </Link>
            <Link to="/book">
              <Button variant="light" size="lg">
                Book Now
              </Button>
            </Link>
          </div>
        </div>
      </section>

      <div className="relative z-10 px-4">
        <AvailabilityWidget />
      </div>

      <Section
        title="About the camp"
        subtitle={`Family and friends only. The camp sleeps ${config?.capacity_adults ?? 6} adults comfortably — more with kids on the bunks.`}
        id="about"
      >
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {AMENITIES.map(({ icon: Icon, label, detail }) => (
            <Card key={label} className="flex items-start gap-3">
              <Icon className="mt-0.5 shrink-0 text-forest-600" size={20} />
              <div>
                <p className="font-semibold text-charcoal">{label}</p>
                <p className="text-sm text-muted">{detail}</p>
              </div>
            </Card>
          ))}
        </div>

        {/* Photo slots — drop real images in and swap these placeholders out. */}
        <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {['The camp', 'The dock', 'The bayou', 'The catch'].map((caption) => (
            <div
              key={caption}
              className="flex aspect-4/3 flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-sand bg-cream-dark/60 text-muted"
            >
              <ImageIcon size={22} />
              <span className="text-xs font-semibold uppercase tracking-wide">{caption}</span>
            </div>
          ))}
        </div>
      </Section>

      <div className="bg-cream-dark/50">
        <Section title="House rules" subtitle="Short list. Leave it better than you found it.">
          <Card>
            <ul className="space-y-3">
              {RULES.map((rule, i) => (
                <li key={rule} className="flex gap-3 text-sm text-charcoal">
                  <span className="grid h-6 w-6 shrink-0 place-items-center rounded-full bg-forest-100 text-xs font-bold text-forest-700">
                    {i + 1}
                  </span>
                  {rule}
                </li>
              ))}
            </ul>
          </Card>
        </Section>
      </div>

      <Section title="Special events" subtitle="Rodeos, tournaments and anything else worth planning around.">
        <UpcomingEvents />
      </Section>

      <div className="bg-cream-dark/50">
        <Section title="On the water" subtitle="Live conditions for Dulac, straight from NOAA.">
          <div className="space-y-4">
            <WeatherWidget />
            <div className="grid gap-4 md:grid-cols-2">
              <TideWidget />
              <LunarWidget />
            </div>
          </div>
        </Section>
      </div>
    </>
  );
}
