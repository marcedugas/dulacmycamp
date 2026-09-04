-- Move "who receives the booking approval email" out of the OWNER_EMAIL env
-- var and into the users table.
--
-- OWNER_EMAIL survives as a bootstrap fallback for a fresh database where
-- nobody has been flagged yet, but it is no longer the source of truth: the
-- recipient list is now editable from the admin panel and may hold more than
-- one address.

ALTER TABLE users ADD COLUMN is_owner boolean NOT NULL DEFAULT false;

-- ── Reconcile Jean's address ────────────────────────────────────────────
-- Production ran with OWNER_EMAIL=jeanldugas@eatel.net, which is a typo: the
-- real mailbox is jldugas@eatel.net, the address 0003 seeded. Approval mail
-- has therefore been going to a dead address. Correct it here so the flag
-- below lands on a row that can actually receive mail.
--
-- A user row may exist under either address (OTP login self-registers), and
-- users.email is UNIQUE, so the two cases have to be handled separately
-- rather than with a blind UPDATE.
DO $$
DECLARE
    canonical uuid;
    typo      uuid;
BEGIN
    SELECT id INTO canonical FROM users WHERE lower(email) = 'jldugas@eatel.net';
    SELECT id INTO typo      FROM users WHERE lower(email) = 'jeanldugas@eatel.net';

    IF typo IS NULL THEN
        -- Nothing signed in under the typo. Nothing to reconcile.
        NULL;

    ELSIF canonical IS NULL THEN
        -- Only the typo'd row exists: correcting the address is enough.
        UPDATE users SET email = 'jldugas@eatel.net' WHERE id = typo;

    ELSE
        -- Both exist — one person, two accounts. Keep the canonical row and
        -- move everything across before deleting the duplicate; bookings and
        -- messages cascade on user delete, so they must be re-pointed first
        -- or they would be destroyed along with the row.
        UPDATE bookings       SET user_id      = canonical WHERE user_id      = typo;
        UPDATE messages       SET recipient_id = canonical WHERE recipient_id = typo;
        UPDATE messages       SET sender_id    = canonical WHERE sender_id    = typo;
        UPDATE blackout_dates SET created_by   = canonical WHERE created_by   = typo;
        UPDATE special_events SET created_by   = canonical WHERE created_by   = typo;

        -- Keep whichever profile field is actually filled in, and never
        -- demote: if either row was an admin, the survivor is an admin.
        UPDATE users c SET
            full_name     = COALESCE(c.full_name, t.full_name),
            phone         = COALESCE(c.phone, t.phone),
            relationship  = COALESCE(c.relationship, t.relationship),
            boat_info     = COALESCE(c.boat_info, t.boat_info),
            notes         = COALESCE(c.notes, t.notes),
            avatar_url    = COALESCE(c.avatar_url, t.avatar_url),
            role          = CASE WHEN 'admin' IN (c.role, t.role) THEN 'admin' ELSE c.role END,
            -- GREATEST ignores NULLs, so a never-logged-in row can't win.
            last_login_at = GREATEST(c.last_login_at, t.last_login_at)
        FROM users t
        WHERE c.id = canonical AND t.id = typo;

        DELETE FROM users WHERE id = typo;
    END IF;
END $$;

-- Outstanding login codes issued to the dead address are dropped rather than
-- re-pointed: they were mailed to a mailbox nobody reads, so nobody could
-- have received them, and moving them would make them usable for the real
-- account without the code ever having been delivered.
DELETE FROM otp_codes WHERE lower(email) = 'jeanldugas@eatel.net';

-- ── Seed the current owner ──────────────────────────────────────────────
-- Jean, the camp owner. This is the row OWNER_EMAIL was pointing at, spelled
-- correctly. Multiple owners are allowed; more can be flagged from the admin
-- panel's Users tab.
UPDATE users SET is_owner = true WHERE lower(email) = 'jldugas@eatel.net';
