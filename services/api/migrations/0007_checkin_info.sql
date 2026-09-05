-- Admin-editable arrival details (key location, wifi code, water heater
-- instructions, etc.). Content only — same list-item CRUD shape as
-- checklist_items/rules_items/amenities_items. Surfaced two places: baked
-- live into the booking-confirmed email at send time, and on the guest-facing
-- /my-stay page for as long as the guest holds an approved, not-yet-checked-
-- out booking (see checkin_info::list_for_guest — access is derived from
-- booking state, never a separate grant/revoke step).

CREATE TABLE checkin_info_items (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    title      text NOT NULL,
    body       text NOT NULL,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_checkin_info_items_sort ON checkin_info_items (sort_order);

CREATE TRIGGER checkin_info_items_touch_updated_at
    BEFORE UPDATE ON checkin_info_items
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Seed a few reasonable placeholders; the admin fills in real details
-- through the Check-In Info admin tab once this ships.
INSERT INTO checkin_info_items (title, body, sort_order) VALUES
    ('Key Location', 'TODO(content): where guests find the key', 1),
    ('WiFi', 'TODO(content): network name and password', 2),
    ('Water Heater', 'TODO(content): how to turn it on if needed', 3),
    ('Trash', 'TODO(content): pickup day and where bins go', 4);
