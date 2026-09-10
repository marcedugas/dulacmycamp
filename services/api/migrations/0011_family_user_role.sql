-- The "user" (family member) role, and an admin-editable map of which roles
-- may see which gated sections.
--
-- Until now the only way past the public site was booking history: you needed
-- an approved stay to see anything else. That makes the site a booking tool,
-- but family who simply want to look at photos have no way in. "user" is that
-- tier — a family account that needs no booking, ever, to exist or to keep
-- its access. Self-registration still lands people as 'guest'; only an admin
-- promotes.

ALTER TABLE users DROP CONSTRAINT users_role_valid;
ALTER TABLE users
    ADD CONSTRAINT users_role_valid CHECK (role IN ('guest', 'user', 'admin'));

-- One row per gated section. Access is data, not code, so opening the album
-- to family is a toggle rather than a deploy.
--
-- Two independent ways in, per section:
--   allowed_roles           — anyone holding one of these roles, full stop
--   approved_booking_grants — anyone with an ever-approved booking, any role
-- A section grants access if EITHER matches; both empty/false means nobody.
CREATE TABLE content_access (
    section_key             text PRIMARY KEY,
    label                   text NOT NULL,
    description             text NOT NULL,
    allowed_roles           text[] NOT NULL DEFAULT '{}',
    approved_booking_grants boolean NOT NULL DEFAULT false,
    -- False for sections whose rule is deliberately NOT role-driven. The
    -- admin panel lists them so the picture is complete, but refuses to edit
    -- them: their real rule lives in code and would silently regress if a
    -- toggle here looked authoritative. See `content_access::update_admin`.
    configurable            boolean NOT NULL DEFAULT true,
    sort_order              integer NOT NULL DEFAULT 0,
    updated_at              timestamptz NOT NULL DEFAULT now()
);

INSERT INTO content_access
    (section_key, label, description, allowed_roles, approved_booking_grants, configurable, sort_order)
VALUES
    (
        'guest_photos',
        'Camp photo album',
        'The shared Google Photos album link, shown on My Stay.',
        '{user,admin}',
        true,
        true,
        1
    ),
    (
        'journal_entry',
        'Writing a journal entry',
        'Tied to having actually stayed: an approved booking whose check-in date has passed, and one entry per stay. Not role-driven — a family member with no bookings cannot write about a stay they did not take. Reading the journal is public to everyone.',
        '{}',
        false,
        false,
        2
    ),
    (
        'checkin_info',
        'Check-in info',
        'The key location, wifi code, and other arrival details. Tied to holding an approved booking that has not been checked out yet — operational access that ends when the stay does. Not role-driven.',
        '{}',
        false,
        false,
        3
    );

CREATE TRIGGER content_access_touch_updated_at
    BEFORE UPDATE ON content_access
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
