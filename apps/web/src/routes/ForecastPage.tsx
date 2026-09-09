import { useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { addDays } from 'date-fns';
import { CalendarRange, Fish } from 'lucide-react';
import { ForecastAboutButton } from '../components/ForecastAbout';
import { ForecastDayCard } from '../components/ForecastDay';
import { Card, EmptyState, Field, Input, PageHeader, Spinner } from '../components/ui';
import { ApiError, api } from '../lib/api';
import { formatRange, toKey } from '../lib/dates';
import type { FishingForecast } from '../lib/types';

/** The landing widget's window: today plus the next six days. */
const DEFAULT_SPAN_DAYS = 6;

export default function ForecastPage() {
  // `from`/`to` are the same query params the booking form already speaks, so
  // /book can hand a guest straight here with their dates in place.
  const [params] = useSearchParams();
  const today = toKey(new Date());

  const [from, setFrom] = useState(params.get('from') ?? today);
  const [to, setTo] = useState(params.get('to') ?? toKey(addDays(new Date(), DEFAULT_SPAN_DAYS)));

  const valid = Boolean(from && to && to >= from);

  const { data, isLoading, isError, error } = useQuery({
    queryKey: ['fishing-forecast', from, to],
    queryFn: () =>
      api<FishingForecast>(`/fishing-forecast?start=${from}&end=${to}`, { anonymous: true }),
    enabled: valid,
    staleTime: 60 * 60_000,
    // The range caps are the server's to enforce; a rejected range is a settled
    // answer, not a blip, so don't spend three round trips rediscovering it.
    retry: false,
  });

  const beyondWeather = useMemo(
    () => data?.days.filter((d) => !d.weather_included).length ?? 0,
    [data],
  );

  return (
    <div className="mx-auto max-w-5xl px-4 py-10">
      <PageHeader
        title="Fishing forecast"
        subtitle="Solunar bite windows and a 1–5 star rating for any stretch of dates — handy before you settle on a weekend."
        actions={<ForecastAboutButton />}
      />

      <Card className="mb-6">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="From">
            <Input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
          </Field>
          <Field label="To">
            <Input type="date" min={from} value={to} onChange={(e) => setTo(e.target.value)} />
          </Field>
        </div>

        {valid ? (
          <p className="mt-3 flex items-center gap-1.5 text-sm text-muted">
            <CalendarRange size={15} />
            {formatRange(from, to)}
            {data && ` · ${data.days.length} ${data.days.length === 1 ? 'day' : 'days'}`}
          </p>
        ) : (
          <p className="mt-3 text-sm text-muted">Pick an end date on or after the start date.</p>
        )}
      </Card>

      {!valid ? null : isLoading ? (
        <div className="flex justify-center py-16">
          <Spinner />
        </div>
      ) : isError ? (
        <EmptyState
          icon={<Fish size={28} />}
          title={
            error instanceof ApiError && error.status === 400
              ? error.message
              : "Couldn't work out a forecast for those dates."
          }
          hint={
            error instanceof ApiError && error.status === 400
              ? 'Pick a shorter or nearer range and try again.'
              : 'Try again in a few minutes.'
          }
        />
      ) : (
        <>
          {beyondWeather > 0 && (
            <div className="mb-4 rounded-xl border border-sand bg-cream-dark/60 px-4 py-3">
              <p className="text-sm font-semibold text-charcoal">
                {beyondWeather === data?.days.length
                  ? 'These dates are past the weather forecast.'
                  : `The last ${beyondWeather} of these days are past the weather forecast.`}
              </p>
              <p className="mt-0.5 text-sm text-muted">
                Moon position and tide strength run years ahead, so those days still score — they
                just don&apos;t carry the wind and barometer part yet. Check back within a week of
                the dates for the full picture.
              </p>
            </div>
          )}

          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {data?.days.map((d) => (
              <ForecastDayCard key={d.date} day={d} />
            ))}
          </div>

          <p className="mt-6 text-xs text-muted">
            {data?.disclaimer ?? 'Based on solunar theory — a fun guide, not a guarantee!'}
          </p>
        </>
      )}
    </div>
  );
}
