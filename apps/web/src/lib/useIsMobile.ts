import { useSyncExternalStore } from 'react';

/**
 * The width Tailwind's `sm:` turns on at, and so the line this app already
 * draws between phone and everything else.
 *
 * Taken from Tailwind's `--breakpoint-sm` (40rem), which this project does not
 * override — `@theme` in `index.css` sets colors and fonts only. `sm:` is also
 * the prefix the components actually reach for, by a wide margin, so it is the
 * breakpoint that decides what "mobile" looks like here in practice.
 *
 * In rem rather than pixels on purpose: that is how Tailwind expresses it, so
 * a reader who bumps the browser's base font size moves this line and the CSS
 * one together instead of drifting them apart.
 */
export const SM_BREAKPOINT = '40rem';

/**
 * Deliberately the *desktop* query, negated at the point of use.
 *
 * `sm:` styles apply at `min-width: 40rem`, so "mobile" is exactly the
 * complement of that. Asking the same question CSS asks and inverting the
 * answer means there is no boundary to get wrong: a hand-rolled
 * `max-width: 39.9375rem` has to guess at the gap between the two, and guesses
 * wrong on fractional device pixels, where a viewport can satisfy neither.
 */
const DESKTOP_QUERY = `(min-width: ${SM_BREAKPOINT})`;

/**
 * One `MediaQueryList` for the whole app, created on first use.
 *
 * Lazy because a module-level `window.matchMedia` would run at import time,
 * which is the one moment a test runner or a prerender might not have a
 * `window` yet.
 */
let mediaQuery: MediaQueryList | undefined;
const media = (): MediaQueryList => (mediaQuery ??= window.matchMedia(DESKTOP_QUERY));

function subscribe(onStoreChange: () => void): () => void {
  const mql = media();
  mql.addEventListener('change', onStoreChange);
  return () => mql.removeEventListener('change', onStoreChange);
}

function getSnapshot(): boolean {
  return !media().matches;
}

/**
 * Whether the app is currently rendering at a phone-sized width.
 *
 * Reactive: a rotation, a window drag, or a devtools viewport change that
 * crosses [[SM_BREAKPOINT]] re-renders the caller. That is the whole reason
 * this is a hook and not a function — a one-time check on mount is right until
 * the moment somebody turns their phone sideways.
 *
 * A media query and not the user-agent string: UA sniffing answers "what kind
 * of device is this", which is not the question. A narrow window on a desktop
 * needs the phone layout, a tablet in landscape does not, and neither fact is
 * in the UA. It also cannot be faked by devtools' own responsive mode, so it
 * would make the layout untestable without a real device.
 *
 * `useSyncExternalStore` rather than `useState` + `useEffect`: the viewport is
 * an external store, and this is what React provides for reading one. It also
 * means the first render already has the real answer, instead of rendering the
 * desktop layout and correcting itself a frame later.
 *
 * Nothing consumes this yet — it exists so mobile-specific work has one place
 * to ask, rather than each component growing its own listener.
 */
export function useIsMobile(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot);
}
