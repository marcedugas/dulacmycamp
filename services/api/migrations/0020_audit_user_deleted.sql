-- Permanent account deletion by an admin gets an audit row.
--
-- `detail` is a free-text, deliberately non-identifying summary such as
-- "guest account, 3 bookings (1 approved)". Nothing that names the person
-- (email, name, phone, address) is ever written to it — the whole point of
-- the deletion is that none of that survives.
--
-- For 'user_deleted' the target is left NULL: there is no row left to point at.
-- Forward-only; existing rows are unchanged and get a NULL detail.

ALTER TABLE admin_audit_log ADD COLUMN detail text;

ALTER TABLE admin_audit_log DROP CONSTRAINT admin_audit_log_action_valid;
ALTER TABLE admin_audit_log ADD CONSTRAINT admin_audit_log_action_valid CHECK (
    action IN ('password_temp_set', 'password_removed', 'password_reset_by_user', 'user_deleted')
);
