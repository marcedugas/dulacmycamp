-- Lets an admin quietly hide an already-approved journal entry from the
-- public feed without un-approving it or notifying the guest — housekeeping,
-- not moderation. Independent of `status`: an archived entry's status stays
-- 'approved' underneath, so un-archiving loses nothing.
ALTER TABLE journal_entries
    ADD COLUMN archived_at timestamptz;
