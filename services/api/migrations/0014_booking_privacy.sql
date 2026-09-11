-- Whether a booking's booker is visible to other signed-in family members.
--
-- DEFAULT true is the whole point of the column, in both directions. Every
-- row already in the table was created when the calendar promised names were
-- never shown, so backfilling them to private is the only honest reading of
-- the consent that was actually given — nobody's stay becomes visible because
-- a column appeared. And NOT NULL DEFAULT true means a booking inserted by
-- older code in flight, or by any path that forgets the field, lands private
-- too: the safe answer is the one you get by doing nothing.
--
-- This only ever gates *identity*. Availability is unchanged: a private
-- booking still blocks its nights for everyone, signed in or not. See
-- `bookings::BookingRow::to_view`, which additionally withholds the name from
-- anonymous visitors entirely and until the stay is actually approved.
ALTER TABLE bookings
    ADD COLUMN is_private boolean NOT NULL DEFAULT true;
