import { format } from 'date-fns';
import { CloudOff, Star } from 'lucide-react';
import type { FishingDay, SolunarPeriod } from '../lib/types';
import { formatPeriodTime, parseDay } from '../lib/dates';
import { cx } from './ui';

/**
 * The pieces both forecast surfaces are built from — the landing page's
 * quick-glance widget and the /forecast lookup — so a day looks and reads the
 * same wherever it appears.
 */

export function StarRating({ stars, size = 18 }: { stars: number; size?: number }) {
  return (
    <span className="flex gap-0.5" aria-label={`${stars} out of 5 stars`}>
      {[1, 2, 3, 4, 5].map((n) => (
        <Star
          key={n}
          size={size}
          className={n <= stars ? 'fill-current text-wood-500' : 'text-sand'}
        />
      ))}
    </span>
  );
}

/** The compact ★★★☆☆ form the day strip uses, where five icons won't fit. */
export function DayStars({ stars }: { stars: number }) {
  return (
    <p className="mt-1 text-sm font-bold tracking-tight text-wood-600" aria-hidden>
      {'★'.repeat(stars)}
      <span className="text-sand">{'★'.repeat(5 - stars)}</span>
    </p>
  );
}

export function PeriodColumn({ label, periods }: { label: string; periods: SolunarPeriod[] }) {
  return (
    <div>
      <p className="text-[11px] font-bold uppercase tracking-wide text-muted">{label}</p>
      {periods.length === 0 ? (
        <p className="text-muted">—</p>
      ) : (
        <ul className="mt-1 space-y-0.5">
          {periods.map((p) => (
            <li key={p.start} className="font-semibold tabular-nums text-charcoal">
              {formatPeriodTime(p.start)} – {formatPeriodTime(p.end)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * Said plainly rather than hidden: past the weather horizon the score is still
 * real, it just rests on two of its three inputs.
 */
export function MoonAndTideOnlyNote({ className }: { className?: string }) {
  return (
    <p className={cx('flex items-start gap-1.5 text-[11px] leading-snug text-muted', className)}>
      <CloudOff size={13} className="mt-px shrink-0" />
      Beyond weather forecast range — moon &amp; tide only
    </p>
  );
}

/**
 * One day, at the size the /forecast page reads it: same card tokens as the
 * landing strip, with room for the phase, the bite windows and the note.
 */
export function ForecastDayCard({ day }: { day: FishingDay }) {
  const date = parseDay(day.date);

  return (
    <div className="rounded-lg border border-sand bg-cream-dark/50 p-3">
      <div className="flex items-baseline justify-between gap-2">
        <p className="text-[11px] font-bold uppercase tracking-wide text-muted">
          {format(date, 'EEE')} <span className="font-normal">{format(date, 'MMM d')}</span>
        </p>
        <span className="text-lg leading-none" aria-hidden>
          {day.moon_emoji}
        </span>
      </div>

      <div className="mt-2 flex items-center gap-2">
        <StarRating stars={day.stars} size={15} />
        <span className="text-sm font-bold text-charcoal">{day.rating_label}</span>
      </div>

      <p className="mt-1 text-xs text-muted">
        {day.moon_phase} · {day.tide_strength} tide
      </p>

      <div className="mt-3 grid grid-cols-2 gap-2 border-t border-sand pt-2 text-xs">
        <PeriodColumn label="Major" periods={day.major_periods} />
        <PeriodColumn label="Minor" periods={day.minor_periods} />
      </div>

      {!day.weather_included && <MoonAndTideOnlyNote className="mt-2.5" />}
    </div>
  );
}
