import { addDays, differenceInCalendarDays, format, parseISO } from 'date-fns';

/**
 * Parses a `YYYY-MM-DD` API date into local midnight.
 *
 * Everything on the calendar is a *calendar* date, not an instant: the camp is
 * booked on the 4th regardless of the viewer's timezone. `parseISO` on a
 * date-only string gives local midnight, which is exactly that.
 */
export function parseDay(iso: string): Date {
  return parseISO(iso.slice(0, 10));
}

/** Formats a Date back into the API's `YYYY-MM-DD`. */
export function toKey(d: Date): string {
  return format(d, 'yyyy-MM-dd');
}

/** Every calendar day from `start` to `end`, both ends included. */
export function daysInclusive(start: Date, end: Date): Date[] {
  const out: Date[] = [];
  for (let d = start; differenceInCalendarDays(d, end) <= 0; d = addDays(d, 1)) out.push(d);
  return out;
}

/**
 * The nights a stay occupies: check-in through the night before check-out.
 * The departure day itself is free for the next guest.
 */
export function nightsOf(checkIn: string, checkOut: string): Date[] {
  const start = parseDay(checkIn);
  const end = addDays(parseDay(checkOut), -1);
  return differenceInCalendarDays(end, start) < 0 ? [] : daysInclusive(start, end);
}

export function nightCount(checkIn: string, checkOut: string): number {
  return differenceInCalendarDays(parseDay(checkOut), parseDay(checkIn));
}

/** "Sep 4 – 7, 2026" or "Dec 30, 2026 – Jan 2, 2027". */
export function formatRange(checkIn: string, checkOut: string): string {
  const a = parseDay(checkIn);
  const b = parseDay(checkOut);
  if (a.getFullYear() !== b.getFullYear()) {
    return `${format(a, 'MMM d, yyyy')} – ${format(b, 'MMM d, yyyy')}`;
  }
  if (a.getMonth() !== b.getMonth()) {
    return `${format(a, 'MMM d')} – ${format(b, 'MMM d, yyyy')}`;
  }
  return `${format(a, 'MMM d')} – ${format(b, 'd, yyyy')}`;
}

export function pluralNights(n: number): string {
  return `${n} ${n === 1 ? 'night' : 'nights'}`;
}

/** A solunar period's "14:30" (local, from the API) as "2:30 PM". */
export function formatPeriodTime(hm: string): string {
  const [h, m] = hm.split(':').map(Number);
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return format(d, 'h:mm a');
}
