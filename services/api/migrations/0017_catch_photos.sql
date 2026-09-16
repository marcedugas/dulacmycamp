-- A photo can now belong to one specific catch rather than to the entry as
-- a whole: "here is the redfish", not "here are some pictures from the trip".
--
-- NULL is the existing behaviour, untouched — a general entry photo, shown
-- in the entry's gallery. Set means the photo belongs to that catch row and
-- is shown with it instead; see `journal::hydrate`, which partitions the two
-- kinds apart so neither ever renders twice.
--
-- ON DELETE CASCADE because a catch photo has no meaning once the catch it
-- documents is gone. `journal_entry_id` stays NOT NULL alongside it, so the
-- entry-level cascade and the entry hard-delete's file sweep keep working
-- exactly as before for both kinds.
ALTER TABLE journal_photos
    ADD COLUMN journal_catch_id uuid REFERENCES journal_catches(id) ON DELETE CASCADE;

-- The gallery query filters on this column ("entry photos" = IS NULL) and
-- the catch hydration groups by it, so both sides of the partition are
-- served rather than only one.
CREATE INDEX idx_journal_photos_catch ON journal_photos (journal_catch_id);
