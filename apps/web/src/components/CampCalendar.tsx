import { useMemo } from 'react';
import {
  addDays,
  addMonths,
  addYears,
  endOfMonth,
  endOfWeek,
  format,
  isSameDay,
  isSameMonth,
  startOfMonth,
  startOfWeek,
} from 'date-fns';
import { ChevronLeft, ChevronRight, Flag, TriangleAlert } from 'lucide-react';
import type { BlackoutDate, Booking, Holiday, SpecialEvent } from '../lib/types';
import { daysInclusive, nightsOf, parseDay, toKey } from '../lib/dates';
import { Button, cx } from './ui';

export type CalendarView = 'week' | 'twoweek' | 'month' | 'year';

export const VIEW_LABELS: Record<CalendarView, string> = {
  week: 'Week',
  twoweek: '2 Weeks',
  month: 'Month',
  year: 'Year',
};

/** Everything the grid needs to know about one calendar day. */
export interface DayCell {
  approved: Booking[];
  pending: Booking[];
  blackout: BlackoutDate | null;
  events: SpecialEvent[];
  /** Adults on approved stays that night. */
  adults: number;
  /**
   * Reference-only US holiday marker, if any. Unlike every other field on
   * this cell, this carries zero booking meaning — it never affects
   * availability, capacity, or how the cell's ground color reads.
   */
  holiday: Holiday | null;
}

const EMPTY: DayCell = {
  approved: [],
  pending: [],
  blackout: null,
  events: [],
  adults: 0,
  holiday: null,
};

/**
 * Indexes bookings, blackouts, events and holidays by day key.
 *
 * A stay occupies its nights — check-in through the night before check-out —
 * so the departure day shows as free for the next guest.
 */
export function buildIndex(
  bookings: Booking[],
  blackouts: BlackoutDate[],
  events: SpecialEvent[],
  holidays: Holiday[] = [],
): Map<string, DayCell> {
  const map = new Map<string, DayCell>();
  const at = (key: string): DayCell => {
    let cell = map.get(key);
    if (!cell) {
      cell = { approved: [], pending: [], blackout: null, events: [], adults: 0, holiday: null };
      map.set(key, cell);
    }
    return cell;
  };

  for (const b of bookings) {
    if (b.status !== 'approved' && b.status !== 'pending') continue;
    for (const night of nightsOf(b.check_in, b.check_out)) {
      const cell = at(toKey(night));
      if (b.status === 'approved') {
        cell.approved.push(b);
        cell.adults += b.guest_count_adults;
      } else {
        cell.pending.push(b);
      }
    }
  }

  for (const bo of blackouts) {
    for (const d of daysInclusive(parseDay(bo.start_date), parseDay(bo.end_date))) {
      at(toKey(d)).blackout = bo;
    }
  }

  for (const ev of events) {
    const end = ev.end_date ?? ev.event_date;
    for (const d of daysInclusive(parseDay(ev.event_date), parseDay(end))) {
      at(toKey(d)).events.push(ev);
    }
  }

  for (const h of holidays) {
    at(h.date).holiday = h;
  }

  return map;
}

/** The days each view renders, always aligned to whole weeks. */
export function visibleDays(view: CalendarView, anchor: Date): Date[] {
  switch (view) {
    case 'week':
      return daysInclusive(startOfWeek(anchor), endOfWeek(anchor));
    case 'twoweek':
      return daysInclusive(startOfWeek(anchor), endOfWeek(addDays(anchor, 7)));
    case 'month':
    case 'year':
      return daysInclusive(startOfWeek(startOfMonth(anchor)), endOfWeek(endOfMonth(anchor)));
  }
}

export function stepAnchor(view: CalendarView, anchor: Date, direction: 1 | -1): Date {
  switch (view) {
    case 'week':
      return addDays(anchor, 7 * direction);
    case 'twoweek':
      return addDays(anchor, 14 * direction);
    case 'month':
      return addMonths(anchor, direction);
    case 'year':
      return addYears(anchor, direction);
  }
}

function periodLabel(view: CalendarView, anchor: Date): string {
  if (view === 'year') return format(anchor, 'yyyy');
  if (view === 'month') return format(anchor, 'MMMM yyyy');
  const days = visibleDays(view, anchor);
  const first = days[0];
  const last = days[days.length - 1];
  return isSameMonth(first, last)
    ? `${format(first, 'MMM d')} – ${format(last, 'd, yyyy')}`
    : `${format(first, 'MMM d')} – ${format(last, 'MMM d, yyyy')}`;
}

const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

/**
 * The stays on a day whose booker chose to be seen.
 *
 * `guest_first_name` is the entire test, and the server is what decides it —
 * it is sent only to a signed-in viewer, only for an approved stay, and only
 * when the booker set the reservation to public (`bookings::shows_booker`).
 * Nothing here re-derives that rule from `is_private` or the viewer's role:
 * the client has never been the thing that decides who may be seen, and a
 * second copy of the rule here is exactly how the two would drift apart.
 */
function namedStays(bookings: Booking[]): Booking[] {
  return bookings.filter((b) => Boolean(b.guest_first_name));
}

/** "Jean · 3" — party size is adults plus kids, one number, since a cell has
 *  room for a number and not a sentence. The breakdown is in the tooltip. */
function partySize(b: Booking): number {
  return b.guest_count_adults + b.guest_count_kids;
}

function partyBreakdown(b: Booking): string {
  const kids = b.guest_count_kids;
  return `${b.guest_first_name} — ${b.guest_count_adults} adult${
    b.guest_count_adults === 1 ? '' : 's'
  }${kids > 0 ? `, ${kids} kid${kids === 1 ? '' : 's'}` : ''}`;
}

// ─────────────────────────── day cell ───────────────────────────

interface DayProps {
  date: Date;
  cell: DayCell;
  dimmed: boolean;
  selected: boolean;
  isRangeEdge: boolean;
  capacityLimit: number;
  onClick?: (key: string) => void;
}

function Day({ date, cell, dimmed, selected, isRangeEdge, capacityLimit, onClick }: DayProps) {
  const key = toKey(date);
  const today = isSameDay(date, new Date());
  const over = cell.adults > capacityLimit;
  const doubleBooked = cell.approved.length + cell.pending.length > 1;
  const clickable = Boolean(onClick);
  const named = namedStays(cell.approved);

  return (
    <button
      type="button"
      disabled={!clickable}
      onClick={() => onClick?.(key)}
      aria-label={`${format(date, 'EEEE, MMMM d, yyyy')}${
        cell.blackout ? ', unavailable' : cell.approved.length ? ', booked' : ', available'
      }${named.map((b) => `, ${b.guest_first_name}, party of ${partySize(b)}`).join('')}${
        cell.holiday ? `, ${cell.holiday.name}` : ''
      }`}
      aria-pressed={selected}
      className={cx(
        'relative flex min-h-[84px] flex-col items-stretch gap-1 rounded-lg border p-1.5 text-left transition',
        cell.blackout ? 'stripe-blackout border-sand bg-cream-dark' : 'border-sand bg-white',
        dimmed && 'opacity-45',
        clickable && 'hover:border-forest-400 cursor-pointer',
        selected && 'ring-2 ring-forest-500 ring-offset-1',
        isRangeEdge && 'border-forest-600',
      )}
    >
      <div className="flex items-start justify-between">
        <span className="flex items-center gap-1">
          <span
            className={cx(
              'text-xs font-bold',
              today
                ? 'grid h-5 w-5 place-items-center rounded-full bg-forest-600 text-cream'
                : 'text-charcoal/70',
            )}
          >
            {format(date, 'd')}
          </span>
          {/* Reference-only: a small outline flag, never a colored block —
              nothing here should read as a booking-status signal. */}
          {cell.holiday && (
            <span title={cell.holiday.name} aria-hidden="true" className="text-wood-500">
              <Flag size={10} strokeWidth={2.5} />
            </span>
          )}
        </span>
        <span className="flex items-center gap-0.5">
          {cell.events.map((ev) => (
            <span key={ev.id} title={`${ev.name}${ev.description ? ` — ${ev.description}` : ''}`}>
              {ev.emoji ?? '🎉'}
            </span>
          ))}
          {(over || doubleBooked) && (
            <TriangleAlert
              size={12}
              className="text-clay"
              aria-label={over ? 'Over capacity' : 'Overlapping bookings'}
            />
          )}
        </span>
      </div>

      {/* Reference-only label — plain muted italic text, deliberately not a
          colored pill like the status badges below, so it can never be
          misread as availability. Always visible, so a tap needs no
          interaction to reveal the name. */}
      {cell.holiday && (
        <span className="truncate text-[9px] italic text-wood-600" title={cell.holiday.name}>
          {cell.holiday.name}
        </span>
      )}

      {cell.blackout && (
        <span className="rounded bg-charcoal/75 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wide text-cream">
          Blackout
        </span>
      )}
      {cell.approved.length > 0 && (
        <span className="rounded bg-forest-600 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wide text-cream">
          Booked{cell.approved.length > 1 ? ` ×${cell.approved.length}` : ''}
        </span>
      )}
      {cell.pending.length > 0 && (
        <span className="rounded border border-amber-300 bg-amber-100 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wide text-amber-900">
          Pending{cell.pending.length > 1 ? ` ×${cell.pending.length}` : ''}
        </span>
      )}

      {/* Who's coming, for the stays whose booker opted into being seen. It
          sits under the Booked badge rather than replacing it: the badge is
          the availability answer and stays identical either way, and this is
          only ever an addition to it. */}
      {named.map((b) => (
        <span
          key={b.id}
          title={partyBreakdown(b)}
          className="truncate text-[10px] font-semibold text-forest-700"
        >
          {b.guest_first_name} · {partySize(b)}
        </span>
      ))}
    </button>
  );
}

// ─────────────────────────── year view ───────────────────────────

function YearGrid({
  year,
  index,
  onPickMonth,
}: {
  year: number;
  index: Map<string, DayCell>;
  onPickMonth: (d: Date) => void;
}) {
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      {Array.from({ length: 12 }, (_, m) => {
        const first = new Date(year, m, 1);
        const days = daysInclusive(startOfWeek(startOfMonth(first)), endOfWeek(endOfMonth(first)));
        return (
          <div key={m} className="rounded-lg border border-sand bg-white p-2.5">
            <button
              onClick={() => onPickMonth(first)}
              className="mb-1.5 w-full text-left text-sm font-bold text-charcoal hover:text-forest-600"
            >
              {format(first, 'MMMM')}
            </button>
            <div className="grid grid-cols-7 gap-0.5">
              {WEEKDAYS.map((d) => (
                <span key={d} className="text-center text-[9px] font-semibold text-muted">
                  {d[0]}
                </span>
              ))}
              {days.map((d) => {
                const cell = index.get(toKey(d)) ?? EMPTY;
                const outside = !isSameMonth(d, first);
                const tone = cell.blackout
                  ? 'bg-sand text-charcoal/60'
                  : cell.approved.length
                    ? 'bg-forest-600 text-cream'
                    : cell.pending.length
                      ? 'bg-amber-200 text-amber-900'
                      : 'text-charcoal/70';
                return (
                  <span
                    key={toKey(d)}
                    // The year view's cells are five-pixel squares, so the
                    // names live in the tooltip rather than on the grid.
                    title={[
                      format(d, 'MMM d, yyyy'),
                      cell.holiday?.name,
                      ...namedStays(cell.approved).map(partyBreakdown),
                    ]
                      .filter(Boolean)
                      .join(' — ')}
                    className={cx(
                      'relative grid h-5 place-items-center rounded text-[10px] font-medium',
                      tone,
                      outside && 'opacity-25',
                    )}
                  >
                    {format(d, 'd')}
                    {/* Corner dot, opposite the event dot and a different hue
                        (wood, not bayou) — reference-only, never a booking
                        signal. */}
                    {cell.holiday && (
                      <span className="absolute -left-px -top-px h-1.5 w-1.5 rounded-full bg-wood-500" />
                    )}
                    {cell.events.length > 0 && (
                      <span className="absolute -right-px -top-px h-1.5 w-1.5 rounded-full bg-bayou-500" />
                    )}
                  </span>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ─────────────────────────── legend ───────────────────────────

export function Legend() {
  const items = [
    { label: 'Available', className: 'bg-white border-sand' },
    { label: 'Booked', className: 'bg-forest-600 border-forest-600' },
    { label: 'Pending', className: 'bg-amber-100 border-amber-300' },
    { label: 'Blackout', className: 'stripe-blackout bg-cream-dark border-sand' },
    { label: 'Special event', className: 'bg-bayou-500 border-bayou-500' },
  ];
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted">
      {items.map((i) => (
        <span key={i.label} className="flex items-center gap-1.5">
          <span className={cx('inline-block h-3 w-3 rounded border', i.className)} />
          {i.label}
        </span>
      ))}
      {/* Rendered as the same flag glyph the grid uses, not a color swatch —
          the legend should teach "look for the icon", not "look for a
          color", since holidays carry no availability meaning. */}
      <span className="flex items-center gap-1.5">
        <Flag size={12} className="text-wood-500" strokeWidth={2.5} /> Holiday (reference only)
      </span>
      <span className="flex items-center gap-1.5">
        <TriangleAlert size={12} className="text-clay" /> Overlap / over capacity
      </span>
    </div>
  );
}

// ─────────────────────────── calendar ───────────────────────────

interface Props {
  bookings: Booking[];
  blackouts: BlackoutDate[];
  events: SpecialEvent[];
  /** Reference-only US holiday markers — see [[useHolidays]]. */
  holidays?: Holiday[];
  capacityLimit: number;
  view: CalendarView;
  onViewChange: (v: CalendarView) => void;
  anchor: Date;
  onAnchorChange: (d: Date) => void;
  /** Optional `YYYY-MM-DD` range highlight. */
  selection?: { start?: string; end?: string };
  onDayClick?: (key: string) => void;
}

export default function CampCalendar({
  bookings,
  blackouts,
  events,
  holidays = [],
  capacityLimit,
  view,
  onViewChange,
  anchor,
  onAnchorChange,
  selection,
  onDayClick,
}: Props) {
  const index = useMemo(
    () => buildIndex(bookings, blackouts, events, holidays),
    [bookings, blackouts, events, holidays],
  );
  const days = useMemo(() => visibleDays(view, anchor), [view, anchor]);

  // A selection covers the nights of the stay: check-in through the night
  // before check-out. A start with no end yet highlights just that one day.
  const inSelection = (key: string) => {
    if (!selection?.start) return false;
    if (!selection.end) return key === selection.start;
    const [lo, hi] =
      selection.start <= selection.end
        ? [selection.start, selection.end]
        : [selection.end, selection.start];
    return key >= lo && key < hi;
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-1">
          <button
            onClick={() => onAnchorChange(stepAnchor(view, anchor, -1))}
            aria-label="Previous"
            className="rounded-lg border border-sand bg-white p-2 hover:bg-cream-dark"
          >
            <ChevronLeft size={16} />
          </button>
          <button
            onClick={() => onAnchorChange(stepAnchor(view, anchor, 1))}
            aria-label="Next"
            className="rounded-lg border border-sand bg-white p-2 hover:bg-cream-dark"
          >
            <ChevronRight size={16} />
          </button>
          <Button variant="ghost" size="sm" className="ml-1" onClick={() => onAnchorChange(new Date())}>
            Today
          </Button>
          <h3 className="ml-2 text-lg font-bold text-charcoal">{periodLabel(view, anchor)}</h3>
        </div>

        <div className="flex rounded-lg border border-sand bg-white p-0.5">
          {(Object.keys(VIEW_LABELS) as CalendarView[]).map((v) => (
            <button
              key={v}
              onClick={() => onViewChange(v)}
              aria-pressed={view === v}
              className={cx(
                'rounded-md px-3 py-1.5 text-xs font-semibold transition',
                view === v ? 'bg-forest-600 text-cream' : 'text-muted hover:text-charcoal',
              )}
            >
              {VIEW_LABELS[v]}
            </button>
          ))}
        </div>
      </div>

      {view === 'year' ? (
        <YearGrid
          year={anchor.getFullYear()}
          index={index}
          onPickMonth={(d) => {
            onAnchorChange(d);
            onViewChange('month');
          }}
        />
      ) : (
        <div>
          <div className="mb-1 grid grid-cols-7 gap-1.5">
            {WEEKDAYS.map((d) => (
              <span key={d} className="px-1 text-xs font-bold uppercase tracking-wide text-muted">
                {d}
              </span>
            ))}
          </div>
          <div className="grid grid-cols-7 gap-1.5">
            {days.map((d) => {
              const key = toKey(d);
              return (
                <Day
                  key={key}
                  date={d}
                  cell={index.get(key) ?? EMPTY}
                  dimmed={view === 'month' && !isSameMonth(d, anchor)}
                  selected={inSelection(key)}
                  isRangeEdge={key === selection?.start || key === selection?.end}
                  capacityLimit={capacityLimit}
                  onClick={onDayClick}
                />
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
