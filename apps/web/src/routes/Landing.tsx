import { useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import { addDays, format } from 'date-fns';
import { ArrowRight, CalendarDays, CheckCircle2, PartyPopper, XCircle } from 'lucide-react';
import { buildIndex } from '../components/CampCalendar';
import { AboutTabs } from '../components/AboutTabs';
import {
  FishingWidget,
  LunarWidget,
  TideWidget,
  WeatherWidget,
} from '../components/EnvironmentWidgets';
import { Button, Card, EmptyState, Section, Spinner, cx } from '../components/ui';
import { assetUrl } from '../lib/api';
import { useCalendarData, useConfig, useSiteContent } from '../lib/queries';
import { daysInclusive, formatRange, parseDay, toKey } from '../lib/dates';

const DEFAULT_HERO_TITLE = 'Dulac My Camp';
const DEFAULT_HERO_SUBTITLE = 'A fishing camp in the heart of Dulac, Louisiana';

/** Quick "are these dates free?" check, answered from data already on the page. */
function AvailabilityWidget() {
  const { bookings, blackouts, events, occupancy } = useCalendarData();
  const [from, setFrom] = useState(toKey(new Date()));
  const [to, setTo] = useState(toKey(addDays(new Date(), 3)));

  const index = useMemo(
    () => buildIndex(bookings, blackouts, events, occupancy),
    [bookings, blackouts, events, occupancy],
  );

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
  const { data: content, isLoading } = useSiteContent();

  // Loading and "freshly migrated, nobody's edited it yet" render the same
  // way on purpose — these match the DB defaults from migration 0005, so
  // there's no flash between the two.
  const heroTitle = content?.hero_title ?? DEFAULT_HERO_TITLE;
  const heroSubtitle = content?.hero_subtitle ?? DEFAULT_HERO_SUBTITLE;
  const heroImageUrl = assetUrl(content?.hero_image_url);
  const rules = content?.rules ?? [];

  return (
    <>
      {/* Hero. A real photo once one's uploaded; the gradient is the
          permanent fallback, not just a loading state — see D1. */}
      <section className="relative isolate overflow-hidden bg-charcoal">
        <div
          className="absolute inset-0 bg-gradient-to-br from-forest-900 via-forest-700 to-bayou-800"
          aria-hidden
        />
        {heroImageUrl && (
          <img
            src={heroImageUrl}
            alt=""
            aria-hidden
            className="absolute inset-0 h-full w-full object-cover"
          />
        )}
        <div className="absolute inset-0 bg-charcoal/35" aria-hidden />
        <div className="wood-grain absolute inset-0 opacity-40" aria-hidden />
        <div className="relative mx-auto flex max-w-6xl flex-col items-center px-4 py-28 text-center sm:py-36">
          <p className="mb-3 text-xs font-bold uppercase tracking-[0.25em] text-forest-200">
            {config?.location ?? 'Dulac, Louisiana'}
          </p>
          <h1 className="font-display text-5xl font-extrabold text-cream drop-shadow sm:text-7xl">
            {heroTitle}
          </h1>
          <p className="mt-4 max-w-xl text-lg text-cream/85">{heroSubtitle}</p>
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

      <AboutTabs
        content={content}
        isLoading={isLoading}
        capacityAdults={config?.capacity_adults ?? 10}
      />

      <div className="bg-cream-dark/50">
        <Section title="House rules" subtitle="Short list. Leave it better than you found it.">
          {isLoading ? (
            <div className="flex justify-center py-10">
              <Spinner />
            </div>
          ) : rules.length === 0 ? (
            <EmptyState title="No rules posted yet" hint="Add some from the admin panel's Site Content tab." />
          ) : (
            <Card>
              <ul className="space-y-3">
                {rules.map((rule, i) => (
                  <li key={rule.id} className="flex gap-3 text-sm text-charcoal">
                    <span className="grid h-6 w-6 shrink-0 place-items-center rounded-full bg-forest-100 text-xs font-bold text-forest-700">
                      {i + 1}
                    </span>
                    {rule.text}
                  </li>
                ))}
              </ul>
            </Card>
          )}
        </Section>
      </div>

      <Section title="Special events" subtitle="Rodeos, tournaments and anything else worth planning around.">
        <UpcomingEvents />
      </Section>

      <div className="bg-cream-dark/50">
        <Section
          title="On the water"
          subtitle="Live conditions for the Cocodrie estuary — the water you'll actually fish — straight from NOAA, plus a solunar bite forecast."
        >
          <div className="space-y-4">
            <WeatherWidget />
            <div className="grid gap-4 md:grid-cols-2">
              <TideWidget />
              <LunarWidget />
            </div>
            <FishingWidget />
            <div className="flex justify-end">
              <Link
                to="/forecast"
                className="inline-flex items-center gap-1.5 text-sm font-semibold text-forest-600 hover:text-forest-700"
              >
                See full forecast
                <ArrowRight size={15} />
              </Link>
            </div>
          </div>
        </Section>
      </div>

    </>
  );
}
