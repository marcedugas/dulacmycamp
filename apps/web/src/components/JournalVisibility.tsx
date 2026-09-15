import { Home, Users } from 'lucide-react';
import { cx } from './ui';
import type { JournalVisibility as Visibility } from '../lib/types';

/**
 * The one explanation of what a journal entry's visibility does, so the
 * wording is identical wherever the choice is offered — writing a story,
 * editing it later, or an admin changing it on someone's behalf.
 *
 * The sentence that matters is the last one: "public" on a site that already
 * requires a login does not mean the open internet, and someone deciding
 * between these two should not have to guess that.
 */
export const JOURNAL_VISIBILITY_NOTE =
  'Either way you can change your mind later, and nobody outside the camp’s ' +
  'registered accounts can read it — the journal is behind the login for everyone.';

interface Choice {
  value: Visibility;
  label: string;
  blurb: string;
  icon: typeof Users;
}

const CHOICES: Choice[] = [
  {
    value: 'family',
    label: 'Family',
    blurb: 'Only family accounts (plus the camp owner) can read this one.',
    icon: Home,
  },
  {
    value: 'public',
    label: 'Public',
    blurb: 'Anyone with a registered account can read it, including guests.',
    icon: Users,
  },
];

/**
 * The deliberate choice, as its own block rather than another line in a field
 * list — the same treatment, and for the same reason, as
 * {@link ReservationVisibility}. Radios and not a checkbox: a checkbox has a
 * state you reach by doing nothing, and who gets to read your stay should not
 * be decided by doing nothing. Family is preselected, so the private answer
 * is still the one you get by not engaging.
 */
export function JournalVisibilityChoice({
  value,
  onChange,
  name = 'journal-visibility',
}: {
  value: Visibility;
  onChange: (next: Visibility) => void;
  /** Radio group name, so two of these on one page can't share a selection. */
  name?: string;
}) {
  return (
    <fieldset className="rounded-xl border border-sand bg-cream-dark/50 p-4">
      <legend className="px-1.5 text-sm font-bold text-charcoal">Who can read this?</legend>

      <div className="mt-1 grid gap-2 sm:grid-cols-2">
        {CHOICES.map((choice) => {
          const selected = value === choice.value;
          const Icon = choice.icon;
          return (
            <label
              key={choice.value}
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

      <p className="mt-3 text-xs leading-relaxed text-muted">{JOURNAL_VISIBILITY_NOTE}</p>
    </fieldset>
  );
}
