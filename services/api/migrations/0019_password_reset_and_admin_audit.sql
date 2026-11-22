-- Forgot-password and admin password tools.
--
-- 1. otp_codes gains a purpose, so a login code and a password-reset code are
--    different things even though they share a table, a generator and a
--    mailer. DEFAULT 'login' keeps every existing row, and every insert that
--    does not name a purpose, behaving exactly as before.
--
-- 2. otp_codes gains an attempt counter. Until now a code could be guessed at
--    without limit for its whole ten minutes; verification now burns the code
--    after a handful of wrong tries.
--
-- 3. users gains the temporary-password state (must_change_password, expiry)
--    and session_version. There was no way to end a session before its 24h JWT
--    expiry; the token now carries the version it was issued under and a
--    mismatch is a 401. A counter rather than a "valid after" timestamp so
--    there is no same-second race between a password change and the fresh
--    session issued straight after it.
--
-- 4. admin_audit_log: who did a sensitive thing to whose account, and when.
--    No passwords, codes or hashes are ever written to it.
--
-- Forward-only; nothing is backfilled.

ALTER TABLE otp_codes
    ADD COLUMN purpose  text    NOT NULL DEFAULT 'login',
    ADD COLUMN attempts integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT otp_codes_purpose_valid CHECK (purpose IN ('login', 'password_reset'));

ALTER TABLE users
    ADD COLUMN must_change_password     boolean     NOT NULL DEFAULT false,
    ADD COLUMN temp_password_expires_at timestamptz,
    ADD COLUMN session_version          integer     NOT NULL DEFAULT 0;

CREATE TABLE admin_audit_log (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id       uuid REFERENCES users(id) ON DELETE SET NULL,
    target_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    action         text NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT admin_audit_log_action_valid CHECK (
        action IN ('password_temp_set', 'password_removed', 'password_reset_by_user')
    )
);

CREATE INDEX admin_audit_log_target_idx ON admin_audit_log (target_user_id, created_at DESC);
