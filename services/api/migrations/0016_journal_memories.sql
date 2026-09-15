-- The camp journal becomes a per-stay memory log: catches, photos, and a
-- visibility choice the author makes for themselves.
--
-- The admin pre-publish approval queue is retired outright. An entry is live
-- the moment it is written, scoped by its own `visibility` and `archived_at`;
-- admin moderation becomes after-the-fact (edit / archive / delete) rather
-- than a gate every story has to queue behind.

-- Who may read an entry. Deliberately distinct from `bookings.is_private`,
-- which answers a different question ("hidden from everyone but admin and the
-- owner"). This one is "family only, or any registered account" — there is no
-- longer an anonymous-internet tier at all, since the feed itself now requires
-- a login.
--
-- DEFAULT 'family' is the more private of the two, matching the reservation
-- privacy toggle's philosophy. It applies to existing rows as well, so stories
-- that were previously readable by anonymous visitors land on the private side
-- of the new line rather than the permissive one. Authors can flip their own
-- entry to 'public' whenever they like; nothing is lost, only narrowed.
ALTER TABLE journal_entries
    ADD COLUMN visibility text NOT NULL DEFAULT 'family';
ALTER TABLE journal_entries
    ADD CONSTRAINT journal_entries_visibility_valid
    CHECK (visibility IN ('public', 'family'));

-- A 'rejected' row carries a real admin decision: someone looked at it and
-- said no. Removing the gate must not quietly undo that, so those rows are
-- archived — the same lever an admin would reach for today to hide something.
-- Rows already archived keep their original timestamp; this only fills gaps.
UPDATE journal_entries
SET archived_at = now()
WHERE status = 'rejected' AND archived_at IS NULL;

-- 'pending' rows needed no special handling: nothing ever decided them, and
-- with the queue gone they simply become ordinary entries, visible per their
-- (now 'family') visibility like anything written from here on.

-- With self-publish there is no approval to record, so the whole approval
-- vocabulary goes rather than lingering as columns pinned to a fixed value —
-- a `status` frozen at 'approved' would claim a decision nobody made, and
-- `approved_by` / `rejected_reason` would describe a workflow that no longer
-- exists. `created_at` is now the one authored-at/posted-at timestamp, which
-- is what the feed orders by.
DROP INDEX idx_journal_entries_status;
ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_status_valid;
ALTER TABLE journal_entries DROP COLUMN status;
ALTER TABLE journal_entries DROP COLUMN rejected_reason;
ALTER TABLE journal_entries DROP COLUMN approved_at;
ALTER TABLE journal_entries DROP COLUMN approved_by;

CREATE INDEX idx_journal_entries_visibility
    ON journal_entries (visibility, created_at DESC);

-- Admin-editable species list, same shape and lifecycle as checklist_items:
-- ordered, renameable, and deactivated rather than deleted once something
-- historical points at it (see `fish_species::remove`).
CREATE TABLE fish_species (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name       text NOT NULL,
    sort_order integer NOT NULL DEFAULT 0,
    active     boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- A starting point for the Cocodrie/Dulac estuary, not an exhaustive list —
-- the admin panel is where this actually gets tuned.
INSERT INTO fish_species (name, sort_order) VALUES
    ('Redfish (Red Drum)', 1),
    ('Speckled Trout', 2),
    ('Black Drum', 3),
    ('Flounder', 4),
    ('Sheepshead', 5),
    ('Tripletail', 6),
    ('Catfish', 7),
    ('Largemouth Bass', 8),
    ('Bream / Bluegill', 9),
    ('Blue Crab', 10),
    ('Shrimp', 11),
    ('Other', 12);

CREATE INDEX idx_fish_species_sort ON fish_species (sort_order);

CREATE TRIGGER fish_species_touch_updated_at
    BEFORE UPDATE ON fish_species
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- The catch log: repeatable rows hanging off one entry.
--
-- `species_id` is nullable and deliberately NOT cascading: deactivating or
-- losing a species must never delete somebody's record of what they caught.
-- Every measurement is optional too — plenty of real entries are "we caught
-- a mess of trout" with no tape measure involved.
--
-- `double precision` rather than `numeric` for the measurements: a fish on a
-- tape measure is an approximate physical quantity, not money, so exact
-- decimal arithmetic buys nothing here — and `numeric` would mean pulling in
-- a decimal crate for sqlx to decode it, which is a real dependency to carry
-- for a field nobody sums or compares for equality.
CREATE TABLE journal_catches (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    journal_entry_id uuid NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    species_id       uuid REFERENCES fish_species(id),
    length_inches    double precision,
    weight_lbs       double precision,
    quantity         integer NOT NULL DEFAULT 1,
    notes            text,
    sort_order       integer NOT NULL DEFAULT 0
);

CREATE INDEX idx_journal_catches_entry ON journal_catches (journal_entry_id);

-- Photos, mirroring `gallery_photos` column-for-column (url + optional
-- caption + ordering) so both kinds of upload read and write the same way —
-- see `crate::uploads`, which both share.
CREATE TABLE journal_photos (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    journal_entry_id uuid NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    url              text NOT NULL,
    caption          text,
    sort_order       integer NOT NULL DEFAULT 0,
    created_at       timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_journal_photos_entry ON journal_photos (journal_entry_id);
