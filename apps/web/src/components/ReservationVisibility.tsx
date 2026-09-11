import { Eye, EyeOff } from 'lucide-react';
import { cx } from './ui';

/**
 * The one explanation of what the privacy toggle does, shared verbatim by the
 * booking form, the admin's manual-entry form, and `/my-bookings`. Someone who
 * reads it while booking and reads it again while changing their mind should
 * not have to work out whether the two say the same thing.
 *
 * The first sentence is the one that matters: the common fear about a switch
 * like this is that going public gives away the camp's schedule, and the
 * answer is that availability was never the part being hidden.
 */
export const VISIBILITY_NOTE =
  'Dates are always shown as unavailable to everyone either way. This only controls ' +
  'whether your name and the number of people in your party are visible to other ' +
  'registered family members browsing the calendar.';

/** The same note, for an admin entering a booking on someone else's behalf. */
export const VISIBILITY_NOTE_FOR_GUEST =
  'Dates are always shown as unavailable to everyone either way. This only controls ' +
  'whether the guest’s name and the number of people in their party are visible to ' +
  'other registered family members browsing the calendar.';

interface Choice {
  value: boolean;
  label: string;
  blurb: string;
  icon: typeof Eye;
}

const CHOICES = (forGuest: boolean): Choice[] => [
  {
    value: true,
    label: 'Private',
    blurb: 'Other registered users just see these dates as unavailable, nothing else.',
    icon: EyeOff,
  },
  {
    value: false,
    label: 'Public',
    blurb: forGuest
      ? 'Other registered users can see the guest’s name and party size alongside these dates.'
      : 'Other registered users can see your name and party size alongside these dates.',
    icon: Eye,
  },
];

/**
 * The deliberate choice, as its own block rather than another line in a field
 * list. Radios and not a checkbox on purpose: a checkbox has a state you reach
 * by doing nothing, and this decision should not be reachable by doing
 * nothing. Private is preselected, so the safe answer is still the one you get
 * by not engaging — but you cannot submit without having looked at a block
 * that says, in a heading, that visibility is a thing you are deciding.
 */
export function ReservationVisibility({
  value,
  onChange,
  forGuest = false,
  name = 'reservation-visibility',
}: {
  /** `is_private` — true keeps the booker's identity off other calendars. */
  value: boolean;
  onChange: (isPrivate: boolean) => void;
  /** Wording for an admin booking on a guest's behalf. */
  forGuest?: boolean;
  /** Radio group name, so two of these on one page can't share a selection. */
  name?: string;
}) {
  return (
    <fieldset className="rounded-xl border border-sand bg-cream-dark/50 p-4">
      <legend className="px-1.5 text-sm font-bold text-charcoal">Reservation Visibility</legend>

      <div className="mt-1 grid gap-2 sm:grid-cols-2">
        {CHOICES(forGuest).map((choice) => {
          const selected = value === choice.value;
          const Icon = choice.icon;
          return (
            <label
              key={choice.label}
              className={cx(
                'flex cursor-pointer gap-2.5 rounded-lg border bg-white p-3 transition',
                selected
                  ? 'border-forest-600 ring-1 ring-forest-600'
                  : 'border-sand hover:border-forest-400',
              )}
            >
              <input
                type="radio"
                name={name}
                checked={selected}
                onChange={() => onChange(choice.value)}
                className="mt-0.5 h-4 w-4 shrink-0 accent-forest-600"
              />
              <span>
                <span className="flex items-center gap-1.5 text-sm font-semibold text-charcoal">
                  <Icon size={14} className={selected ? 'text-forest-600' : 'text-muted'} />
                  {choice.label}
                </span>
                <span className="mt-0.5 block text-xs leading-snug text-muted">{choice.blurb}</span>
              </span>
            </label>
          );
        })}
      </div>

      <p className="mt-3 text-xs leading-relaxed text-muted">
        {forGuest ? VISIBILITY_NOTE_FOR_GUEST : VISIBILITY_NOTE}
      </p>
    </fieldset>
  );
}
