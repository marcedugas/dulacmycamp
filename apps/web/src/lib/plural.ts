/**
 * "1 adult", "2 adults" — a count and the right form of its noun.
 *
 * The plural defaults to the singular plus "s", which covers every noun this
 * app counts (adult, kid, night, photo, guest). Pass `pluralForm` explicitly
 * for anything irregular rather than teaching this function English.
 *
 * The API has its own copy of this problem in `bookings::guest_count_phrase`,
 * which builds the same phrasing for emails and inbox notifications. The two
 * are deliberately separate — that one composes a whole sentence ("2 adults
 * and 1 kid") for prose, while this one is a fragment the UI lays out itself.
 */
export function plural(n: number, singular: string, pluralForm = `${singular}s`): string {
  return `${n} ${n === 1 ? singular : pluralForm}`;
}
