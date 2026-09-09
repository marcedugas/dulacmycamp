import { useState } from 'react';
import { Info } from 'lucide-react';
import { Modal } from './ui';

/**
 * The plain-English account of how the star rating is actually produced, kept
 * beside the widget that shows it.
 *
 * Everything asserted here is something `services/api/src/solunar.rs` really
 * does — the Meeus moon position, the NOAA tide range against the station's
 * Great Diurnal Range, and the NWS wind/barometer modifiers. If that scoring
 * changes, this copy changes with it.
 */

/** One weighted input to the rating, as the reader meets it. */
function Factor({ emoji, title, children }: { emoji: string; title: string; children: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="mt-0.5 shrink-0 text-xl leading-none" aria-hidden>
        {emoji}
      </span>
      <p className="text-sm leading-relaxed text-charcoal">
        <span className="font-semibold">{title}</span> — {children}
      </p>
    </li>
  );
}

export function ForecastAboutButton() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-label="How this forecast works"
        title="How this forecast works"
        className="rounded-full p-1 text-muted transition hover:bg-cream-dark hover:text-forest-600"
      >
        <Info size={16} />
      </button>

      <Modal open={open} onClose={() => setOpen(false)} title="How This Forecast Works" size="lg">
        <div className="space-y-4">
          <p className="text-sm leading-relaxed text-charcoal">
            This forecast isn&apos;t pulled from a third-party fishing app — it&apos;s calculated
            right here, from real astronomical and weather data, using a method called{' '}
            <strong>Solunar Theory</strong> that anglers have leaned on since the 1920s.
          </p>

          <p className="text-sm font-semibold text-charcoal">
            Three things go into each day&apos;s rating:
          </p>

          <ul className="space-y-3">
            <Factor emoji="🌙" title="Moon position">
              Fish activity tends to peak around the new and full moon. We work out the moon&apos;s
              exact position over Cocodrie, Louisiana ourselves — rise, set and the overhead pass —
              rather than using a rough estimate, and check it against U.S. Naval Observatory
              times. It lands within a few minutes.
            </Factor>
            <Factor emoji="🌊" title="Tide strength">
              Bigger tidal swings mean more water moving, which tends to mean more active feeding.
              We compare each day&apos;s predicted tide range to what&apos;s typical for this
              stretch of water, using the real NOAA tide station down at Cocodrie. A big swing
              lifts the day&apos;s rating; a slack one simply doesn&apos;t get that lift.
            </Factor>
            <Factor emoji="🌡️" title="Weather">
              Wind and barometric pressure move fish too. A falling barometer — a front rolling in
              — tends to fire them up, while a settled high-pressure day tends to slow things down,
              and a hard blow flattens it out. We pull this from the National Weather Service.
            </Factor>
          </ul>

          <p className="text-sm leading-relaxed text-charcoal">
            We also show <strong>major</strong> and <strong>minor</strong> feeding windows for each
            day — specific time ranges built around when the moon is overhead, underfoot, rising or
            setting.
          </p>

          <p className="text-sm leading-relaxed text-charcoal">
            Like any forecast, this is a helpful guide, not a guarantee — the fish didn&apos;t sign
            off on it!
          </p>

          <p className="border-t border-sand pt-3 text-xs leading-relaxed text-muted">
            The barometer reading is a right-now signal, so it only shapes today. Wind comes from
            the week-out weather forecast; past that, a day&apos;s rating rests on the moon and the
            tide alone.
          </p>
        </div>
      </Modal>
    </>
  );
}
