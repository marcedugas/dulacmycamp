-- Guest checkout checklist and camp journal. The checklist replaces the
-- physical paper sign at the camp; completing checkout is what unlocks the
-- journal prompt for that stay — there is no separate scheduled job, the
-- chaining is inline in the checkout response (see services/api/src/checkout.rs).

-- Admin-managed checklist content.
CREATE TABLE checklist_items (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    label      text NOT NULL,
    sort_order integer NOT NULL DEFAULT 0,
    -- Inactive items are hidden from new checkouts but preserved so
    -- historical completed-checkout records still resolve correctly.
    active     boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- One record per completed checkout.
CREATE TABLE booking_checkouts (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    booking_id        uuid NOT NULL UNIQUE REFERENCES bookings(id),
    user_id           uuid NOT NULL REFERENCES users(id),
    -- Array of checklist_items.id (as strings) that were checked at
    -- completion time. Honor-system checklist — unchecked items are simply
    -- absent, not recorded as false.
    checked_item_ids  jsonb NOT NULL DEFAULT '[]',
    -- Optional "anything to flag?" note — private to the admin, distinct
    -- from the public journal entry a guest may separately choose to write.
    notes             text,
    completed_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_booking_checkouts_user ON booking_checkouts (user_id);

-- Seed a reasonable default checklist; the admin can edit/reorder later.
INSERT INTO checklist_items (label, sort_order) VALUES
    ('Turn off all lights', 1),
    ('Turn off the AC/heat or set to away mode', 2),
    ('Lock all doors and windows', 3),
    ('Take trash out / to the road', 4),
    ('Make sure the boat is properly secured', 5),
    ('Clean up the kitchen and put away dishes', 6),
    ('Report any issues or damage', 7);

CREATE INDEX idx_checklist_items_sort ON checklist_items (sort_order);

-- Guest stories, fishing reports, reflections — free-form, no ratings.
-- One entry per completed stay: a guest journals about each stay
-- separately, but only once per stay.
CREATE TABLE journal_entries (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid NOT NULL REFERENCES users(id),
    booking_id      uuid NOT NULL UNIQUE REFERENCES bookings(id),
    title           text NOT NULL,
    body            text NOT NULL,
    status          text NOT NULL DEFAULT 'pending',
    rejected_reason text,
    approved_at     timestamptz,
    -- 'admin' or the admin's email — mirrors bookings.approved_by.
    approved_by     text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT journal_entries_status_valid
        CHECK (status IN ('pending', 'approved', 'rejected'))
);

CREATE INDEX idx_journal_entries_status ON journal_entries (status, created_at DESC);
CREATE INDEX idx_journal_entries_user ON journal_entries (user_id);

CREATE TRIGGER checklist_items_touch_updated_at
    BEFORE UPDATE ON checklist_items
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TRIGGER journal_entries_touch_updated_at
    BEFORE UPDATE ON journal_entries
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
